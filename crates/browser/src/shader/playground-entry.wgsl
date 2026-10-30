@fragment
fn fragment_playground_scene(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene_from_environment_fields(input);
    if occluder_field_inspection_mode() == 1u {
        return vec4<f32>(encode_display_color(occluder_field_inspection_grids(input.position.xy)), 1.0);
    }
    let color = final_scene_color(
        initialized.input,
        initialized.atmosphere,
        initialized.surface_circle_position,
    );
    let adaptation = field_adaptation(input.css_position_px);
    var displayed = tone_map_scene_color(color.rgb, adaptation, input.css_position_px);
    if occluder_field_inspection_mode() == 2u && occluder_field_lookups_outside {
        displayed = vec3<f32>(1.0, 0.0, 1.0);
    }
    return vec4<f32>(encode_display_color(displayed), color.a);
}

// The composition with every surface a matte 50% grey under the light the
// eye adapts to there, shown through the eye.
@fragment
fn fragment_playground_neutral_scene(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene_from_environment_fields(input);
    let light = scene_illumination(
        initialized.input.world_position_m,
        initialized.atmosphere,
        initialized.surface_circle_position,
    );
    let adaptation = field_adaptation(input.css_position_px);
    return vec4<f32>(encode_display_color(tone_map_scene_color(0.5 * light, adaptation, input.css_position_px)), 1.0);
}

@fragment
fn fragment_playground_illumination(input: VertexOutput) -> @location(0) vec4<f32> {
    let initialized = initialize_playground_scene_from_environment_fields(input);
    return adapting_light(scene_illumination(
        initialized.input.world_position_m,
        initialized.atmosphere,
        initialized.surface_circle_position,
    ));
}
