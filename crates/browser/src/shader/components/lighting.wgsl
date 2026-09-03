fn celestial_direction(declination: f32, hour_angle: f32) -> vec3<f32> {
    let latitude = radians(surface_latitude_phase_degrees_at(surface_position_m));
    return vec3<f32>(
        -cos(declination) * sin(hour_angle),
        sin(declination) * cos(latitude)
            - cos(declination) * sin(latitude) * cos(hour_angle),
        sin(latitude) * sin(declination)
            + cos(latitude) * cos(declination) * cos(hour_angle),
    );
}

fn topocentric_moon_direction_at_height(height_m: f32) -> vec3<f32> {
    let world_radius_m = scene.world_size_m.x / TAU;
    let observer_radius_planet_radii = (
        world_radius_m + max(height_m, 0.0)
    ) / world_radius_m;
    return normalize(
        LUNAR_DISTANCE_PLANET_RADII * geocentric_moon_light_direction
            - vec3<f32>(0.0, 0.0, observer_radius_planet_radii),
    );
}

fn angular_separation(left: vec3<f32>, right: vec3<f32>) -> f32 {
    return atan2(length(cross(left, right)), dot(left, right));
}

fn circle_overlap_fraction(
    separation: f32,
    target_radius: f32,
    occluder_radius: f32,
) -> f32 {
    if separation >= target_radius + occluder_radius {
        return 0.0;
    }
    if separation <= abs(target_radius - occluder_radius) {
        if occluder_radius >= target_radius {
            return 1.0;
        }
        return occluder_radius * occluder_radius / (target_radius * target_radius);
    }
    let target_angle = acos(clamp(
        (separation * separation + target_radius * target_radius
            - occluder_radius * occluder_radius)
            / (2.0 * separation * target_radius),
        -1.0,
        1.0,
    ));
    let occluder_angle = acos(clamp(
        (separation * separation + occluder_radius * occluder_radius
            - target_radius * target_radius)
            / (2.0 * separation * occluder_radius),
        -1.0,
        1.0,
    ));
    let lens = 0.5 * sqrt(
        (-separation + target_radius + occluder_radius)
            * (separation + target_radius - occluder_radius)
            * (separation - target_radius + occluder_radius)
            * (separation + target_radius + occluder_radius),
    );
    return (
        target_radius * target_radius * target_angle
            + occluder_radius * occluder_radius * occluder_angle
            - lens
    ) / (PI * target_radius * target_radius);
}

fn initialize_celestial_lighting() {
    let hour_angle = TAU * (surface_local_solar_phase() - 0.5);
    sun_light_direction = celestial_direction(
        scene.celestial_orbit_angles.x,
        hour_angle,
    );
    geocentric_moon_light_direction = celestial_direction(
        scene.celestial_orbit_angles.z,
        hour_angle
            + scene.celestial_orbit_angles.y
            - scene.celestial_orbit_angles.w,
    );
    lunar_penumbra_fraction = scene.celestial_orbit_light.y;
    lunar_umbra_fraction = scene.celestial_orbit_light.z;
    let datum_light = celestial_receiver_light_at_height(0.0);
    moon_light_direction = datum_light.moon_direction;
    solar_eclipse_fraction = datum_light.solar_eclipse_fraction;
    sun_light_amount = datum_light.sun_amount;
    moon_light_amount = datum_light.moon_amount;
}

fn sun_direction() -> vec3<f32> {
    return sun_light_direction;
}

fn moon_direction() -> vec3<f32> {
    return moon_light_direction;
}

fn celestial_sun_amount() -> f32 {
    return sun_light_amount;
}

fn celestial_moon_amount() -> f32 {
    return moon_light_amount;
}

fn direct_sun_amount() -> f32 {
    let visibility = direct_light_visibility();
    return visibility.x * celestial_sun_amount();
}

fn direct_moon_amount() -> f32 {
    let visibility = direct_light_visibility();
    return visibility.y * celestial_moon_amount();
}

struct CelestialReceiverLight {
    height_m: f32,
    horizon_z: f32,
    sun_direction: vec3<f32>,
    moon_direction: vec3<f32>,
    solar_eclipse_fraction: f32,
    sun_amount: f32,
    moon_amount: f32,
};

