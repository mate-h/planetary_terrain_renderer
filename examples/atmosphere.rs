//! Atmosphere example with California terrain, based on `bevy/examples/3d/atmosphere.rs`.
//!
//! Run from this crate root:
//! `cargo run --example atmosphere --features atmosphere_example`

use bevy::{
    asset::io::AssetSourceBuilder,
    camera::{Exposure, Hdr},
    camera_controller::free_camera::{FreeCamera, FreeCameraPlugin, UpAxis},
    core_pipeline::tonemapping::{GranTurismo7Params, Tonemapping},
    input::keyboard::KeyCode,
    light::{
        Atmosphere, AtmosphereEnvironmentMapLight, SunDisk, VolumetricLight,
        atmosphere::ScatteringMedium, light_consts::lux,
    },
    math::{DVec2, DVec3},
    pbr::{AtmosphereMode, AtmosphereSettings},
    post_process::bloom::Bloom,
    prelude::*,
    reflect::TypePath,
    render::render_resource::AsBindGroup,
    shader::ShaderRef,
    window::{DisplayTarget, PrimaryWindow},
};
use bevy_terrain::{math::Coordinate, prelude::*};
use big_space::prelude::Grid;
use std::f32::consts::PI;

#[path = "helpers/hdr.rs"]
mod hdr;

const EARTH_MINOR_RADIUS: f32 = 6356752.314245;
const EARTH_ATMOSPHERE_SHELL: f32 = 100_000.0;

/// Lowest elevation in `source_data/california.tif` (near Death Valley).
const CALIFORNIA_MIN_HEIGHT: f32 = -8.381_328_6;
const CALIFORNIA_MIN_LAT: f64 = 36.637_500;
const CALIFORNIA_MIN_LON: f64 = -117.159_259;
/// Highest elevation in the preprocessed california height tiles.
const CALIFORNIA_MAX_HEIGHT: f32 = 4416.048_340;

/// California tiles in `config.tc.ron` live on cube face 4 (−Z normal) at lod 11.
const CALIFORNIA_FACE: u32 = 4;
const CALIFORNIA_LOD: u32 = 11;
const CALIFORNIA_TILE_XY: IVec2 = IVec2::new(91, 1695);
const CALIFORNIA_MEAN_HEIGHT: f32 = (CALIFORNIA_MIN_HEIGHT + CALIFORNIA_MAX_HEIGHT) * 0.5;
/// Lod-11 load radius is ~29 km; stay within that so tiles stream in.
const CALIFORNIA_CAMERA_ALTITUDE: f64 = 15_000.0;

/// Matches the lon/lat convention in `preprocess/src/transformers.rs`.
fn lat_lon_to_unit_position(lat_deg: f64, lon_deg: f64) -> DVec3 {
    let lat = lat_deg.to_radians();
    let lon = lon_deg.to_radians();
    DVec3::new(-lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
}

/// Geocentric distance from the planet center at a surface point with elevation `height`.
fn geocentric_radius_at_elevation(
    shape: TerrainShape,
    lat_deg: f64,
    lon_deg: f64,
    height: f32,
) -> f32 {
    let unit = lat_lon_to_unit_position(lat_deg, lon_deg);
    let coordinate = Coordinate::from_unit_position(unit, true);
    coordinate.local_position(shape, height).length() as f32
}

fn california_atmosphere_radii() -> (f32, f32) {
    let inner_radius = geocentric_radius_at_elevation(
        TerrainShape::WGS84,
        CALIFORNIA_MIN_LAT,
        CALIFORNIA_MIN_LON,
        CALIFORNIA_MIN_HEIGHT,
    );
    (inner_radius, inner_radius + EARTH_ATMOSPHERE_SHELL)
}

fn california_coordinate() -> Coordinate {
    let tile_count = 2f64.powi(CALIFORNIA_LOD as i32);
    let uv = (CALIFORNIA_TILE_XY.as_dvec2() + DVec2::splat(0.5)) / tile_count;
    Coordinate::new(CALIFORNIA_FACE, uv)
}

fn california_surface_normal() -> Dir3 {
    let unit = california_coordinate().unit_position(true);
    Dir3::new_unchecked((TerrainShape::WGS84.scale() * unit).normalize().as_vec3())
}

fn california_surface_point() -> DVec3 {
    california_coordinate().local_position(TerrainShape::WGS84, CALIFORNIA_MEAN_HEIGHT)
}

#[derive(Resource)]
struct GameState {
    paused: bool,
    /// When `true`, `HdrPlugin` auto-selects the best HDR transfer; when `false`, output is pinned to SDR.
    hdr_output_enabled: bool,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            paused: false,
            hdr_output_enabled: true,
        }
    }
}

#[derive(Resource)]
struct AtmospherePresets {
    earth: Handle<ScatteringMedium>,
    mars: Handle<ScatteringMedium>,
}

