// The depth of the first surface each pixel's eye ray meets, which
// `vertex_terrain_depth` rasterizes; cleared to sea level.
@group(0) @binding(8)
var terrain_depth: texture_depth_2d;

// The scale 1 − z / H of the offset from the view centre at which the eye ray
// through the sea-level point `view_offset_m` meets the terrain.
fn terrain_hit_scale(view_offset_m: vec2<f32>) -> f32 {
    let view = vec2<f32>(0.5) + vec2<f32>(1.0, -1.0) * view_offset_m / scene.camera_and_view_m.zw;
    let size = vec2<i32>(textureDimensions(terrain_depth));
    let texel = clamp(vec2<i32>(floor(view * vec2<f32>(size))), vec2<i32>(0), size - 1);
    let near_scale = scene.drone_camera.y;
    return near_scale / max(1.0 - textureLoad(terrain_depth, texel, 0), near_scale);
}
