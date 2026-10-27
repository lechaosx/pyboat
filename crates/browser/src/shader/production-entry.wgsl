@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.is_terrain <= 0.5 {
        return vec4<f32>(encode_display_color(tone_map_scene_color(
            linear_material_color(input.color), input.css_position_px,
        )), 1.0);
    }
    visual_pixel_footprint_m = scene.world_phases_and_pixel_footprint_m.w;
    tuning = production_tuning();
    landscape_visibility = vec4<f32>(1.0);
    var periodic_input = input;
    periodic_input.world_position_m = canonical_world_position(input.world_position_m);
    surface_position_m = periodic_input.world_position_m;
    let surface_circle_position = periodic_noise_circle_position_from_canonical(
        surface_position_m,
    );
    let fields = environment_field_sample(surface_position_m);
    let surface_weather = cloud_weather_field_from_environment_fields(
        surface_position_m,
        fields,
    );
    surface_cloud_weather = surface_weather.weather;
    initialize_surface_tide_from_sample(fields.tide);
    initialize_celestial_lighting();
    local_aerosol_optical_depth = local_aerosol_optical_depth_at(surface_cloud_weather);
    regional_aerosol_optical_depth = regional_aerosol_optical_depth_at(surface_cloud_weather.regional);
    let color = final_scene_color(
        periodic_input,
        surface_weather.atmosphere,
        surface_circle_position,
    );
    return vec4<f32>(encode_display_color(tone_map_scene_color(color.rgb, input.css_position_px)), color.a);
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
