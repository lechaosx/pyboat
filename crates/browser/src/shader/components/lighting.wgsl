fn celestial_direction(declination: f32, hour_angle: f32) -> vec3<f32> {
    let latitude = surface_latitude_degrees() * PI / 180.0;
    return vec3<f32>(
        -cos(declination) * sin(hour_angle),
        sin(declination) * cos(latitude)
            - cos(declination) * sin(latitude) * cos(hour_angle),
        sin(latitude) * sin(declination)
            + cos(latitude) * cos(declination) * cos(hour_angle),
    );
}

fn topocentric_moon_direction(geocentric_direction: vec3<f32>) -> vec3<f32> {
    return normalize(
        LUNAR_DISTANCE_PLANET_RADII * geocentric_direction
            - vec3<f32>(0.0, 0.0, 1.0),
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
    let geocentric_moon_direction = celestial_direction(
        scene.celestial_orbit_angles.z,
        hour_angle
            + scene.celestial_orbit_angles.y
            - scene.celestial_orbit_angles.w,
    );
    moon_light_direction = topocentric_moon_direction(geocentric_moon_direction);
    solar_eclipse_fraction = circle_overlap_fraction(
        angular_separation(sun_light_direction, moon_light_direction),
        CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
        MOON_ANGULAR_RADIUS_RAD,
    );
    lunar_penumbra_fraction = scene.celestial_orbit_light.y;
    lunar_umbra_fraction = scene.celestial_orbit_light.z;
    let sun_horizon = smoothstep(0.0, 0.08, sun_light_direction.z);
    sun_light_amount = (1.0 - solar_eclipse_fraction)
        * sun_horizon
        * (0.35 + 0.65 * sqrt(max(sun_light_direction.z, 0.0)));
    let moon_elevation = sqrt(max(moon_light_direction.z, 0.0));
    let lunar_shadow_transmittance = max(
        1.0 - 0.18 * lunar_penumbra_fraction - 0.78 * lunar_umbra_fraction,
        0.04,
    );
    moon_light_amount = lunar_shadow_transmittance
        * 0.12
        * scene.celestial_orbit_light.x
        * smoothstep(0.0, 0.08, moon_light_direction.z)
        * (0.35 + 0.65 * moon_elevation);
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

fn calculate_atmospheric_ambient_color_with_visibility(
    canopy: RegionalCanopyConditions,
    visibility: vec2<f32>,
) -> vec3<f32> {
    if visibility.x <= 0.0 && visibility.y <= 0.0 {
        return vec3<f32>(0.0);
    }
    let sun = sun_direction();
    let sun_elevation = sqrt(max(sun.z, 0.0));
    let twilight = smoothstep(-0.18, 0.12, sun.z);
    let clear_intensity = min(
        1.0,
        0.035
            + (1.0 - 0.85 * solar_eclipse_fraction)
                * twilight
                * (0.45 + 0.515 * sun_elevation)
            + 0.85 * celestial_moon_amount(),
    );
    let daylight = smoothstep(-0.08, 0.35, sun.z);
    let horizon_glow = smoothstep(-0.12, 0.02, sun.z)
        * (1.0 - smoothstep(0.18, 0.55, sun.z));
    let sunset_warmth = horizon_glow * horizon_scattering;
    var clear_sky_light = vec3<f32>(0.0);
    if visibility.x > 0.0 {
        var sky_color = mix(
            vec3<f32>(0.42, 0.52, 0.75),
            vec3<f32>(0.84, 0.90, 1.0),
            twilight,
        );
        sky_color = mix(sky_color, vec3<f32>(1.0), daylight);
        sky_color = mix(
            sky_color,
            vec3<f32>(1.30, 0.52, 0.30),
            0.58 * sunset_warmth,
        );
        let moon_presence = (1.0 - daylight) * clamp(
            celestial_moon_amount() / 0.12,
            0.0,
            1.0,
        );
        sky_color = mix(sky_color, moon_light_color, 0.25 * moon_presence);
        clear_sky_light = clear_intensity * sky_color;
    }

    var canopy_light = vec3<f32>(0.0);
    if visibility.y > 0.0 {
        let sun_source = celestial_sun_amount() * sun_direct_light_color();
        let moon_source = celestial_moon_amount() * moon_light_color;
        let source_amount = celestial_sun_amount() + celestial_moon_amount();
        let source_color = select(
            vec3<f32>(0.72, 0.80, 0.88),
            (sun_source + moon_source) / max(source_amount, 0.0001),
            source_amount > 0.0,
        );
        let source_presence = smoothstep(0.0, 0.2, source_amount);
        let canopy_color = mix(
            vec3<f32>(0.72, 0.80, 0.88),
            source_color,
            0.45 * source_presence,
        );
        let canopy_attenuation = mix(
            0.82,
            0.34,
            smoothstep(0.35, 3.40, canopy.mean_optical_thickness),
        );
        canopy_light = clear_intensity * canopy_attenuation * canopy_color;
    }
    return visibility.x * (1.0 - canopy.sky_fraction) * clear_sky_light
        + visibility.y * canopy.sky_fraction * canopy_light;
}

fn calculate_atmospheric_ambient_color(
    canopy: RegionalCanopyConditions,
) -> vec3<f32> {
    return calculate_atmospheric_ambient_color_with_visibility(
        canopy,
        indirect_light_visibility(),
    );
}

fn atmospheric_ambient_color() -> vec3<f32> {
    return ambient_light_color;
}

fn aerosol_optical_depth_at(
    position_m: vec2<f32>,
    weather: CloudWeatherSample,
) -> f32 {
    let regional = 0.5 + 0.5 * spatiotemporal_fractal_value(
        vec2<u32>(0x1f83d9abu, 0xfb41bd6bu),
        position_m,
        max(8192.0, 0.125 * scene.world_size_m.x),
        0.15 * scene.pressure_time_coordinate.x,
        4u,
        0.5,
    );
    let dry_air = 1.0 - smoothstep(0.55, 0.95, weather.relative_humidity);
    let unwashed = 1.0 - 0.72 * weather.rain;
    return mix(0.025, 0.32, clamp(0.68 * regional + 0.32 * dry_air, 0.0, 1.0))
        * unwashed;
}

fn calculate_horizon_scattering() -> f32 {
    let aerosol = smoothstep(
        0.04,
        0.30,
        aerosol_optical_depth_at(surface_position_m, surface_cloud_weather),
    );
    let moist_haze = smoothstep(0.65, 1.10, relative_humidity());
    return clamp(0.82 * aerosol + 0.18 * moist_haze, 0.0, 1.0);
}

fn calculate_moon_light_color() -> vec3<f32> {
    let low_moon = 1.0 - smoothstep(0.08, 0.55, moon_direction().z);
    let atmospheric_warmth = low_moon * mix(0.04, 0.82, horizon_scattering);
    let eclipse_warmth = smoothstep(0.05, 0.90, lunar_umbra_fraction);
    let warmth = 1.0
        - (1.0 - atmospheric_warmth) * (1.0 - eclipse_warmth);
    return mix(
        vec3<f32>(0.48, 0.62, 1.0),
        vec3<f32>(1.20, 0.28, 0.06),
        warmth,
    );
}

fn sun_direct_light_color() -> vec3<f32> {
    let low_sun = 1.0 - smoothstep(0.08, 0.55, sun_direction().z);
    let warmth = low_sun * mix(0.06, 1.0, horizon_scattering);
    return mix(vec3<f32>(1.0), vec3<f32>(1.55, 0.34, 0.06), warmth);
}

fn sun_direct_contribution_color() -> vec3<f32> {
    return sun_direct_contribution_light_color;
}

fn moon_direct_contribution_color() -> vec3<f32> {
    return moon_direct_contribution_light_color;
}

fn ambient_contribution_color() -> vec3<f32> {
    return ambient_contribution_light_color;
}

fn initialize_light_contributions() {
    let sun_share = 0.38 * sqrt(clamp(direct_sun_amount(), 0.0, 1.0));
    let moon_share = 0.16 * sqrt(clamp(direct_moon_amount(), 0.0, 1.0));
    sun_direct_contribution_light_color = sun_share * sun_direct_light_color();
    moon_direct_contribution_light_color = moon_share * moon_light_color;
    ambient_contribution_light_color = atmospheric_ambient_color()
        * max(1.0 - sun_share - moon_share, 0.0);
}

fn ambient_contribution_share() -> f32 {
    let sun_share = 0.38 * sqrt(clamp(direct_sun_amount(), 0.0, 1.0));
    let moon_share = 0.16 * sqrt(clamp(direct_moon_amount(), 0.0, 1.0));
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
    if height_m <= 0.0 || light.z <= 0.0 || horizontal_light <= 0.001 {
        return position_m;
    }
    let longitude = TAU * position_m.x
        * scene.cloud_anchor_phase_and_inverse_world_size_m.z;
    let polar_y_m = clamp(
        position_m.y,
        -0.5 * scene.world_size_m.y,
        0.5 * scene.world_size_m.y,
    );
    let polar_excess_y_m = position_m.y - polar_y_m;
    let latitude = PI * polar_y_m
        * scene.cloud_anchor_phase_and_inverse_world_size_m.w;
    let longitude_sin = sin(longitude);
    let longitude_cos = cos(longitude);
    let latitude_sin = sin(latitude);
    let latitude_cos = cos(latitude);
    let up = vec3<f32>(
        latitude_cos * longitude_cos,
        latitude_cos * longitude_sin,
        latitude_sin,
    );
    let east = vec3<f32>(-longitude_sin, longitude_cos, 0.0);
    let north = vec3<f32>(
        -latitude_sin * longitude_cos,
        -latitude_sin * longitude_sin,
        latitude_cos,
    );
    let world_light = light.x * east + light.y * north + light.z * up;
    let world_radius_m = scene.world_size_m.x / TAU;
    let source_radius_m = world_radius_m + max(source_height_m, 0.0);
    let cloud_radius_m = world_radius_m + cloud_height_m;
    let ray_origin = source_radius_m * up;
    let origin_along_ray_m = dot(ray_origin, world_light);
    let ray_distance_m = -origin_along_ray_m + sqrt(
        origin_along_ray_m * origin_along_ray_m
            + cloud_radius_m * cloud_radius_m
            - source_radius_m * source_radius_m,
    );
    let cloud_position = ray_origin + ray_distance_m * world_light;
    let cloud_direction = normalize(cloud_position);
    return vec2<f32>(
        scene.world_size_m.x * atan2(cloud_direction.y, cloud_direction.x) / TAU,
        polar_excess_y_m
            + scene.world_size_m.y * asin(clamp(cloud_direction.z, -1.0, 1.0)) / PI,
    );
}

fn projected_cloud_footprint_m(position_m: vec2<f32>) -> f32 {
    var x_derivative = dpdx(position_m);
    var y_derivative = dpdy(position_m);
    x_derivative.x -= scene.world_size_m.x
        * round(
            x_derivative.x * scene.cloud_anchor_phase_and_inverse_world_size_m.z,
        );
    y_derivative.x -= scene.world_size_m.x
        * round(
            y_derivative.x * scene.cloud_anchor_phase_and_inverse_world_size_m.z,
        );
    return max(length(x_derivative), length(y_derivative));
}

struct CloudShadowProjection {
    low_position_m: vec2<f32>,
    upper_position_m: vec2<f32>,
    low_footprint_m: f32,
    upper_footprint_m: f32,
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
        max(
            projected_cloud_footprint_m(low_position_m),
            finite_source_shadow_footprint_m(LOW_CLOUD_BASE_HEIGHT_M - surface_height_m),
        ),
        max(
            projected_cloud_footprint_m(upper_position_m),
            finite_source_shadow_footprint_m(WEATHER_CANOPY_HEIGHT_M - surface_height_m),
        ),
    );
}

fn cloud_shadow_depths_at(projection: CloudShadowProjection) -> CloudShadowDepths {
    var depths = CloudShadowDepths(0.0, 0.0);
    if layer_visibility.y > 0.0 {
        depths.low = layer_visibility.y * procedural_low_cloud_optical_depth_with_weather(
            projection.low_position_m,
            low_cloud_weather_at(projection.low_position_m),
            projection.low_footprint_m,
        );
    }
    if layer_visibility.z > 0.0 {
        depths.upper = layer_visibility.z * procedural_upper_cloud_optical_depth_filtered(
            projection.upper_position_m,
            projection.upper_footprint_m,
        );
    }
    return depths;
}

fn celestial_cloud_shadow_depths_at(
    position_m: vec2<f32>,
    surface_height_m: f32,
) -> CelestialCloudShadowDepths {
    var celestial_depths = CelestialCloudShadowDepths(vec2<f32>(0.0), vec2<f32>(0.0));
    if layer_visibility.y <= 0.0 && layer_visibility.z <= 0.0 {
        return celestial_depths;
    }
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let is_sun = source_index == 0u;
        let light = select(moon_direction(), sun_direction(), is_sun);
        let light_amount = select(direct_moon_amount(), direct_sun_amount(), is_sun);
        if light_amount > 0.0 {
            let projection = cloud_shadow_projection_at(position_m, surface_height_m, light);
            let depths = cloud_shadow_depths_at(projection);
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
        -0.85
            * tuning.cloud_rendering.x
            * cloud_shadow_contrast(light)
            * depth,
    );
    return mix(1.0, shadow, clamp(tuning.cloud_rendering.y, 0.0, 1.0));
}

fn effective_cloud_shadow_amount(sun_depth: f32, moon_depth: f32) -> f32 {
    return clamp(
        direct_sun_amount()
                * (1.0 - cloud_shadow_transmittance(sun_depth, sun_direction()))
            + direct_moon_amount()
                * (1.0 - cloud_shadow_transmittance(moon_depth, moon_direction())),
        0.0,
        1.0,
    );
}

fn finite_source_shadow_footprint_m(height_m: f32) -> f32 {
    return 2.0 * max(height_m, 0.0) * CELESTIAL_DISC_ANGULAR_RADIUS_RAD;
}
