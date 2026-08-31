@fragment
fn fragment_playground_scene(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene(input);
    initialize_shadow_cache_debug();
    if shadow_cache_debug_mode() == 2u {
        return shadow_cache_debug_tiles(initialized.input.world_position_m);
    }
    if shadow_cache_debug_mode() == 3u {
        return shadow_cache_debug_atlas(input.position.xy);
    }
    let color = final_scene_color(
        initialized.input,
        initialized.atmosphere,
        initialized.surface_circle_position,
    );
    return apply_shadow_cache_debug(color);
}
