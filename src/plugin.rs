use crate::{
    floating_origin::register_plugins,
    formats::{TerrainConfigLoader, TiffLoader},
    preprocess::{MipPipelines, mip_prepass},
    render::{
        DepthCopyPipeline, GpuTerrain, GpuTerrainShadow, GpuTerrainView, TerrainItem,
        TerrainShadowPipelines, TerrainTilingPrepassPipelines, TerrainUniform, TilingPrepassItem,
        extract_terrain_phases, extract_terrain_uniform, prepare_terrain_depth_textures,
        queue_tiling_prepass, terrain_pass, terrain_shadow_pass, tiling_prepass,
    },
    shaders::{InternalShaders, load_terrain_shaders},
    terrain::{TerrainComponents, TerrainConfig},
    terrain_data::{
        AttachmentLabel, GpuTileAtlas, TerrainTileDropped, TerrainTileReady, TileAtlas, TileTree,
        finish_loading, start_loading,
    },
    terrain_shadow::{
        TerrainShadowSettings, TerrainShadowUniform, extract_terrain_shadow, update_terrain_shadow,
    },
    terrain_view::TerrainViewComponents,
};
use bevy::transform::TransformSystems;
use bevy::{
    core_pipeline::{
        core_3d::main_opaque_pass_3d,
        schedule::{Core3d, Core3dSystems, camera_driver},
    },
    prelude::*,
    render::{
        Render, RenderApp, RenderSystems,
        render_phase::{DrawFunctions, ViewSortedRenderPhases, sort_phase_system},
        render_resource::*,
        renderer::RenderGraph,
    },
};

#[derive(Resource)]
pub struct TerrainSettings {
    pub attachments: Vec<AttachmentLabel>,
    pub atlas_size: u32,
}

impl Default for TerrainSettings {
    fn default() -> Self {
        Self {
            attachments: vec![AttachmentLabel::Height],
            atlas_size: 1028,
        }
    }
}

impl TerrainSettings {
    pub fn new(custom_attachments: Vec<&str>) -> Self {
        let mut attachments = vec![AttachmentLabel::Height];
        attachments.extend(
            custom_attachments
                .into_iter()
                .map(|name| AttachmentLabel::Custom(name.to_string())),
        );

        Self {
            attachments,
            atlas_size: 1028,
        }
    }
}

/// The plugin for the terrain renderer.
pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        register_plugins(app);

        app.init_asset::<TerrainConfig>()
            .init_resource::<InternalShaders>()
            .init_resource::<TerrainViewComponents<TileTree>>()
            .init_resource::<TerrainSettings>()
            .init_resource::<TerrainShadowSettings>()
            .init_resource::<TerrainViewComponents<TerrainShadowUniform>>()
            .init_asset_loader::<TerrainConfigLoader>()
            .init_asset_loader::<TiffLoader>()
            .add_message::<TerrainTileReady>()
            .add_message::<TerrainTileDropped>()
            .add_systems(
                PostUpdate,
                (
                    // Todo: enable visibility checking again
                    // check_visibility::<With<TileAtlas>>.in_set(VisibilitySystems::CheckVisibility),
                    (
                        TileTree::compute_requests,
                        finish_loading,
                        TileAtlas::update,
                        TileAtlas::emit_tile_events,
                        start_loading,
                        TileTree::adjust_to_tile_atlas,
                        TileTree::generate_surface_approximation,
                        TileTree::update_terrain_view_buffer,
                        update_terrain_shadow,
                    )
                        .chain()
                        .after(TransformSystems::Propagate),
                ),
            );
        app.sub_app_mut(RenderApp)
            .init_resource::<SpecializedComputePipelines<MipPipelines>>()
            .init_resource::<SpecializedComputePipelines<TerrainTilingPrepassPipelines>>()
            .init_resource::<SpecializedComputePipelines<TerrainShadowPipelines>>()
            .init_resource::<TerrainComponents<GpuTileAtlas>>()
            .init_resource::<TerrainComponents<GpuTerrain>>()
            .init_resource::<TerrainComponents<TerrainUniform>>()
            .init_resource::<TerrainViewComponents<GpuTerrainView>>()
            .init_resource::<TerrainViewComponents<GpuTerrainShadow>>()
            .init_resource::<TerrainViewComponents<TilingPrepassItem>>()
            .init_resource::<TerrainShadowSettings>()
            .init_resource::<TerrainViewComponents<TerrainShadowUniform>>()
            .init_resource::<DrawFunctions<TerrainItem>>()
            .init_resource::<ViewSortedRenderPhases<TerrainItem>>()
            .add_systems(
                ExtractSchedule,
                (
                    extract_terrain_phases,
                    extract_terrain_shadow,
                    extract_terrain_uniform,
                    GpuTileAtlas::initialize,
                    GpuTileAtlas::extract.after(GpuTileAtlas::initialize),
                    GpuTerrain::initialize.after(GpuTileAtlas::initialize),
                    GpuTerrainView::initialize,
                    GpuTerrainShadow::initialize,
                ),
            )
            .add_systems(
                Render,
                (
                    (
                        GpuTileAtlas::prepare,
                        GpuTerrain::prepare,
                        GpuTerrainShadow::prepare,
                        GpuTerrainView::prepare_terrain_view,
                        GpuTerrainView::prepare_indirect,
                        GpuTerrainView::prepare_refine_tiles,
                    )
                        .in_set(RenderSystems::PrepareBindGroups),
                    sort_phase_system::<TerrainItem>.in_set(RenderSystems::PhaseSort),
                    prepare_terrain_depth_textures.in_set(RenderSystems::PrepareResources),
                    (
                        queue_tiling_prepass,
                        GpuTileAtlas::queue,
                        GpuTerrainShadow::queue,
                    )
                        .in_set(RenderSystems::Queue),
                    GpuTileAtlas::_cleanup.in_set(RenderSystems::Cleanup),
                ),
            )
            .add_systems(
                RenderGraph,
                (mip_prepass, tiling_prepass, terrain_shadow_pass)
                    .chain()
                    .before(camera_driver),
            )
            .add_systems(
                Core3d,
                terrain_pass
                    .before(main_opaque_pass_3d)
                    .in_set(Core3dSystems::MainPass),
            );
    }

    fn finish(&self, app: &mut App) {
        let attachments = app
            .world()
            .resource::<TerrainSettings>()
            .attachments
            .clone();

        load_terrain_shaders(app, &attachments);

        app.sub_app_mut(RenderApp)
            .init_resource::<TerrainTilingPrepassPipelines>()
            .init_resource::<TerrainShadowPipelines>()
            .init_resource::<MipPipelines>()
            .init_resource::<DepthCopyPipeline>();
    }
}
