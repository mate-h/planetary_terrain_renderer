#import bevy_terrain::types::AtlasTile
#import bevy_terrain::bindings::{terrain_data, landcover_attachment, albedo_attachment, terrain_sampler}
#import bevy_terrain::attachments::{compute_sample_uv, sample_height_mask, sample_surface_gradient}
#import bevy_terrain::fragment::{FragmentInput, FragmentOutput, fragment_info, fragment_output, fragment_debug}
#import bevy_terrain::functions::lookup_tile

struct CaliforniaMaterialSettings {
    show_landcover: u32,
    show_albedo: u32,
}

@group(4) @binding(0)
var<uniform> material_settings: CaliforniaMaterialSettings;

fn sample_landcover(tile: AtlasTile) -> vec4<f32> {
    let uv = compute_sample_uv(tile, terrain_data.attachments.landcover);

#ifdef SAMPLE_GRAD
    return textureSampleGrad(landcover_attachment, terrain_sampler, uv.uv, tile.index, uv.dx, uv.dy);
#else
    return textureSampleLevel(landcover_attachment, terrain_sampler, uv.uv, tile.index, tile.blend_ratio);
#endif
}

fn sample_albedo(tile: AtlasTile) -> vec4<f32> {
    let uv = compute_sample_uv(tile, terrain_data.attachments.albedo);

#ifdef SAMPLE_GRAD
    return textureSampleGrad(albedo_attachment, terrain_sampler, uv.uv, tile.index, uv.dx, uv.dy);
#else
    return textureSampleLevel(albedo_attachment, terrain_sampler, uv.uv, tile.index, tile.blend_ratio);
#endif
}

@fragment
fn fragment(input: FragmentInput) -> FragmentOutput {
    var info = fragment_info(input);

    let tile             = lookup_tile(info.coordinate, info.blend);
    let mask             = sample_height_mask(tile);
    var color            = vec4<f32>(0.5);
    let surface_gradient = sample_surface_gradient(tile, info.tangent_space);

    if (material_settings.show_albedo != 0u) {
        color = sample_albedo(tile);
    } else if (material_settings.show_landcover != 0u) {
        color = sample_landcover(tile);
    }

    if (color.a < 0.01) {
        color = vec4<f32>(0.5);
    }

    if mask { discard; }

    var output: FragmentOutput;
    fragment_output(&info, &output, color, surface_gradient);
    fragment_debug(&info, &output, tile, surface_gradient);
    return FragmentOutput(vec4<f32>(output.color.xyz, 1.0));
}