#[derive(Asset, AsBindGroup, TypePath, Clone, Default)]
struct CaliforniaMaterial {}

impl Material for CaliforniaMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/atmosphere.wgsl".into()
    }
}

fn main() {
    App::new()
        .register_asset_source(
            "bevy",
            AssetSourceBuilder::platform_default("../bevy/assets", None),
        )
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(GameState::default())
        .insert_resource(GlobalAmbientLight::NONE)
        .insert_resource(TerrainSettings::default())
        .add_plugins((
            DefaultPlugins
                .build()
                .set(bevy::render::RenderPlugin {
                    // Rec.2020 is GT7's native working space; pairs with HDR output on XDR displays.
                    working_color_space:
                        bevy::render::working_color_space::WorkingColorSpace::Rec2020,
                    ..default()
                })
                .disable::<TransformPlugin>(),
            FreeCameraPlugin,
            TerrainPlugin,
            TerrainMaterialPlugin::<CaliforniaMaterial>::default(),
        ))
        .add_plugins(hdr::HdrPlugin::default())
        .add_systems(Startup, (setup_hdr_display, setup_scene, print_controls))
        .add_systems(Update, (dynamic_scene, atmosphere_controls))
        .run();
}

fn print_controls() {
    println!("Atmosphere + California Terrain Controls:");
    println!("    1          - Switch to lookup texture rendering method");
    println!("    2          - Switch to raymarched rendering method");
    println!("    3          - Switch to Earth atmosphere");
    println!("    4          - Switch to Mars atmosphere");
    println!("    Enter      - Pause/Resume sun motion");
    println!("    H          - Toggle HDR display output on/off");
    println!("    Up/Down    - Increase/Decrease exposure");
    println!("    WASD       - Move camera (FreeCamera)");
}

fn setup_hdr_display(
    mut display_target: Single<&mut DisplayTarget, With<PrimaryWindow>>,
    mut hdr_preference: ResMut<hdr::HdrPreference>,
    mut game_state: ResMut<GameState>,
) {
    if std::env::var("CI_TESTING_CONFIG").is_ok() {
        hdr_preference.manual_override = true;
        **display_target = DisplayTarget::SDR_SRGB;
        game_state.hdr_output_enabled = false;
        return;
    }
    display_target.paper_white_nits = 200.0;
    display_target.peak_luminance_nits = 1000.0;
}

fn atmosphere_controls(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut planet_atmosphere: Query<(&mut Atmosphere, &mut GlobalTransform)>,
    mut camera_settings: Query<&mut AtmosphereSettings, With<Camera3d>>,
    mut sun_disks: Query<&mut SunDisk, With<DirectionalLight>>,
    atmosphere_presets: Res<AtmospherePresets>,
    mut game_state: ResMut<GameState>,
    mut camera_exposure: Query<&mut Exposure, With<Camera3d>>,
    mut display_target: Single<&mut DisplayTarget, With<PrimaryWindow>>,
    mut hdr_preference: ResMut<hdr::HdrPreference>,
    time: Res<Time>,
) {
    if keyboard_input.just_pressed(KeyCode::Digit3) {
        let (inner_radius, outer_radius) = california_atmosphere_radii();
        for (mut atmosphere, mut transform) in &mut planet_atmosphere {
            *atmosphere = Atmosphere::earth(atmosphere_presets.earth.clone());
            atmosphere.inner_radius = inner_radius;
            atmosphere.outer_radius = outer_radius;
            *transform = GlobalTransform::from_translation(-Vec3::Y * atmosphere.inner_radius);
        }
        for mut sun_disk in &mut sun_disks {
            sun_disk.angular_size = SunDisk::EARTH.angular_size;
        }
        println!("Switched to Earth atmosphere");
    }

    if keyboard_input.just_pressed(KeyCode::Digit4) {
        for (mut atmosphere, mut transform) in &mut planet_atmosphere {
            *atmosphere = Atmosphere::mars(atmosphere_presets.mars.clone());
            *transform = GlobalTransform::from_translation(-Vec3::Y * atmosphere.inner_radius);
        }
        for mut sun_disk in &mut sun_disks {
            sun_disk.angular_size = SunDisk::MARS.angular_size;
        }
        println!("Switched to Mars atmosphere");
    }

    if keyboard_input.just_pressed(KeyCode::Digit1) {
        for mut settings in &mut camera_settings {
            settings.rendering_method = AtmosphereMode::LookupTexture;
            println!("Switched to lookup texture rendering method");
        }
    }

    if keyboard_input.just_pressed(KeyCode::Digit2) {
        for mut settings in &mut camera_settings {
            settings.rendering_method = AtmosphereMode::Raymarched;
            println!("Switched to raymarched rendering method");
        }
    }

    if keyboard_input.just_pressed(KeyCode::Enter) {
        game_state.paused = !game_state.paused;
    }

    if keyboard_input.just_pressed(KeyCode::KeyH) {
        game_state.hdr_output_enabled = !game_state.hdr_output_enabled;
        if game_state.hdr_output_enabled {
            hdr_preference.manual_override = false;
            display_target.paper_white_nits = 200.0;
            display_target.peak_luminance_nits = 1000.0;
            println!("HDR display output: on (auto-select transfer)");
        } else {
            hdr_preference.manual_override = true;
            **display_target = DisplayTarget::SDR_SRGB;
            println!("HDR display output: off (SDR sRGB)");
        }
    }

    if keyboard_input.pressed(KeyCode::ArrowUp) {
        for mut exposure in &mut camera_exposure {
            exposure.ev100 -= time.delta_secs() * 2.0;
        }
    }

    if keyboard_input.pressed(KeyCode::ArrowDown) {
        for mut exposure in &mut camera_exposure {
            exposure.ev100 += time.delta_secs() * 2.0;
        }
    }
}

