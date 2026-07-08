#import bevy_terrain::types::{AtlasTile}
#import bevy_terrain::bindings::{terrain_data, terrain_view, height_attachment, terrain_sampler}
#import bevy_terrain::attachments::{sample_height, sample_height_mask, sample_surface_gradient, relief_shading}
#import bevy_terrain::fragment::{FragmentInput, FragmentOutput, fragment_info, fragment_output, fragment_debug}
#import bevy_terrain::functions::{lookup_tile, inverse_mix}

struct GradientInfo {
    mode: u32,
}

@group(4) @binding(0)
var gradient: texture_2d<f32>;
@group(4) @binding(1)
var gradient_sampler: sampler;
@group(4) @binding(2)
var<uniform> gradient_info: GradientInfo;

fn color_dataset(tile: AtlasTile) -> vec4<f32> {
    let height = sample_height(tile);

    return textureSampleLevel(
        gradient,
        gradient_sampler,
        vec2<f32>(inverse_mix(terrain_data.terrain.min_height, terrain_data.terrain.max_height, height), 0.5),
        0.0,
    );
}

fn color_earth(tile: AtlasTile) -> vec4<f32> {
    let height = sample_height(tile);

    if (height < 0.0) {
        return textureSampleLevel(
            gradient,
            gradient_sampler,
            vec2<f32>(mix(0.0, 0.075, pow(height / terrain_data.terrain.min_height, 0.25)), 0.5),
            0.0,
        );
    }

    return textureSampleLevel(
        gradient,
        gradient_sampler,
        vec2<f32>(mix(0.09, 0.6, pow(height / terrain_data.terrain.max_height * 1.4, 1.0)), 0.5),
        0.0,
    );
}

fn sample_color(tile: AtlasTile) -> vec4<f32> {
    switch (gradient_info.mode) {
        case 0u: { return color_dataset(tile); }
        case 1u: { return color_earth(tile);   }
        default: { return color_dataset(tile); }
    }
}

@fragment
fn fragment(input: FragmentInput) -> FragmentOutput {
    var info = fragment_info(input);

    let tile             = lookup_tile(info.coordinate, info.blend);
    let mask             = sample_height_mask(tile);
    var color            = sample_color(tile);
    var surface_gradient = sample_surface_gradient(tile, info.tangent_space);

    if (mask) { discard; }

    var output: FragmentOutput;
#ifdef LIGHTING
    output.color = color * relief_shading(info.world_coordinate, surface_gradient);
#else
    output.color = color;
#endif

    fragment_output(&info, &output, color, surface_gradient);
    fragment_debug(&info, &output, tile, surface_gradient);
    return output;
}