fn celestial_horizon_z_at_height(height_m: f32) -> f32 {
    let world_radius_m = scene.world_size_m.x / TAU;
    let observer_radius_m = world_radius_m + max(height_m, 0.0);
    let radius_ratio = world_radius_m / observer_radius_m;
    return -sqrt(max(1.0 - radius_ratio * radius_ratio, 0.0));
}

fn celestial_disc_visibility_at_height(
    height_m: f32,
    light: vec3<f32>,
    angular_radius_rad: f32,
) -> f32 {
    let horizon_z = celestial_horizon_z_at_height(height_m);
    return celestial_disc_visibility_at_horizon(
        horizon_z,
        light,
        angular_radius_rad,
    );
}

fn celestial_disc_visibility_at_horizon(
    horizon_z: f32,
    light: vec3<f32>,
    angular_radius_rad: f32,
) -> f32 {
    let horizon_cosine = sqrt(max(1.0 - horizon_z * horizon_z, 0.0));
    let disc_sine = sin(angular_radius_rad);
    let disc_cosine = cos(angular_radius_rad);
    return smoothstep(
        horizon_z * disc_cosine - horizon_cosine * disc_sine,
        horizon_z * disc_cosine + horizon_cosine * disc_sine,
        light.z,
    );
}

fn celestial_visible_disc_sample_direction_at_horizon(
    horizon_z: f32,
    light: vec3<f32>,
    angular_radius_rad: f32,
) -> vec3<f32> {
    let light_horizontal = length(light.xy);
    if light_horizontal <= 0.001 {
        return light;
    }
    let disc_sine = sin(angular_radius_rad);
    let disc_cosine = cos(angular_radius_rad);
    let lower_limb_z = light.z * disc_cosine - light_horizontal * disc_sine;
    let upper_limb_z = light.z * disc_cosine + light_horizontal * disc_sine;
    if lower_limb_z >= horizon_z || upper_limb_z <= horizon_z {
        return light;
    }
    // The centre ray enters the planet during the lower half of partial visibility.
    // One visible-slice sample keeps the whole transition on the sunward path.
    let sampled_z = 0.5 * (horizon_z + upper_limb_z);
    let sampled_horizontal = sqrt(max(1.0 - sampled_z * sampled_z, 0.0));
    return vec3<f32>(sampled_horizontal * light.xy / light_horizontal, sampled_z);
}

fn solar_eclipse_fraction_for_moon(moon: vec3<f32>) -> f32 {
    let disc_sum = CELESTIAL_DISC_ANGULAR_RADIUS_RAD + MOON_ANGULAR_RADIUS_RAD;
    if dot(sun_direction(), moon) <= cos(disc_sum) {
        return 0.0;
    }
    return circle_overlap_fraction(
        angular_separation(sun_direction(), moon),
        CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
        MOON_ANGULAR_RADIUS_RAD,
    );
}

fn sun_amount_at_horizon_with_eclipse(
    horizon_z: f32,
    eclipse_fraction: f32,
) -> f32 {
    let elevation = clamp(
        (sun_direction().z - horizon_z) / (1.0 - horizon_z),
        0.0,
        1.0,
    );
    return (1.0 - eclipse_fraction)
        * celestial_disc_visibility_at_horizon(
            horizon_z,
            sun_direction(),
            CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
        )
        * (0.35 + 0.65 * sqrt(elevation));
}

fn celestial_receiver_light_at_height(height_m: f32) -> CelestialReceiverLight {
    let clamped_height_m = max(height_m, 0.0);
    let horizon_z = celestial_horizon_z_at_height(height_m);
    let moon = topocentric_moon_direction_at_height(clamped_height_m);
    let eclipse_fraction = solar_eclipse_fraction_for_moon(moon);
    let sun_amount = sun_amount_at_horizon_with_eclipse(
        horizon_z,
        eclipse_fraction,
    );
    let moon_elevation = clamp(
        (moon.z - horizon_z) / (1.0 - horizon_z),
        0.0,
        1.0,
    );
    let lunar_shadow_transmittance = max(
        1.0 - 0.18 * lunar_penumbra_fraction - 0.78 * lunar_umbra_fraction,
        0.04,
    );
    let moon_amount = lunar_shadow_transmittance
        * 0.12
        * scene.celestial_orbit_light.x
        * celestial_disc_visibility_at_horizon(
            horizon_z,
            moon,
            MOON_ANGULAR_RADIUS_RAD,
        )
        * (0.35 + 0.65 * sqrt(moon_elevation));
    return CelestialReceiverLight(
        clamped_height_m,
        horizon_z,
        sun_direction(),
        moon,
        eclipse_fraction,
        sun_amount,
        moon_amount,
    );
}

