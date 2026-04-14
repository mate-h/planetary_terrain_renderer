@group(0) @binding(0) var<uniform> index: u32;
@group(0) @binding(1) var parent_texture: texture_2d_array<f32>;

#ifdef RGB8U
@group(0) @binding(2) var child_texture: texture_storage_2d_array<rgba8unorm, write>;
#else ifdef RGBA8U 
@group(0) @binding(2) var child_texture: texture_storage_2d_array<rgba8unorm, write>;
#else ifdef R16U
@group(0) @binding(2) var child_texture: texture_storage_2d_array<r16uint, write>;
#else ifdef R16I
@group(0) @binding(2) var child_texture: texture_storage_2d_array<r16sint, write>;
#else ifdef Rg16U
@group(0) @binding(2) var child_texture: texture_storage_2d_array<rg16uint, write>;
#else ifdef R32F
@group(0) @binding(2) var child_texture: texture_storage_2d_array<r32float, write>;
#endif

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) invocation_id: vec3<u32>) {
    let coord = invocation_id.xy;

    let data00 = textureLoad(parent_texture, 2 * coord + vec2<u32>(0, 0), index, 0);
    let data01 = textureLoad(parent_texture, 2 * coord + vec2<u32>(0, 1), index, 0);
    let data10 = textureLoad(parent_texture, 2 * coord + vec2<u32>(1, 0), index, 0);
    let data11 = textureLoad(parent_texture, 2 * coord + vec2<u32>(1, 1), index, 0);

#ifdef R32F
    let data = 0.25 * (data00.x + data01.x + data10.x + data11.x);
    textureStore(child_texture, coord, index, vec4<f32>(data, 0.0, 0.0, 0.0));
#else ifdef R16U
    let data = 0.25 * (data00.x + data01.x + data10.x + data11.x);
    textureStore(child_texture, coord, index, vec4<u32>(u32(data * 65535.0), 0u, 0u, 0u));
#else ifdef R16I
    let data = 0.25 * (data00.x + data01.x + data10.x + data11.x);
    textureStore(child_texture, coord, index, vec4<i32>(i32(data * 32767.0), 0, 0, 0));
#else ifdef Rg16U
    let data = 0.25 * (data00.xy + data01.xy + data10.xy + data11.xy);
    textureStore(child_texture, coord, index, vec4<u32>(u32(data.x * 65535.0), u32(data.y * 65535.0), 0u, 0u));
#else
    let data = 0.25 * (data00 + data01 + data10 + data11);
    textureStore(child_texture, coord, index, data);
#endif
}
