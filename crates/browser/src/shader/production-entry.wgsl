@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.is_terrain <= 0.5 {
        return vec4<f32>(input.color, 1.0);
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
    let surface_weather = low_cloud_weather_field_at(surface_position_m);
    surface_cloud_weather = surface_weather.weather;
    initialize_surface_tide_from_circle(surface_circle_position);
    initialize_celestial_lighting();
    horizon_scattering = calculate_horizon_scattering();
    moon_light_color = calculate_moon_light_color();
    ambient_light_color = calculate_atmospheric_ambient_color(
        regional_canopy_conditions(surface_cloud_weather),
    );
    initialize_light_contributions();
    return final_scene_color(
        periodic_input,
        surface_weather.atmosphere,
        surface_circle_position,
    );
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

fn indirect_light_visibility() -> vec2<f32> {
    return vec2<f32>(1.0);
}

fn fragment_layer_visibility() -> vec4<f32> {
    return vec4<f32>(1.0);
}

fn fragment_surface_light_visibility(_surface_kind: u32) -> vec4<f32> {
    return vec4<f32>(1.0);
}

fn fragment_low_cloud_light_visibility() -> vec4<f32> {
    return vec4<f32>(1.0);
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