fn moon_canopy_scattered_light(
    optical_thickness: f32,
    underside_escape: f32,
    canopy_light: CelestialReceiverLight,
) -> vec3<f32> {
    let slant_cosine = max(
        (canopy_light.moon_direction.z - canopy_light.horizon_z)
            / (1.0 - canopy_light.horizon_z),
        0.04,
    );
    let intercepted_moon = 1.0 - exp(-optical_thickness / slant_cosine);
    return 0.38
        * canopy_light.moon_amount
        * intercepted_moon
        * underside_escape
        * calculate_moon_light_color_for_direction(
            regional_horizon_scattering,
            canopy_light.moon_direction,
        );
}

struct AtmosphericAmbientLight {
    clear_sky: vec3<f32>,
    canopy: vec3<f32>,
};

fn calculate_atmospheric_ambient_light(
    canopy: RegionalCanopyConditions,
    receiver: CelestialReceiverLight,
) -> AtmosphericAmbientLight {
    let sun = sun_direction();
    let receiver_sun_z = clamp(
        (sun.z - receiver.horizon_z) / (1.0 - receiver.horizon_z),
        0.0,
        1.0,
    );
    let sun_elevation = sqrt(receiver_sun_z);
    // These sine-of-elevation thresholds must stay synchronized with game::celestial_lighting.
    let daylight = smoothstep(-0.0348995, 0.1391731, sun.z);
    let lower_twilight = smoothstep(-0.1564345, 0.0174524, sun.z);
    let upper_afterglow = smoothstep(-0.309017, -0.1391731, sun.z)
        * (1.0 - smoothstep(-0.1391731, -0.0348995, sun.z));
    let solar_ambient = daylight * (0.45 + 0.515 * sun_elevation)
        + mix(0.24, 0.36, regional_horizon_scattering)
            * (1.0 - daylight)
            * lower_twilight
        + mix(0.04, 0.09, regional_horizon_scattering) * upper_afterglow;
    let clear_intensity = min(
        1.0,
        0.035
            + (1.0 - 0.85 * receiver.solar_eclipse_fraction)
                * solar_ambient
            + 0.85 * receiver.moon_amount,
    );
    let horizon_glow = max(lower_twilight, 0.35 * upper_afterglow)
        * (1.0 - smoothstep(0.18, 0.55, sun.z));
    let sunset_warmth = horizon_glow * regional_horizon_scattering;
    let sun_source = receiver.sun_amount * sun_diffuse_light_color();
    let moon_source = receiver.moon_amount
        * calculate_moon_light_color_for_direction(
            regional_horizon_scattering,
            receiver.moon_direction,
        );
    let source_amount = receiver.sun_amount + receiver.moon_amount;
    let source_color = select(
        vec3<f32>(0.72, 0.80, 0.88),
        (sun_source + moon_source) / max(source_amount, 0.0001),
        source_amount > 0.0,
    );
    let source_presence = smoothstep(0.0, 0.2, source_amount);
    let solar_sky_presence = clamp(
        daylight + 0.75 * lower_twilight + 0.35 * upper_afterglow,
        0.0,
        1.0,
    );
    var sky_color = mix(
        vec3<f32>(0.42, 0.52, 0.75),
        vec3<f32>(0.68, 0.80, 1.0),
        solar_sky_presence,
    );
    sky_color = mix(sky_color, vec3<f32>(0.72, 0.84, 1.0), daylight);
    sky_color = mix(
        sky_color,
        vec3<f32>(1.30, 0.52, 0.30),
        0.58 * sunset_warmth,
    );
    sky_color *= mix(
        vec3<f32>(1.0),
        source_color,
        0.35 * source_presence,
    );
    let clear_sky_light = clear_intensity * sky_color;

    let optical_thickness = max(canopy.mean_optical_thickness, 0.0);
    let clear_contribution = (1.0 - canopy.sky_fraction) * clear_sky_light;
    if canopy.sky_fraction <= 0.0 {
        return AtmosphericAmbientLight(clear_contribution, vec3<f32>(0.0));
    }
    if optical_thickness <= 0.0 {
        return AtmosphericAmbientLight(
            clear_contribution,
            canopy.sky_fraction * clear_sky_light,
        );
    }
    let diffuse_transmittance = exp(-0.72 * optical_thickness);
    let downward_albedo = mix(
        0.58,
        0.08,
        smoothstep(1.50, 4.80, optical_thickness),
    );
    let neutral_cloud_light = clear_intensity * vec3<f32>(0.78, 0.82, 0.86);
    let transformed_sky_light = diffuse_transmittance * clear_sky_light
        + downward_albedo
            * (1.0 - diffuse_transmittance)
            * neutral_cloud_light;

    let canopy_receiver = celestial_receiver_light_at_height(
        WEATHER_CANOPY_HEIGHT_M,
    );
    let slant_cosine = max(
        (sun.z - canopy_receiver.horizon_z) / (1.0 - canopy_receiver.horizon_z),
        0.04,
    );
    let intercepted_sun = 1.0 - exp(-optical_thickness / slant_cosine);
    let underside_escape = exp(-0.42 * optical_thickness);
    let scattered_sun_light = 0.38
        * canopy_receiver.sun_amount
        * intercepted_sun
        * underside_escape
        * sun_diffuse_light_color();
    let scattered_moon_light = moon_canopy_scattered_light(
        optical_thickness,
        underside_escape,
        canopy_receiver,
    );
    let canopy_light = transformed_sky_light
        + scattered_sun_light
        + scattered_moon_light;

    return AtmosphericAmbientLight(
        clear_contribution,
        canopy.sky_fraction * canopy_light,
    );
}

