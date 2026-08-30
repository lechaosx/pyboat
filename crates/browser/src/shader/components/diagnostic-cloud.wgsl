fn diagnostic_cloud_low_latent_texture(input: VertexOutput) -> vec4<f32> {
    let latent = low_cloud_latent_sample(
        input.world_position_m - scene.view_cloud_motion_m.xy,
    );
    return vec4<f32>(signed_debug_color(latent.value), 1.0);
}

fn diagnostic_cloud_low_optical_depth(input: VertexOutput) -> vec4<f32> {
    let low = procedural_low_cloud_sample(
        input.world_position_m,
        visual_cloud_weather(surface_cloud_weather),
    );
    return vec4<f32>(scalar_debug_color(low.optical_depth), 1.0);
}

fn diagnostic_cloud_low_normals(input: VertexOutput) -> vec4<f32> {
    let low = procedural_low_cloud_sample(
        input.world_position_m,
        visual_cloud_weather(surface_cloud_weather),
    );
    let normal = normalize(vec3<f32>(-low.gradient, 1.0));
    return vec4<f32>(0.5 + 0.5 * normal, 1.0);
}

fn diagnostic_cloud_weather_canopy_optical_depth(input: VertexOutput) -> vec4<f32> {
    let optical_depth = procedural_upper_cloud_optical_depth(
        input.world_position_m,
        visual_cloud_weather(surface_cloud_weather),
    );
    return vec4<f32>(scalar_debug_color(optical_depth), 1.0);
}

fn diagnostic_cloud_shadow_depths(position_m: vec2<f32>) -> CelestialCloudShadowDepths {
    let terrain = procedural_terrain_sample(position_m);
    let surface_height_m = max(terrain.elevation_m, tide_height_m());
    return celestial_cloud_shadow_depths_at(
        position_m,
        surface_height_m,
        visual_cloud_weather(surface_cloud_weather),
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
    let ambient_sky = ambient_sky_conditions_from_atmosphere(
        input.world_position_m,
        atmosphere,
    );
    return vec4<f32>(scalar_debug_color(ambient_sky.sky_fraction), 1.0);
}

fn diagnostic_lighting_solar_elevation(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(scalar_debug_color(0.5 + 0.5 * sun_direction().z), 1.0);
}

fn diagnostic_lighting_aerosol_optical_depth(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let aerosol = aerosol_optical_depth_at(position_m, cloud_weather_conditions_at(position_m));
    return vec4<f32>(scalar_debug_color(aerosol / 0.32), 1.0);
}

fn diagnostic_lighting_solar_eclipse_coverage(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(scalar_debug_color(solar_eclipse_fraction), 1.0);
}
