struct LowCloudSample {
    body_amount: f32,
    optical_depth: f32,
    gradient: vec2<f32>,
};

fn visual_cloud_weather(source: CloudWeatherSample) -> CloudWeatherSample {
    var weather = source;
    weather.convective = clamp(
        weather.instability * smoothstep(0.35, 0.85, weather.lift),
        0.0,
        1.0,
    );
    return weather;
}

fn low_cloud_latent_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    let body_size = clamp(tuning.cloud_wisps.z, 0.25, 4.0);
    return spatiotemporal_fractal_sample(
        vec2<u32>(0x38d01377u, 0x452821e6u),
        position_m,
        640.0 * body_size,
        scene.cloud_time_coordinates.x,
        5u,
        0.58,
    );
}

fn low_cloud_latent_value(position_m: vec2<f32>) -> f32 {
    let body_size = clamp(tuning.cloud_wisps.z, 0.25, 4.0);
    return spatiotemporal_fractal_value(
        vec2<u32>(0x38d01377u, 0x452821e6u),
        position_m,
        640.0 * body_size,
        scene.cloud_time_coordinates.x,
        5u,
        0.58,
    );
}

fn low_cloud_shape(raw_value: f32, weather: CloudWeatherSample) -> vec3<f32> {
    let cover = visible_low_cloud_coverage(weather);
    let occurrence = clamp(tuning.cloud_wisps.w, 0.0, 4.0);
    let convective = smoothstep(0.12, 0.68, weather.instability);
    let character = mix(
        clamp(tuning.cloud_detail.x, 0.0, 1.0),
        1.0,
        0.70 * convective,
    );
    let rounding = 0.05;
    let rounding_scale = sqrt(1.0 + rounding * rounding) - rounding;
    let magnitude = sqrt(raw_value * raw_value + rounding * rounding);
    let rounded_value = 2.0 * (magnitude - rounding) / rounding_scale - 1.0;
    let rounded_derivative = 2.0 * raw_value / (magnitude * rounding_scale);
    let detail_value = mix(raw_value, rounded_value + 0.10, character);
    let detail_value_derivative = mix(1.0, rounded_derivative, character);
    let contrast = max(tuning.cloud_detail.z, 0.01);
    let unclamped_detail = 0.5 + 0.5 * contrast * detail_value;
    let detail = clamp(unclamped_detail, 0.0, 1.0);
    let detail_derivative = select(
        0.0,
        0.5 * contrast * detail_value_derivative,
        unclamped_detail > 0.0 && unclamped_detail < 1.0,
    );
    let target_coverage = clamp(tuning.cloud_wisps.x * cover * occurrence, 0.0, 0.95);
    let threshold = mix(0.78, 0.32, sqrt(target_coverage));
    let edge_scale = clamp(max(tuning.cloud_detail.w, 0.005) / 0.06, 0.1, 2.0);
    let span = max(0.005, (1.0 - threshold) * edge_scale);
    let unclamped_amount = (detail - threshold) / span;
    let raw_body_amount = clamp(unclamped_amount, 0.0, 1.0);
    let raw_body_amount_derivative = select(
        0.0,
        detail_derivative / span,
        unclamped_amount > 0.0 && unclamped_amount < 1.0,
    );
    let rounded_body_amount = raw_body_amount * raw_body_amount
        * (3.0 - 2.0 * raw_body_amount);
    let rounded_body_amount_derivative = 6.0 * raw_body_amount
        * (1.0 - raw_body_amount) * raw_body_amount_derivative;
    let blobiness = mix(
        clamp(tuning.cloud_detail.y, 0.0, 1.0),
        1.0,
        0.72 * convective,
    );
    let presence = smoothstep(0.0, 0.08, cover * occurrence);
    let body_amount = presence * mix(
        raw_body_amount,
        rounded_body_amount,
        0.45 * blobiness,
    );
    let body_amount_derivative = presence * mix(
        raw_body_amount_derivative,
        rounded_body_amount_derivative,
        0.45 * blobiness,
    );
    let height_detail = 0.35 + 0.65 * detail;
    let optical_depth = body_amount * body_amount * (0.25 + detail);
    let height_derivative = 2.0 * body_amount * body_amount_derivative * height_detail
        + 0.65 * body_amount * body_amount * detail_derivative;
    return vec3<f32>(body_amount, optical_depth, height_derivative);
}