fn atmospheric_ambient_color_with_visibility(
    ambient: AtmosphericAmbientLight,
    visibility: vec2<f32>,
) -> vec3<f32> {
    return visibility.x * ambient.clear_sky + visibility.y * ambient.canopy;
}

fn regional_aerosol_optical_depth_at(regional: RegionalCloudDrivers) -> f32 {
    let settled_air = 1.0 - regional.pressure_lift;
    let dry_air = 1.0 - regional.condensation;
    let storm = regional.condensation
        * regional.pressure_lift
        * regional.instability;
    let unwashed = 1.0 - 0.72 * smoothstep(0.25, 0.70, storm);
    return mix(0.025, 0.32, clamp(0.58 * settled_air + 0.42 * dry_air, 0.0, 1.0))
        * unwashed;
}

fn local_aerosol_optical_depth_at(weather: CloudWeatherSample) -> f32 {
    let settled_air = 1.0 - weather.regional.pressure_lift;
    let dry_air = 1.0 - smoothstep(0.55, 0.95, weather.relative_humidity);
    let unwashed = 1.0 - 0.72 * weather.rain;
    return mix(0.025, 0.32, clamp(0.58 * settled_air + 0.42 * dry_air, 0.0, 1.0))
        * unwashed;
}

fn calculate_regional_horizon_scattering() -> f32 {
    let aerosol = smoothstep(
        0.04,
        0.30,
        regional_aerosol_optical_depth_at(surface_cloud_weather.regional),
    );
    let moist_haze = smoothstep(
        0.40,
        0.90,
        surface_cloud_weather.regional.condensation,
    );
    return clamp(0.82 * aerosol + 0.18 * moist_haze, 0.0, 1.0);
}

fn calculate_local_horizon_scattering() -> f32 {
    let aerosol = smoothstep(
        0.04,
        0.30,
        local_aerosol_optical_depth_at(surface_cloud_weather),
    );
    let moist_haze = smoothstep(0.65, 1.10, relative_humidity());
    return clamp(0.82 * aerosol + 0.18 * moist_haze, 0.0, 1.0);
}

