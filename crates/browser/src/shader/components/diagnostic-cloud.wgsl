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

struct DiagnosticCloudShadowSample {
    depths: CelestialCloudShadowDepths,
    receiver: CelestialReceiverLight,
};

fn diagnostic_cloud_shadow_sample(position_m: vec2<f32>) -> DiagnosticCloudShadowSample {
    let terrain = procedural_terrain_sample(position_m);
    let surface_height_m = max(terrain.elevation_m, tide_height_m());
    let receiver = celestial_receiver_light_at_height(surface_height_m);
    return DiagnosticCloudShadowSample(
        celestial_cloud_shadow_depths_at(receiver),
        receiver,
    );
}

fn diagnostic_lighting_projected_low_shadow(input: VertexOutput) -> vec4<f32> {
    let sample = diagnostic_cloud_shadow_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(effective_cloud_shadow_amount(
        sample.depths.sun.low,
        sample.depths.moon.low,
        sample.receiver,
    )), 1.0);
}

fn diagnostic_lighting_projected_upper_shadow(input: VertexOutput) -> vec4<f32> {
    let sample = diagnostic_cloud_shadow_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(effective_cloud_shadow_amount(
        sample.depths.sun.upper,
        sample.depths.moon.upper,
        sample.receiver,
    )), 1.0);
}

