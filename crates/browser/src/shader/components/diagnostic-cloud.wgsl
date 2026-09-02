fn diagnostic_cloud_low_latent_texture(input: VertexOutput) -> vec4<f32> {
    let latent = low_cloud_latent_sample(
        input.world_position_m - scene.view_cloud_motion_m.xy,
    );
    return vec4<f32>(signed_debug_color(latent.value), 1.0);
}

fn diagnostic_cloud_low_optical_depth(input: VertexOutput) -> vec4<f32> {
    let low = procedural_low_cloud_sample(
        input.world_position_m,
        surface_cloud_weather,
    );
    return vec4<f32>(scalar_debug_color(low.optical_depth), 1.0);
}

fn diagnostic_cloud_low_normals(input: VertexOutput) -> vec4<f32> {
    let low = procedural_low_cloud_sample(
        input.world_position_m,
        surface_cloud_weather,
    );
    let normal = normalize(vec3<f32>(-low.gradient, 1.0));
    return vec4<f32>(0.5 + 0.5 * normal, 1.0);
}

fn diagnostic_cloud_weather_canopy_optical_depth(input: VertexOutput) -> vec4<f32> {
    let atmosphere = surface_atmosphere_sample_at(input.world_position_m);
    let weather = cloud_weather_from_atmosphere(input.world_position_m, atmosphere);
    let optical_depth = procedural_upper_cloud_optical_depth(
        input.world_position_m,
        regional_canopy_conditions(weather),
    );
    return vec4<f32>(scalar_debug_color(optical_depth / 5.43), 1.0);
}

fn diagnostic_cloud_shadow_depths(position_m: vec2<f32>) -> CelestialCloudShadowDepths {
    let terrain = procedural_terrain_sample(position_m);
    let surface_height_m = max(terrain.elevation_m, tide_height_m());
    return celestial_cloud_shadow_depths_at(
        position_m,
        surface_height_m,
        celestial_receiver_light_at_height(surface_height_m),
    );
}

fn diagnostic_lighting_projected_low_shadow(input: VertexOutput) -> vec4<f32> {
    let depths = diagnostic_cloud_shadow_depths(input.world_position_m);
    return vec4<f32>(scalar_debug_color(effective_cloud_shadow_amount(
        depths.low.x,
        depths.low.y,
    )), 1.0);
}

fn diagnostic_lighting_projected_upper_shadow(input: VertexOutput) -> vec4<f32> {
    let depths = diagnostic_cloud_shadow_depths(input.world_position_m);
    return vec4<f32>(scalar_debug_color(effective_cloud_shadow_amount(
        depths.upper.x,
        depths.upper.y,
    )), 1.0);
}

fn diagnostic_lighting_combined_transmittance(input: VertexOutput) -> vec4<f32> {
    let depths = diagnostic_cloud_shadow_depths(input.world_position_m);
    let shadow_amount = effective_cloud_shadow_amount(
        depths.low.x + depths.upper.x,
        depths.low.y + depths.upper.y,
    );
    return vec4<f32>(scalar_debug_color(1.0 - shadow_amount), 1.0);
}

fn diagnostic_lighting_ambient_sky_occlusion(input: VertexOutput) -> vec4<f32> {
    let atmosphere = surface_atmosphere_sample_at(input.world_position_m);
    let weather = cloud_weather_from_atmosphere(input.world_position_m, atmosphere);
    let ambient_sky = regional_canopy_conditions(weather);
    return vec4<f32>(scalar_debug_color(ambient_sky.sky_fraction), 1.0);
}

fn diagnostic_lighting_solar_elevation(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(scalar_debug_color(0.5 + 0.5 * sun_direction().z), 1.0);
}

fn diagnostic_lighting_low_cloud_sun_visibility(input: VertexOutput) -> vec4<f32> {
    let visibility = celestial_disc_visibility_at_height(
        LOW_CLOUD_BASE_HEIGHT_M,
        sun_direction(),
        CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
    );
    return vec4<f32>(scalar_debug_color(visibility), 1.0);
}

fn diagnostic_lighting_canopy_sun_visibility(input: VertexOutput) -> vec4<f32> {
    let visibility = celestial_disc_visibility_at_height(
        WEATHER_CANOPY_HEIGHT_M,
        sun_direction(),
        CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
    );
    return vec4<f32>(scalar_debug_color(visibility), 1.0);
}

fn diagnostic_lighting_canopy_moon_scattering(input: VertexOutput) -> vec4<f32> {
    let canopy = regional_canopy_conditions(surface_cloud_weather);
    let underside_escape = exp(-0.42 * canopy.mean_optical_thickness);
    let scattering = moon_canopy_scattered_light(
        canopy.mean_optical_thickness,
        underside_escape,
        celestial_receiver_light_at_height(WEATHER_CANOPY_HEIGHT_M),
    );
    let luminance = canopy.sky_fraction
        * dot(scattering, vec3<f32>(0.2126, 0.7152, 0.0722));
    return vec4<f32>(scalar_debug_color(luminance / 0.05), 1.0);
}

