@group(1) @binding(0)
var environment_field_output: texture_storage_2d_array<rgba32float, write>;

@compute @workgroup_size(8, 8)
fn generate_environment_fields(
    @builtin(global_invocation_id) invocation_id: vec3<u32>,
) {
    let position_m = canonical_world_position(
        scene.camera_and_view_m.xy
            + scene.environment_field_grid_m.xy
            + vec2<f32>(invocation_id.xy) * scene.environment_field_grid_m.zw,
    );
    let layers = environment_field_layers(procedural_environment_field_sample(position_m));
    for (var layer = 0u; layer < ENVIRONMENT_FIELD_LAYERS; layer++) {
        textureStore(environment_field_output, invocation_id.xy, layer, layers[layer]);
    }
}