fn diagnostic_lighting_combined_transmittance(input: VertexOutput) -> vec4<f32> {
    let sample = diagnostic_cloud_shadow_sample(input.world_position_m);
    let shadow_amount = effective_cloud_shadow_amount(
        sample.depths.sun.low + sample.depths.sun.upper,
        sample.depths.moon.low + sample.depths.moon.upper,
        sample.receiver,
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
    let receiver = celestial_receiver_light_at_height(0.0);
    return vec4<f32>(scalar_debug_color(receiver.solar_eclipse_fraction), 1.0);
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

fn diagnostic_lighting_receiver_moon_ambient(height_m: f32) -> vec4<f32> {
    regional_horizon_scattering = 0.5;
    let receiver = celestial_receiver_light_at_height(height_m);
    return vec4<f32>(scalar_debug_color(
        moon_atmospheric_amount(receiver),
    ), 1.0);
}

fn diagnostic_lighting_low_cloud_canopy_transmittance(
    input: VertexOutput,
) -> vec4<f32> {
    let low_cloud_light = celestial_receiver_light_at_height(
        LOW_CLOUD_BASE_HEIGHT_M,
    );
    let transmittance = upper_cloud_transmittance_at_low_cloud(low_cloud_light);
    return vec4<f32>(scalar_debug_color(min(
        transmittance.x,
        transmittance.y,
    )), 1.0);
}

fn diagnostic_low_cloud_sun_lighting() -> vec4<f32> {
    let lighting = low_cloud_sun_lighting(
        celestial_receiver_light_at_height(LOW_CLOUD_BASE_HEIGHT_M),
    );
    return vec4<f32>(
        lighting.surface_amount,
        lighting.total_amount,
        lighting.elevated_amount,
        1.0,
    );
}

fn diagnostic_projected_low_cloud_direct_color(input: VertexOutput) -> vec4<f32> {
    let position_m = visible_low_cloud_position_m(input.world_position_m);
    let weather = cloud_weather_at(position_m);
    let receiver = celestial_receiver_light_at_position_and_height(
        position_m,
        LOW_CLOUD_BASE_HEIGHT_M,
    );
    let color = cloud_color(
        vec3<f32>(0.0),
        LowCloudSample(1.0, 0.6, vec2<f32>(0.0)),
        vec2<f32>(1.0),
        vec4<f32>(1.0, 0.0, 0.0, 0.0),
        AtmosphericAmbientLight(vec3<f32>(0.0), vec3<f32>(0.0)),
        weather,
        receiver,
    );
    return vec4<f32>(color, 1.0);
}

fn diagnostic_projected_low_cloud_receiver(input: VertexOutput) -> vec4<f32> {
    let projected_position_m = visible_low_cloud_position_m(input.world_position_m);
    let receiver = celestial_receiver_light_at_position_and_height(
        projected_position_m,
        LOW_CLOUD_BASE_HEIGHT_M,
    );
    return vec4<f32>(
        receiver.sun_amount,
        receiver.moon_amount,
        select(0.0, 1.0, projected_position_m.x != input.world_position_m.x),
        1.0,
    );
}

fn diagnostic_below_datum_cloud_projection(input: VertexOutput) -> vec4<f32> {
    let light = normalize(vec3<f32>(0.95, 0.0, 0.2));
    let low_tide_position = projected_cloud_position(
        input.world_position_m,
        -1.0,
        WEATHER_CANOPY_HEIGHT_M,
        light,
    );
    let datum_position = projected_cloud_position(
        input.world_position_m,
        0.0,
        WEATHER_CANOPY_HEIGHT_M,
        light,
    );
    return vec4<f32>(scalar_debug_color(select(
        0.0,
        1.0,
        length(low_tide_position - datum_position) > 0.25,
    )), 1.0);
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

fn diagnostic_landform_shadow_disc_radii(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    var receiver = celestial_receiver_light_at_height(
        broad_landform_elevation_m(position_m),
    );
    let direction = normalize(vec3<f32>(0.95, 0.30, 0.08));
    receiver.sun_direction = direction;
    receiver.moon_direction = direction;
    receiver.sun_amount = 1.0;
    receiver.moon_amount = 1.0;
    let transmittance = landform_shadow_transmittance(receiver);
    return vec4<f32>(scalar_debug_color(clamp(
        500.0 * (transmittance.x - transmittance.y),
        0.0,
        1.0,
    )), 1.0);
}

// Direct-light transmittance under a light `light_height` above each
// receiver's horizon: a landform blocking the whole disc from a raised
// receiver, then cloud of `cloud_depth` over that receiver and over a
// lower-cloud receiver.
fn diagnostic_shadow_darkness(light_height: f32, cloud_depth: f32) -> vec4<f32> {
    let ground_horizon_z = celestial_horizon_z_at_height(300.0);
    let cloud_horizon_z = celestial_horizon_z_at_height(LOW_CLOUD_BASE_HEIGHT_M);
    let ground_light = diagnostic_light_above_horizon(light_height, ground_horizon_z);
    let buried_receiver_m = -2000.0;
    return vec4<f32>(
        landform_shadow_transmittance_for_light(
            vec2<f32>(0.0),
            buried_receiver_m,
            ground_horizon_z,
            ground_light,
            1.0,
            CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
            0u,
        ),
        cloud_shadow_transmittance(cloud_depth, ground_light, ground_horizon_z),
        cloud_layer_transmittance(
            cloud_depth,
            diagnostic_light_above_horizon(light_height, cloud_horizon_z),
            cloud_horizon_z,
        ),
        1.0,
    );
}

fn diagnostic_light_above_horizon(light_height: f32, horizon_z: f32) -> vec3<f32> {
    let z = horizon_z + light_height * (1.0 - horizon_z);
    return vec3<f32>(sqrt(1.0 - z * z), 0.0, z);
}

fn diagnostic_landform_shadow_tangent_continuity(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let elevation_m = broad_landform_elevation_m(position_m);
    let horizontal = normalize(vec2<f32>(0.95, 0.30));
    let horizon_z = celestial_horizon_z_at_height(elevation_m);
    let below = landform_shadow_transmittance_for_light(
        position_m,
        elevation_m,
        horizon_z,
        normalize(vec3<f32>(horizontal, -0.000001)),
        1.0,
        MOON_ANGULAR_RADIUS_RAD,
        1u,
    );
    let above = landform_shadow_transmittance_for_light(
        position_m,
        elevation_m,
        horizon_z,
        normalize(vec3<f32>(horizontal, 0.000001)),
        1.0,
        MOON_ANGULAR_RADIUS_RAD,
        1u,
    );
    return vec4<f32>(scalar_debug_color(clamp(
        8.0 * abs(above - below),
        0.0,
        1.0,
    )), 1.0);
}

fn diagnostic_landform_shadow_penumbra_center() -> vec4<f32> {
    return vec4<f32>(scalar_debug_color(landform_disc_occlusion(0.0, 1.0)), 1.0);
}

fn diagnostic_landform_shadow_partial_disc_ray() -> vec4<f32> {
    var receiver = celestial_receiver_light_at_height(0.0);
    receiver.horizon_z = 0.0;
    receiver.sun_direction = normalize(vec3<f32>(
        1.0,
        0.0,
        -0.5 * CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
    ));
    receiver.sun_amount = 0.5;
    let direction = landform_shadow_sample_direction(
        receiver,
        celestial_source_light(receiver, 0u),
    );
    return vec4<f32>(scalar_debug_color(select(
        0.0,
        1.0,
        direction.z > receiver.horizon_z,
    )), 1.0);
}

fn diagnostic_cloud_shadow_disc_radius(input: VertexOutput) -> vec4<f32> {
    let direction = normalize(vec3<f32>(0.95, 0.30, 0.20));
    let receiver = celestial_receiver_light_at_height(0.0);
    let point_depths = sampled_cloud_shadow_depths_at(
        cloud_shadow_projection_at(
            receiver.position_m,
            receiver.height_m,
            cloud_shadow_direction(receiver, CelestialSourceLight(direction, 1.0, 0.0)),
        ),
        0u,
    );
    let disc_depths = sampled_cloud_shadow_depths_at(
        cloud_shadow_projection_at(
            receiver.position_m,
            receiver.height_m,
            cloud_shadow_direction(receiver, CelestialSourceLight(direction, 1.0, 0.05)),
        ),
        0u,
    );
    let difference = abs(
        cloud_shadow_transmittance(point_depths.low, direction, receiver.horizon_z)
            - cloud_shadow_transmittance(disc_depths.low, direction, receiver.horizon_z),
    ) + abs(
        cloud_shadow_transmittance(point_depths.upper, direction, receiver.horizon_z)
            - cloud_shadow_transmittance(disc_depths.upper, direction, receiver.horizon_z),
    );
    return vec4<f32>(scalar_debug_color(clamp(20.0 * difference, 0.0, 1.0)), 1.0);
}

fn diagnostic_low_cloud_canopy_shadow_disc_radius(input: VertexOutput) -> vec4<f32> {
    let direction = normalize(vec3<f32>(0.95, 0.30, 0.20));
    let receiver = celestial_receiver_light_at_height(LOW_CLOUD_BASE_HEIGHT_M);
    let point_transmittance = upper_cloud_transmittance_at_low_cloud_for_source(
        receiver,
        CelestialSourceLight(direction, 1.0, 0.0),
    );
    let disc_transmittance = upper_cloud_transmittance_at_low_cloud_for_source(
        receiver,
        CelestialSourceLight(direction, 1.0, 0.05),
    );
    return vec4<f32>(scalar_debug_color(clamp(
        20.0 * abs(point_transmittance - disc_transmittance),
        0.0,
        1.0,
    )), 1.0);
}

fn diagnostic_cloud_projection(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let projected_position_m = projected_cloud_position(
        position_m,
        0.0,
        WEATHER_CANOPY_HEIGHT_M,
        normalize(vec3<f32>(0.20, 0.96, 0.20)),
    );
    let normalized_offset = (projected_position_m - position_m)
        * scene.inverse_world_size_m;
    return vec4<f32>(
        clamp(vec2<f32>(0.5) + normalized_offset, vec2<f32>(0.0), vec2<f32>(1.0)),
        0.0,
        1.0,
    );
}

fn diagnostic_cloud_projection_isotropy(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let east_position_m = projected_cloud_position(
        position_m,
        0.0,
        WEATHER_CANOPY_HEIGHT_M,
        normalize(vec3<f32>(1.0, 0.0, 0.2)),
    );
    let north_position_m = projected_cloud_position(
        position_m,
        0.0,
        WEATHER_CANOPY_HEIGHT_M,
        normalize(vec3<f32>(0.0, 1.0, 0.2)),
    );
    let east_distance_m = length(east_position_m - position_m);
    let north_distance_m = length(north_position_m - position_m);
    let relative_difference = abs(east_distance_m - north_distance_m)
        / max(east_distance_m, north_distance_m);
    return vec4<f32>(scalar_debug_color(relative_difference), 1.0);
}

fn diagnostic_cloud_shadow_receiver_light(surface_height_m: f32) -> vec4<f32> {
    let receiver = celestial_receiver_light_at_height(surface_height_m);
    let visibility = direct_light_visibility();
    let expected = clamp(
        visibility.x * receiver.sun_amount
                * (1.0 - cloud_shadow_transmittance(
                    1.0,
                    receiver.sun_direction,
                    receiver.horizon_z,
                ))
            + visibility.y * receiver.moon_amount
                * (1.0 - cloud_shadow_transmittance(
                    1.0,
                    receiver.moon_direction,
                    receiver.horizon_z,
                )),
        0.0,
        1.0,
    );
    let actual = effective_cloud_shadow_amount(
        1.0,
        1.0,
        receiver,
    );
    return vec4<f32>(scalar_debug_color(10.0 * abs(actual - expected)), 1.0);
}

fn diagnostic_world_light_direction() -> vec4<f32> {
    let direction = sun_direction();
    return vec4<f32>(0.5 + 0.5 * direction, 1.0);
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