fn diagnostic_lighting_aerosol_optical_depth(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let atmosphere = surface_atmosphere_sample_at(position_m);
    let weather = cloud_weather_from_atmosphere(position_m, atmosphere);
    let aerosol = regional_aerosol_optical_depth_at(weather.regional);
    return vec4<f32>(scalar_debug_color(aerosol / 0.32), 1.0);
}

fn diagnostic_lighting_solar_eclipse_coverage(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(scalar_debug_color(solar_eclipse_fraction), 1.0);
}

fn diagnostic_lighting_surface_sun_amount(input: VertexOutput) -> vec4<f32> {
    let terrain = procedural_terrain_sample(input.world_position_m);
    let surface_height_m = max(terrain.elevation_m, tide_height_m());
    let direct_light = celestial_receiver_light_at_height(surface_height_m);
    return vec4<f32>(scalar_debug_color(direct_light.sun_amount), 1.0);
}

fn diagnostic_lighting_receiver_moon(height_m: f32) -> vec4<f32> {
    let direct_light = celestial_receiver_light_at_height(height_m);
    return vec4<f32>(
        0.5 + 0.5 * direct_light.moon_direction,
        direct_light.solar_eclipse_fraction,
    );
}

fn diagnostic_lighting_low_cloud_canopy_transmittance(
    input: VertexOutput,
) -> vec4<f32> {
    let low_cloud_light = celestial_receiver_light_at_height(
        LOW_CLOUD_BASE_HEIGHT_M,
    );
    let transmittance = upper_cloud_transmittance_at_low_cloud(
        input.world_position_m,
        WEATHER_CANOPY_HEIGHT_M,
        low_cloud_light,
    );
    return vec4<f32>(scalar_debug_color(min(
        transmittance.x,
        transmittance.y,
    )), 1.0);
}

fn diagnostic_low_cloud_sun_lighting(surface_height_m: f32) -> vec4<f32> {
    let lighting = low_cloud_sun_lighting(
        celestial_receiver_light_at_height(surface_height_m),
        celestial_receiver_light_at_height(LOW_CLOUD_BASE_HEIGHT_M),
    );
    return vec4<f32>(
        lighting.surface_amount,
        lighting.total_amount,
        lighting.elevated_amount,
        1.0,
    );
}

fn diagnostic_landform_sun_grade_moon_coupling() -> vec4<f32> {
    var receiver = celestial_receiver_light_at_height(0.0);
    receiver.sun_direction = vec3<f32>(0.7035, 0.1005, 0.7035);
    receiver.moon_direction = vec3<f32>(-0.5025, 0.5025, 0.7035);
    receiver.sun_amount = 1.0;
    receiver.moon_amount = 1.0;
    var sun_only_receiver = receiver;
    sun_only_receiver.moon_amount = 0.0;
    var moon_only_receiver = receiver;
    moon_only_receiver.sun_amount = 0.0;
    let grade = landform_light_grade(vec2<f32>(0.28, -0.21), 0.45, receiver);
    let sun_only_grade = landform_light_grade(
        vec2<f32>(0.28, -0.21),
        0.45,
        sun_only_receiver,
    );
    let moon_only_grade = landform_light_grade(
        vec2<f32>(0.28, -0.21),
        0.45,
        moon_only_receiver,
    );
    return vec4<f32>(scalar_debug_color(abs(
        grade.y - sun_only_grade.y,
    ) + abs(
        grade.z - moon_only_grade.z,
    )), 1.0);
}

fn diagnostic_receiver_ambient(surface_height_m: f32) -> vec4<f32> {
    let receiver = celestial_receiver_light_at_height(surface_height_m);
    let ambient = calculate_atmospheric_ambient_light(
        RegionalCanopyConditions(0.0, 0.0),
        receiver,
    );
    return vec4<f32>(ambient.clear_sky + ambient.canopy, 1.0);
}

fn diagnostic_receiver_ambient_share(surface_height_m: f32) -> vec4<f32> {
    let receiver = celestial_receiver_light_at_height(surface_height_m);
    return vec4<f32>(scalar_debug_color(
        ambient_contribution_share(receiver),
    ), 1.0);
}

fn diagnostic_ambient_with_surface_horizon(surface_height_m: f32) -> vec4<f32> {
    var receiver = celestial_receiver_light_at_height(0.0);
    receiver.height_m = surface_height_m;
    receiver.horizon_z = celestial_horizon_z_at_height(surface_height_m);
    let ambient = calculate_atmospheric_ambient_light(
        RegionalCanopyConditions(0.0, 0.0),
        receiver,
    );
    return vec4<f32>(ambient.clear_sky + ambient.canopy, 1.0);
}
