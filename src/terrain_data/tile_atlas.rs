use crate::{
    math::{TerrainShape, TileCoordinate},
    plugin::TerrainSettings,
    terrain::TerrainConfig,
    terrain_data::{
        Attachment, AttachmentData, AttachmentLabel, AttachmentTile, AttachmentTileWithData,
        DefaultLoader, TerrainTileDropped, TerrainTileReady, TileTree, TileTreeEntry,
    },
    terrain_view::TerrainViewComponents,
};
use bevy::{
    camera::visibility::{VisibilityClass, add_visibility_class},
    platform::collections::{HashMap, HashSet},
    prelude::*,
    tasks::Task,
};
#[cfg(feature = "big_space")]
use big_space::prelude::CellCoord;
use std::collections::VecDeque;

/// The current state of a tile of a [`TileAtlas`].
///
/// This indicates, whether the tile is loading or loaded and ready to be used.
#[derive(Clone, Copy, Debug)]
enum LoadingState {
    /// The tile is loading, but can not be used yet.
    Loading(u32),
    /// The tile is loaded and can be used.
    Loaded,
}

/// The internal representation of a present tile in a [`TileAtlas`].
struct TileState {
    /// Indicates whether or not the tile is loading or loaded.
    state: LoadingState,
    /// The index of the tile inside the atlas.
    atlas_index: u32,
    /// The count of [`TileTrees`] that have requested this tile.
    requests: u32,
}

// Todo: rename to terrain?
// Todo: consider turning this into an asset

/// A sparse storage of all terrain attachments, which streams data in and out of memory
/// depending on the decisions of the corresponding [`TileTree`]s.
///
/// A tile is considered present and assigned an [`u32`] as soon as it is
/// requested by any tile_tree. Then the tile atlas will start loading all of its attachments
/// by storing the [`TileCoordinate`] (for one frame) in `load_events` for which
/// attachment-loading-systems can listen.
/// Tiles that are not being used by any tile_tree anymore are cached (LRU),
/// until new atlas indices are required.
///
/// The [`u32`] can be used for accessing the attached data in systems by the CPU
/// and in shaders by the GPU.
#[derive(Component)]
#[require(Transform, Visibility, VisibilityClass, DefaultLoader)]
#[cfg_attr(feature = "big_space", require(CellCoord))]
#[component(on_add = add_visibility_class::<TileAtlas>)]
pub struct TileAtlas {
    pub(crate) attachments: HashMap<AttachmentLabel, Attachment>, // stores the attachment data
    tile_states: HashMap<TileCoordinate, TileState>,
    unused_indices: VecDeque<u32>,
    existing_tiles: HashSet<TileCoordinate>,
    pub(crate) uploading_tiles: Vec<AttachmentTileWithData>,
    pub(crate) downloading_tiles: Vec<Task<AttachmentTileWithData>>,
    pub(crate) to_load: Vec<AttachmentTile>,
    /// Tiles that became resident this update, drained into [`TerrainTileReady`] messages.
    ready_tiles: Vec<(TileCoordinate, u32)>,
    /// Ready tiles that were evicted this update, drained into [`TerrainTileDropped`] messages.
    dropped_tiles: Vec<(TileCoordinate, u32)>,

    pub(crate) lod_count: u32,
    pub(crate) min_height: f32,
    pub(crate) max_height: f32,
    pub(crate) height_scale: f32,
    pub(crate) shape: TerrainShape,
}

impl TileAtlas {
    /// Creates a new tile_tree from a terrain config.
    pub fn new(config: &TerrainConfig, settings: &TerrainSettings) -> Self {
        let attachments = config
            .attachments
            .iter()
            .map(|(label, attachment)| (label.clone(), Attachment::new(attachment, &config.path)))
            .collect();

        Self {
            attachments,
            tile_states: default(),
            unused_indices: (0..settings.atlas_size).collect(),
            existing_tiles: HashSet::from_iter(config.tiles.clone()),
            to_load: default(),
            uploading_tiles: default(),
            downloading_tiles: default(),
            ready_tiles: default(),
            dropped_tiles: default(),
            lod_count: config.lod_count,
            min_height: config.min_height,
            max_height: config.max_height,
            height_scale: 1.0,
            shape: config.shape,
        }
    }

    pub(crate) fn get_best_tile(&self, tile_coordinate: TileCoordinate) -> TileTreeEntry {
        let mut best_tile_coordinate = tile_coordinate;

        if !self.existing_tiles.contains(&tile_coordinate) {
            return TileTreeEntry::default();
        }

        loop {
            if best_tile_coordinate == TileCoordinate::INVALID {
                // highest lod is not loaded
                return TileTreeEntry::default();
            }

            if let Some(tile) = self.tile_states.get(&best_tile_coordinate) {
                if matches!(tile.state, LoadingState::Loaded) {
                    // found best loaded tile
                    return TileTreeEntry {
                        atlas_index: tile.atlas_index,
                        atlas_lod: best_tile_coordinate.lod,
                    };
                }
            }

            best_tile_coordinate = best_tile_coordinate
                .parent()
                .unwrap_or(TileCoordinate::INVALID);
        }
    }

