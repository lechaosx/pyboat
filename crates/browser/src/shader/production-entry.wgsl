@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.is_terrain <= 0.5 {
        return vec4<f32>(encode_display_color(tone_map_scene_color(
            linear_material_color(input.color), input.position.xy,
        )), 1.0);
    }
    visual_pixel_footprint_m = scene.world_phases_and_pixel_footprint_m.w;
    tuning = production_tuning();
    layer_visibility = vec4<f32>(1.0);
    landscape_visibility = vec4<f32>(1.0);
    atmosphere_model = vec4<f32>(1.0);
    climate_visibility = vec4<f32>(1.0);
    var periodic_input = input;
    periodic_input.world_position_m = canonical_world_position(input.world_position_m);
    surface_position_m = periodic_input.world_position_m;
    let surface_circle_position = periodic_noise_circle_position_from_canonical(
        surface_position_m,
    );
    let surface_weather = cloud_weather_field_at(surface_position_m);
    surface_cloud_weather = surface_weather.weather;
    initialize_surface_tide_from_circle(surface_circle_position);
    initialize_celestial_lighting();
    local_horizon_scattering = calculate_local_horizon_scattering();
    regional_horizon_scattering = calculate_regional_horizon_scattering();
    let color = final_scene_color(
        periodic_input,
        surface_weather.atmosphere,
        surface_circle_position,
    );
    return vec4<f32>(encode_display_color(tone_map_scene_color(color.rgb, input.position.xy)), color.a);
}

fn environment_feature_visibility() -> vec4<f32> {
    return vec4<f32>(1.0);
}

fn material_feature_visibility() -> vec4<f32> {
    return vec4<f32>(1.0);
}

fn effect_feature_visibility() -> vec4<f32> {
    return vec4<f32>(1.0);
}

fn lighting_feature_visibility() -> vec4<f32> {
    return vec4<f32>(1.0);
}

fn direct_light_visibility() -> vec2<f32> {
    return vec2<f32>(1.0);
}

fn fragment_layer_visibility() -> vec4<f32> {
    return vec4<f32>(1.0);
}

fn fragment_light_visibility() -> vec4<f32> {
    return tuning.light_balance;
}

fn land_material_color_visibility() -> vec4<f32> {
    return vec4<f32>(1.0);
}

fn solid_material_color_visibility() -> vec4<f32> {
    return vec4<f32>(1.0);
}

fn water_material_color_visibility() -> vec2<f32> {
    return vec2<f32>(1.0);
}

fn low_cloud_material_color_visibility() -> f32 {
    return 1.0;
}
