// The broad landform layer as two triangles per texel quad, one instance per
// row of quads. The draw's first vertex and instance pick the quads that
// `FieldGrids::landform_quads` gives.
@vertex
fn vertex_terrain_depth(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) row: u32,
) -> @builtin(position) vec4<f32> {
    // Corners (0, 0), (1, 0), (0, 1), then (0, 1), (1, 0), (1, 1).
    let corner = vertex_index % 6u;
    let texel = vec2<u32>(
        vertex_index / 6u + u32(corner == 1u || corner == 4u || corner == 5u),
        row + u32(corner == 2u || corner == 3u || corner == 5u),
    );
    let landform_m = landform_texel_m(texel);
    // The eye ray through a sea-level offset s passes elevation z at s scaled
    // by 1 − z / H, which clip w divides out; depth is 1 − n / w.
    let scale = 1.0 - max(landform_m.z, 0.0) / scene.drone_camera.x;
    return vec4<f32>(
        2.0 * landform_m.xy / scene.camera_and_view_m.zw,
        scale - scene.drone_camera.y,
        scale,
    );
}