fn setup_scene(
    mut commands: Commands,
    mut scattering_mediums: ResMut<Assets<ScatteringMedium>>,
    asset_server: Res<AssetServer>,
) {
    let earth_medium = scattering_mediums.add(ScatteringMedium::earth(256, 256));
    let mars_phase = asset_server.load("bevy://textures/mars_mie_phase.ktx2");
    let mars_medium = scattering_mediums.add(ScatteringMedium::mars(256, 256, mars_phase));

    commands.insert_resource(AtmospherePresets {
        earth: earth_medium.clone(),
        mars: mars_medium.clone(),
    });

    let mut atmosphere = Atmosphere::earth(earth_medium);
    let (inner_radius, outer_radius) = california_atmosphere_radii();
    atmosphere.inner_radius = inner_radius;
    atmosphere.outer_radius = outer_radius;
    println!(
        "California terrain bounds from source_data/california.tif: \
         min={CALIFORNIA_MIN_HEIGHT:.3} m at ({CALIFORNIA_MIN_LAT:.6}°, {CALIFORNIA_MIN_LON:.6}°), \
         max={CALIFORNIA_MAX_HEIGHT:.3} m"
    );
    println!(
        "Atmosphere inner radius at lowest terrain point: {inner_radius:.3} m \
         (WGS84 minor axis reference: {EARTH_MINOR_RADIUS:.3} m)"
    );

    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            contact_shadows_enabled: true,
            illuminance: lux::RAW_SUNLIGHT,
            ..default()
        },
        Transform::from_xyz(1.0, 0.4, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
        VolumetricLight,
        SunDisk::EARTH,
    ));

    let mut view = Entity::PLACEHOLDER;

    commands.spawn_big_space(Grid::default(), |root| {
        root.spawn((
            Transform::IDENTITY,
            CellCoord::default(),
            GlobalTransform::from_translation(Vec3::Z),
            atmosphere,
        ));

        let surface = california_surface_point();
        let up = california_surface_normal();
        let camera_position = surface + up.as_vec3().as_dvec3() * CALIFORNIA_CAMERA_ALTITUDE;
        let look_dir = (surface.as_vec3() - camera_position.as_vec3()).normalize();
        let grid = Grid::default();
        let (cell, local_translation) =
            grid.imprecise_translation_to_grid(camera_position.as_vec3());

        view = root
            .spawn_spatial((
                cell,
                Transform::from_translation(local_translation).looking_to(look_dir, up.as_vec3()),
                Camera3d::default(),
                Hdr,
                Tonemapping::GranTurismo7,
                GranTurismo7Params::default(),
                Exposure { ev100: 13.0 },
                Bloom::NATURAL,
                AtmosphereSettings {
                    rendering_method: AtmosphereMode::Raymarched,
                    ..default()
                },
                FreeCamera {
                    walk_speed: 2500.0,
                    run_speed: 10000.0,
                    up_axis: UpAxis::Fixed(up),
                    ..default()
                },
                FloatingOrigin,
                AtmosphereEnvironmentMapLight::default(),
            ))
            .id();
    });

    commands.spawn_terrain(
        asset_server.load("terrains/california/config.tc.ron"),
        TerrainViewConfig {
            view_lod: CALIFORNIA_LOD,
            ..default()
        },
        CaliforniaMaterial::default(),
        view,
    );
}

fn dynamic_scene(
    mut suns: Query<&mut Transform, With<DirectionalLight>>,
    time: Res<Time>,
    sun_motion_state: Res<GameState>,
) {
    if !sun_motion_state.paused {
        suns.iter_mut()
            .for_each(|mut tf| tf.rotate_x(-time.delta_secs() * PI / 10.0));
    }
}