    pub(crate) fn tile_loaded(&mut self, tile: AttachmentTile, data: AttachmentData) {
        if let Some(tile_state) = self.tile_states.get_mut(&tile.coordinate) {
            // The last outstanding attachment flips the tile to `Loaded`; that is the
            // once-per-tile edge at which the tile becomes sampleable.
            let became_ready = matches!(tile_state.state, LoadingState::Loading(1));

            tile_state.state = match tile_state.state {
                LoadingState::Loading(1) => LoadingState::Loaded,
                LoadingState::Loading(n) => LoadingState::Loading(n - 1),
                LoadingState::Loaded => {
                    panic!("Loaded more attachments, than registered with the tile atlas.")
                }
            };

            self.uploading_tiles.push(AttachmentTileWithData {
                atlas_index: tile_state.atlas_index,
                label: tile.label,
                data,
            });

            if became_ready {
                self.ready_tiles
                    .push((tile.coordinate, tile_state.atlas_index));
            }
        } else {
            dbg!("Tile is no longer loaded.");
        }
    }

    /// Drains the pending residency changes into [`TerrainTileReady`] / [`TerrainTileDropped`]
    /// messages. Runs after [`Self::update`] so both edges of the frame are observed.
    pub(crate) fn emit_tile_events(
        mut tile_atlases: Query<(Entity, &mut TileAtlas)>,
        mut ready_messages: MessageWriter<TerrainTileReady>,
        mut dropped_messages: MessageWriter<TerrainTileDropped>,
    ) {
        for (terrain, mut tile_atlas) in &mut tile_atlases {
            let TileAtlas {
                ready_tiles,
                dropped_tiles,
                ..
            } = &mut *tile_atlas;

            ready_messages.write_batch(ready_tiles.drain(..).map(|(coordinate, atlas_index)| {
                TerrainTileReady {
                    terrain,
                    coordinate,
                    atlas_index,
                }
            }));

            dropped_messages.write_batch(dropped_tiles.drain(..).map(
                |(coordinate, atlas_index)| TerrainTileDropped {
                    terrain,
                    coordinate,
                    atlas_index,
                },
            ));
        }
    }

    /// Updates the tile atlas according to all corresponding tile_trees.
    pub(crate) fn update(
        mut tile_trees: ResMut<TerrainViewComponents<TileTree>>,
        mut tile_atlases: Query<&mut TileAtlas>,
    ) {
        for (&(terrain, _view), tile_tree) in tile_trees.iter_mut() {
            let mut tile_atlas = tile_atlases.get_mut(terrain).unwrap();

            for tile_coordinate in tile_tree.released_tiles.drain(..) {
                tile_atlas.release_tile(tile_coordinate);
            }

            for tile_coordinate in tile_tree.requested_tiles.drain(..) {
                tile_atlas.request_tile(tile_coordinate);
            }
        }
    }

    fn request_tile(&mut self, tile_coordinate: TileCoordinate) {
        if !self.existing_tiles.contains(&tile_coordinate) {
            return;
        }

        // check if the tile is already present else start loading it
        if let Some(tile) = self.tile_states.get_mut(&tile_coordinate) {
            if tile.requests == 0 {
                // the tile is now used again
                self.unused_indices
                    .retain(|&atlas_index| tile.atlas_index != atlas_index);
            }

            tile.requests += 1;
        } else {
            let atlas_index = self
                .unused_indices
                .pop_front()
                .expect("Atlas out of indices");

            // Remove the tile still cached in this slot. If it was ready, report it
            // dropped in the same pass so its data is no longer considered available.
            let Self {
                tile_states,
                dropped_tiles,
                ..
            } = self;
            tile_states.retain(|&coordinate, tile| {
                if tile.atlas_index != atlas_index {
                    return true;
                }
                if matches!(tile.state, LoadingState::Loaded) {
                    dropped_tiles.push((coordinate, atlas_index));
                }
                false
            });

            self.tile_states.insert(
                tile_coordinate,
                TileState {
                    requests: 1,
                    state: LoadingState::Loading(self.attachments.len() as u32),
                    atlas_index,
                },
            );

            for label in self.attachments.keys() {
                self.to_load.push(AttachmentTile {
                    coordinate: tile_coordinate,
                    label: label.clone(),
                });
            }
        }
    }

    fn release_tile(&mut self, tile_coordinate: TileCoordinate) {
        if !self.existing_tiles.contains(&tile_coordinate) {
            return;
        }

        let tile = self.tile_states.get_mut(&tile_coordinate).unwrap();
        tile.requests -= 1;

        if tile.requests == 0 {
            self.unused_indices.push_back(tile.atlas_index);

            // Todo: we should cancel loading tiles, that have not yet started loading and a no longer requested
        }
    }
}
