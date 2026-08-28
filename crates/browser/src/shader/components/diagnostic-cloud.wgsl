fn diagnostic_cloud_low_latent_texture(input: VertexOutput) -> vec4<f32> {
    let latent = visual_cloud_noise_sample(
        input.world_position_m,
        false,
        vec2<u32>(0x38d01377u, 0x452821e6u),
        320.0,
        12u,
        scene.low_cloud_epoch,
        scene.cloud_anchor_phase_and_padding.x,
        1800.0,
        visual_pixel_footprint_m,
    );
    return vec4<f32>(scalar_debug_color(0.5 + 0.5 * latent.value), 1.0);
}

fn diagnostic_cloud_low_coverage_threshold(input: VertexOutput) -> vec4<f32> {
    let low = procedural_low_cloud_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(low.detail_gate), 1.0);
}

fn diagnostic_cloud_low_amount(input: VertexOutput) -> vec4<f32> {
    let low = procedural_low_cloud_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(low.amount), 1.0);
}

fn diagnostic_cloud_low_optical_depth(input: VertexOutput) -> vec4<f32> {
    let low = procedural_low_cloud_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(low.optical_depth), 1.0);
}

fn diagnostic_cloud_low_normals(input: VertexOutput) -> vec4<f32> {
    let low = procedural_low_cloud_sample(input.world_position_m);
    let normal = normalize(vec3<f32>(-low.gradient, 1.0));
    return vec4<f32>(0.5 + 0.5 * normal, 1.0);
}

fn diagnostic_cloud_weather_canopy_optical_depth(input: VertexOutput) -> vec4<f32> {
    let upper = procedural_upper_cloud_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(upper.optical_depth), 1.0);
}

fn diagnostic_cloud_low_height(input: VertexOutput) -> vec4<f32> {
    let low = procedural_low_cloud_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(low.height_m / 320.0), 1.0);
}

fn diagnostic_cloud_low_formation(input: VertexOutput) -> vec4<f32> {
    let formation = visible_low_cloud_coverage(surface_cloud_weather);
    return vec4<f32>(vec3<f32>(formation), 1.0);
}

fn diagnostic_cloud_shadow_depths(position_m: vec2<f32>) -> CelestialCloudShadowDepths {
    let terrain = procedural_terrain_sample(position_m);
    let surface_height_m = max(terrain.elevation_m, tide_height_m());
    return celestial_cloud_shadow_depths_at(position_m, surface_height_m);
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

fn diagnostic_lighting_solar_elevation(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(scalar_debug_color(0.5 + 0.5 * sun_direction().z), 1.0);
}

fn diagnostic_lighting_aerosol_optical_depth(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let aerosol = aerosol_optical_depth_at(position_m, cloud_weather_at(position_m));
    return vec4<f32>(scalar_debug_color(aerosol / 0.32), 1.0);
}

fn diagnostic_lighting_solar_eclipse_coverage(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(scalar_debug_color(solar_eclipse_fraction), 1.0);
}
