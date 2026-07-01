#import bevy_terrain::types::AtlasTile
#import bevy_terrain::attachments::{sample_height_mask, sample_surface_gradient}
#import bevy_terrain::fragment::{FragmentInput, FragmentOutput, fragment_info, fragment_output, fragment_debug}
#import bevy_terrain::functions::lookup_tile

@fragment
fn fragment(input: FragmentInput) -> FragmentOutput {
    var info = fragment_info(input);

    let tile             = lookup_tile(info.coordinate, info.blend);
    let mask             = sample_height_mask(tile);
    let color            = vec4<f32>(0.5);
    let surface_gradient = sample_surface_gradient(tile, info.tangent_space);

    if mask { discard; }

    var output: FragmentOutput;
    fragment_output(&info, &output, color, surface_gradient);
    fragment_debug(&info, &output, tile, surface_gradient);
    return FragmentOutput(vec4<f32>(output.color.xyz, 1.0));
}