fn calculate_moon_light_color_for_direction(
    scattering: f32,
    moon: vec3<f32>,
) -> vec3<f32> {
    let low_moon = 1.0 - smoothstep(0.08, 0.55, moon.z);
    let atmospheric_warmth = low_moon * mix(0.04, 0.82, scattering);
    let eclipse_warmth = smoothstep(0.05, 0.90, lunar_umbra_fraction);
    let warmth = 1.0
        - (1.0 - atmospheric_warmth) * (1.0 - eclipse_warmth);
    return mix(
        vec3<f32>(0.48, 0.62, 1.0),
        vec3<f32>(1.20, 0.28, 0.06),
        warmth,
    );
}

fn calculate_moon_light_color(scattering: f32) -> vec3<f32> {
    return calculate_moon_light_color_for_direction(scattering, moon_direction());
}

fn sun_light_color(scattering: f32) -> vec3<f32> {
    let low_sun = 1.0 - smoothstep(0.08, 0.55, sun_direction().z);
    let warmth = low_sun * mix(0.06, 1.0, scattering);
    return mix(vec3<f32>(1.0), vec3<f32>(1.55, 0.34, 0.06), warmth);
}

fn sun_direct_light_color() -> vec3<f32> {
    return sun_light_color(local_horizon_scattering);
}

fn sun_diffuse_light_color() -> vec3<f32> {
    return sun_light_color(regional_horizon_scattering);
}

fn sun_direct_contribution_color() -> vec3<f32> {
    return sun_direct_contribution_light_color;
}

fn moon_direct_contribution_color() -> vec3<f32> {
    return moon_direct_contribution_light_color;
}

fn sun_direct_contribution_color_for_amount(amount: f32) -> vec3<f32> {
    return 0.38 * sqrt(clamp(amount, 0.0, 1.0)) * sun_direct_light_color();
}

fn moon_direct_contribution_color_for_receiver(
    receiver: CelestialReceiverLight,
) -> vec3<f32> {
    return 0.16
        * sqrt(clamp(receiver.moon_amount, 0.0, 1.0))
        * calculate_moon_light_color_for_direction(
            local_horizon_scattering,
            receiver.moon_direction,
        );
}

fn initialize_light_contributions() {
    let sun_share = 0.38 * sqrt(clamp(direct_sun_amount(), 0.0, 1.0));
    let moon_share = 0.16 * sqrt(clamp(direct_moon_amount(), 0.0, 1.0));
    sun_direct_contribution_light_color = sun_share * sun_direct_light_color();
    moon_direct_contribution_light_color = moon_share * moon_direct_light_color;
}

fn ambient_contribution_share(receiver: CelestialReceiverLight) -> f32 {
    let visibility = direct_light_visibility();
    let sun_share = 0.38
        * sqrt(clamp(visibility.x * receiver.sun_amount, 0.0, 1.0));
    let moon_share = 0.16
        * sqrt(clamp(visibility.y * receiver.moon_amount, 0.0, 1.0));
    return max(1.0 - sun_share - moon_share, 0.0);
}

fn projected_cloud_position(
    position_m: vec2<f32>,
    source_height_m: f32,
    cloud_height_m: f32,
    light: vec3<f32>,
) -> vec2<f32> {
    let height_m = max(cloud_height_m - source_height_m, 0.0);
    let horizontal_light = length(light.xy);
    if height_m <= 0.0 || horizontal_light <= 0.001 {
        return position_m;
    }
    let world_radius_m = scene.world_size_m.x / TAU;
    let source_radius_m = world_radius_m + max(source_height_m, 0.0);
    let cloud_radius_m = world_radius_m + cloud_height_m;
    let origin_along_ray_m = source_radius_m * light.z;
    let ray_distance_m = -origin_along_ray_m + sqrt(
        origin_along_ray_m * origin_along_ray_m
            + cloud_radius_m * cloud_radius_m
            - source_radius_m * source_radius_m,
    );
    let angular_distance = atan2(
        ray_distance_m * horizontal_light,
        source_radius_m + ray_distance_m * light.z,
    );
    let tangent_direction = light.xy / horizontal_light;
    return position_m
        + world_radius_m * angular_distance * tangent_direction;
}

struct CloudShadowProjection {
    low_position_m: vec2<f32>,
    upper_position_m: vec2<f32>,
};

struct CloudShadowDepths {
    low: f32,
    upper: f32,
};