fn procedural_low_cloud_sample(
    position_m: vec2<f32>,
    weather: CloudWeatherSample,
) -> LowCloudSample {
    let field_position_m = position_m - scene.view_cloud_motion_m.xy;
    if visible_low_cloud_coverage(weather) <= 0.0 || tuning.cloud_wisps.w <= 0.0 {
        return LowCloudSample(0.0, 0.0, vec2<f32>(0.0));
    }
    let latent = low_cloud_latent_sample(field_position_m);
    let shape = low_cloud_shape(latent.value, weather);
    let normal_strength = mix(0.20, 1.0, clamp(tuning.cloud_wisps.y, 0.0, 1.0));
    return LowCloudSample(
        shape.x,
        shape.y,
        180.0 * normal_strength * shape.z * latent.gradient,
    );
}

fn procedural_low_cloud_optical_depth(
    position_m: vec2<f32>,
    weather: CloudWeatherSample,
) -> f32 {
    let field_position_m = position_m - scene.view_cloud_motion_m.xy;
    if visible_low_cloud_coverage(weather) <= 0.0 || tuning.cloud_wisps.w <= 0.0 {
        return 0.0;
    }
    return low_cloud_shape(low_cloud_latent_value(field_position_m), weather).y;
}

struct RegionalCanopyConditions {
    sky_fraction: f32,
    mean_optical_thickness: f32,
}

fn regional_canopy_conditions(weather: CloudWeatherSample) -> RegionalCanopyConditions {
    return RegionalCanopyConditions(
        clamp(weather.upper_coverage, 0.0, 1.0),
        0.35
            + 0.75 * weather.stratiform
            + 0.95 * weather.convective,
    );
}

fn ambient_sky_conditions_from_atmosphere(
    position_m: vec2<f32>,
    atmosphere: SurfaceAtmosphereSample,
) -> RegionalCanopyConditions {
    if environment_feature_visibility().w <= 0.0 {
        return RegionalCanopyConditions(0.0, 0.0);
    }
    let climate_air_temperature_c = atmosphere.climate.annual_mean_air_temperature_c
        + 0.5 * atmosphere.climate.seasonal_temperature_range_c
            * atmosphere.climate.seasonal_latitude_factor
            * scene.seasonal_phase_factors.x;
    let broad_temperature_scale_c = latitude_curve_value(
        TEMPERATURE_ANOMALY_SCALE_C,
        atmosphere.climate.latitude_degrees,
    );
    let broad_temperature_anomaly_c = 0.85 * broad_temperature_scale_c
        * spatiotemporal_noise_value(
        vec2<u32>(0x38d01377u, 0x452821e6u),
        position_m,
        65536.0,
        scene.scalar_field_time_coordinates.x,
    );
    let broad_air_temperature_c = climate_air_temperature_c
        + broad_temperature_anomaly_c;
    let climate_water_vapor_g_kg = select(
        0.0,
        0.72 * water_vapor_saturation_capacity_g_kg(climate_air_temperature_c),
        environment_feature_visibility().z > 0.0,
    );
    let broad_relative_humidity = clamp(
        climate_water_vapor_g_kg
            / water_vapor_saturation_capacity_g_kg(broad_air_temperature_c),
        0.0,
        1.5,
    );
    let broad_pressure_hpa = 1013.25
        + atmosphere.pressure.climate_potential_m2_s / 6000.0;
    let condensation = smoothstep(0.45, 0.95, broad_relative_humidity);
    let pressure_lift = 1.0 - smoothstep(990.0, 1014.0, broad_pressure_hpa);
    let cloud_signal = 0.72 * condensation + 0.28 * pressure_lift;
    let sky_fraction = clamp(cloud_signal, 0.0, 1.0);
    let storm_depth = smoothstep(0.55, 0.92, sky_fraction)
        * mix(0.35, 1.0, pressure_lift);
    return RegionalCanopyConditions(
        sky_fraction,
        0.25 + 0.80 * condensation + 1.10 * pressure_lift + 2.05 * storm_depth,
    );
}

fn procedural_upper_cloud_optical_depth(
    position_m: vec2<f32>,
    weather: CloudWeatherSample,
) -> f32 {
    let field_position_m = position_m - scene.view_cloud_motion_m.zw;
    let regional = regional_canopy_conditions(weather);
    let cover = clamp(
        max(regional.sky_fraction, 0.30 * weather.low_coverage),
        0.0,
        1.0,
    );
    if cover <= 0.0 {
        return 0.0;
    }
    let latent = spatiotemporal_fractal_value(
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        field_position_m,
        8000.0,
        scene.cloud_time_coordinates.y,
        4u,
        0.58,
    );
    let layered_amount = clamp(cover + 0.30 * latent, 0.0, 1.0);
    let cell = smoothstep(0.42, 0.72, 0.5 + 0.5 * latent);
    let cellular_amount = layered_amount * mix(0.20, 1.0, cell);
    let amount = smoothstep(0.0, 0.12, cover) * mix(
        layered_amount,
        max(0.20 * layered_amount, cellular_amount),
        weather.convective,
    );
    return amount * regional.mean_optical_thickness;
}
