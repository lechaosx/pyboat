@fragment
fn fragment_playground_scene(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene_from_environment_fields(input);
    initialize_shadow_cache_debug();
    if shadow_cache_debug_mode() == 2u {
        let color = shadow_cache_debug_tiles(initialized.input.world_position_m);
        return vec4<f32>(encode_display_color(color.rgb), color.a);
    }
    if shadow_cache_debug_mode() == 3u {
        let color = shadow_cache_debug_atlas(input.position.xy);
        return vec4<f32>(encode_display_color(color.rgb), color.a);
    }
    let color = final_scene_color(
        initialized.input,
        initialized.atmosphere,
        initialized.surface_circle_position,
    );
    let displayed = apply_shadow_cache_debug(vec4<f32>(tone_map_scene_color(color.rgb, input.position.xy), color.a));
    return vec4<f32>(encode_display_color(displayed.rgb), displayed.a);
}