struct CelestialCloudShadowDepths {
    low: vec2<f32>,
    upper: vec2<f32>,
};

fn cloud_shadow_projection_at(
    position_m: vec2<f32>,
    surface_height_m: f32,
    light: vec3<f32>,
) -> CloudShadowProjection {
    let low_position_m = projected_cloud_position(
        position_m,
        surface_height_m,
        LOW_CLOUD_BASE_HEIGHT_M,
        light,
    );
    let upper_position_m = projected_cloud_position(
        position_m,
        surface_height_m,
        WEATHER_CANOPY_HEIGHT_M,
        light,
    );
    return CloudShadowProjection(
        low_position_m,
        upper_position_m,
    );
}

fn procedural_cloud_shadow_depths_at(
    projection: CloudShadowProjection,
) -> CloudShadowDepths {
    var depths = CloudShadowDepths(0.0, 0.0);
    if layer_visibility.y > 0.0 {
        let weather = cloud_weather_conditions_at(projection.low_position_m);
        depths.low = layer_visibility.y * tuning.cloud_shadows.x
            * procedural_low_cloud_optical_depth(projection.low_position_m, weather);
    }
    if layer_visibility.z > 0.0 {
        let canopy = regional_canopy_conditions_at(projection.upper_position_m);
        depths.upper = layer_visibility.z * tuning.cloud_shadows.y
            * procedural_upper_cloud_optical_depth(projection.upper_position_m, canopy);
    }
    return depths;
}

fn celestial_cloud_shadow_depths_at(
    position_m: vec2<f32>,
    surface_height_m: f32,
    direct_light: CelestialReceiverLight,
) -> CelestialCloudShadowDepths {
    var celestial_depths = CelestialCloudShadowDepths(vec2<f32>(0.0), vec2<f32>(0.0));
    if layer_visibility.y <= 0.0 && layer_visibility.z <= 0.0 {
        return celestial_depths;
    }
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let is_sun = source_index == 0u;
        let light = select(
            direct_light.moon_direction,
            direct_light.sun_direction,
            is_sun,
        );
        let light_amount = select(
            direct_light.moon_amount,
            direct_light.sun_amount,
            is_sun,
        );
        if light_amount > 0.0 && cloud_shadow_contrast(light) > 0.0 {
            let projection = cloud_shadow_projection_at(position_m, surface_height_m, light);
            let depths = sampled_cloud_shadow_depths_at(projection);
            celestial_depths.low[source_index] = depths.low;
            celestial_depths.upper[source_index] = depths.upper;
        }
    }
    return celestial_depths;
}

fn cloud_shadow_contrast(light: vec3<f32>) -> f32 {
    return smoothstep(0.04, 0.22, light.z);
}

fn cloud_shadow_transmittance(depth: f32, light: vec3<f32>) -> f32 {
    let shadow = exp(
        -tuning.cloud_shadows.z
            * tuning.cloud_rendering.x
            * cloud_shadow_contrast(light)
            * depth,
    );
    return mix(1.0, shadow, clamp(tuning.cloud_shadows.w, 0.0, 1.0));
}

fn cloud_layer_transmittance(
    depth: f32,
    light: vec3<f32>,
    receiver_horizon_z: f32,
) -> f32 {
    let slant_cosine = max(
        (light.z - receiver_horizon_z) / (1.0 - receiver_horizon_z),
        0.20,
    );
    let shadow = exp(
        -tuning.cloud_shadows.z
            * tuning.cloud_rendering.x
            * depth
            / slant_cosine,
    );
    return mix(1.0, shadow, clamp(tuning.cloud_shadows.w, 0.0, 1.0));
}

fn effective_cloud_shadow_amount(
    sun_depth: f32,
    moon_depth: f32,
    receiver: CelestialReceiverLight,
) -> f32 {
    let visibility = direct_light_visibility();
    return clamp(
        visibility.x * receiver.sun_amount
                * (1.0 - cloud_shadow_transmittance(sun_depth, receiver.sun_direction))
            + visibility.y * receiver.moon_amount
                * (1.0 - cloud_shadow_transmittance(moon_depth, receiver.moon_direction)),
        0.0,
        1.0,
    );
}
