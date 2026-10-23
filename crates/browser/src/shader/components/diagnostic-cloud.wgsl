struct DiagnosticCloudShadowSample {
    transmittance: vec2<f32>,
    receiver: CelestialReceiverLight,
};

fn diagnostic_cloud_shadow_sample(position_m: vec2<f32>) -> DiagnosticCloudShadowSample {
    let terrain = procedural_terrain_sample(position_m);
    let surface_height_m = max(terrain.elevation_m, tide_height_m());
    let receiver = celestial_receiver_light_at_height(surface_height_m);
    return DiagnosticCloudShadowSample(
        celestial_cloud_transmittance_at(receiver),
        receiver,
    );
}

fn diagnostic_lighting_cloud_transmittance(input: VertexOutput) -> vec4<f32> {
    let sample = diagnostic_cloud_shadow_sample(input.world_position_m);
    let shadow_amount = effective_cloud_shadow_amount(
        sample.transmittance.x,
        sample.transmittance.y,
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
        CLOUD_SHADOW_PLANE_HEIGHT_M,
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

// Direct-light transmittance under a light `light_height` above a raised
// receiver's horizon: a landform blocking the whole disc, then cloud of
// optical depth `cloud_depth` along the light.
fn diagnostic_shadow_darkness(light_height: f32, cloud_depth: f32) -> vec4<f32> {
    let ground_horizon_z = celestial_horizon_z_at_height(300.0);
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
        cloud_shadow_transmittance(exp(-cloud_depth), ground_light, ground_horizon_z, 1.0),
        0.0,
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
                    0.5,
                    receiver.sun_direction,
                    receiver.horizon_z,
                    1.0,
                ))
            + visibility.y * receiver.moon_amount
                * (1.0 - cloud_shadow_transmittance(
                    0.5,
                    receiver.moon_direction,
                    receiver.horizon_z,
                    moon_cloud_shadow_weight(
                        receiver,
                        calculate_atmospheric_ambient_light(RegionalCanopyConditions(0.0, 0.0), receiver),
                    ),
                )),
        0.0,
        1.0,
    );
    let actual = effective_cloud_shadow_amount(
        0.5,
        0.5,
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
    return vec4<f32>(ambient.clear_sky + ambient.canopy + ambient.canopy_scattered, 1.0);
}

fn diagnostic_receiver_ambient_share(surface_height_m: f32) -> vec4<f32> {
    let receiver = celestial_receiver_light_at_height(surface_height_m);
    return vec4<f32>(scalar_debug_color(
        calculate_atmospheric_ambient_light(
            RegionalCanopyConditions(0.0, 0.0),
            receiver,
        ).split.diffuse_shares.x,
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
    return vec4<f32>(ambient.clear_sky + ambient.canopy + ambient.canopy_scattered, 1.0);
}

// Shadow of the cumulus (0) or the deck (1) over this position under a light
// at the zenith, with the weather under them.
fn diagnostic_cloud_field_overhead_shadow(input: VertexOutput, output: u32) -> vec4<f32> {
    let position_m = input.world_position_m;
    let rays = cloud_shadow_rays_for_light(position_m, vec3<f32>(0.0, 0.0, 1.0), 1.0, 0.0);
    var transmittance = deck_transmittance(
        rays,
        deck_drivers(cloud_weather_with_land_at(rays.deck.position_m)),
        0.0,
    );
    if output == 0u {
        let field = direct_cloud_shadow_texel(position_m, rays);
        transmittance = 1.0 - disc_cover(field.x) * (1.0 - field.y);
    }
    return vec4<f32>(scalar_debug_color(1.0 - transmittance), 1.0);
}

// Cumulus transmittance at a cloud position along `light`, from the plane
// position whose cumulus ray meets it.
fn diagnostic_cumulus_along_light(cloud_position_m: vec2<f32>, light: vec3<f32>) -> f32 {
    var height_m = 2000.0;
    var plane_position_m = cloud_position_m;
    for (var step = 0; step < 4; step++) {
        plane_position_m = 2.0 * cloud_position_m - projected_cloud_position(
            cloud_position_m,
            CLOUD_SHADOW_PLANE_HEIGHT_M,
            height_m,
            light,
        );
        let rays = cloud_shadow_rays_for_light(
            plane_position_m,
            light,
            1.0,
            CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
        );
        let guess = cumulus_ray(
            plane_position_m,
            rays,
            cumulus_height_m(cloud_weather_with_land_at(rays.deck.position_m)),
        );
        height_m = cumulus_height_m(cloud_weather_with_land_at(guess.position_m));
    }
    let rays = cloud_shadow_rays_for_light(
        plane_position_m,
        light,
        1.0,
        CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
    );
    let field = direct_cloud_shadow_texel(plane_position_m, rays);
    return 1.0 - disc_cover(field.x) * (1.0 - field.y);
}

// Cumulus transmittance at this cloud position along two lights 30° high from
// opposite azimuths.
fn diagnostic_cumulus_along_two_lights(input: VertexOutput) -> vec4<f32> {
    let elevation = radians(30.0);
    var transmittance = vec2<f32>(1.0);
    for (var index = 0u; index < 2u; index++) {
        let azimuth = radians(20.0 + 180.0 * f32(index));
        transmittance[index] = diagnostic_cumulus_along_light(
            input.world_position_m,
            vec3<f32>(cos(elevation) * vec2<f32>(cos(azimuth), sin(azimuth)), sin(elevation)),
        );
    }
    return vec4<f32>(transmittance, 0.0, 1.0);
}
