@group(0) @binding(0) var boats: texture_2d<f32>;
struct BoatTarget {
    clip_scale_offset: vec4<f32>,
    origin_and_size_px: vec4<f32>,
};
@group(0) @binding(1) var<uniform> boat_target: BoatTarget;

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let xy = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(xy[index], 0.0, 1.0);
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(boats, vec2<i32>(position.xy - boat_target.origin_and_size_px.xy), 0);
}
