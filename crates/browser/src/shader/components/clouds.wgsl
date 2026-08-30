struct LowCloudSample {
    amount: f32,
    optical_depth: f32,
    gradient: vec2<f32>,
};

fn view_center_cloud_weather() -> CloudWeatherSample {
    let relative_humidity = scene.view_cloud_primary.x;
    let instability = scene.view_cloud_primary.y;
    let lift = scene.view_cloud_primary.z;
    let rain = scene.view_cloud_secondary.y;
    let stratiform = clamp(
        smoothstep(0.78, 1.05, relative_humidity)
            * (1.0 - instability)
            * smoothstep(0.18, 0.75, lift),
        0.0,
        1.0,
    );
    let convective = clamp(
        instability * smoothstep(0.35, 0.85, lift) * mix(0.45, 1.0, rain),
        0.0,
        1.0,
    );
    return CloudWeatherSample(
        relative_humidity,
        instability,
        lift,
        scene.view_cloud_primary.w,
        scene.view_cloud_secondary.x,
        rain,
        stratiform,
        convective,
        scene.view_cloud_winds_m_s.xy,
        scene.view_cloud_winds_m_s.zw,
    );
}

fn low_cloud_latent_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    let body_size = clamp(tuning.cloud_wisps.z, 0.25, 4.0);
    return spatiotemporal_fractal_sample(
        vec2<u32>(0x38d01377u, 0x452821e6u),
        position_m - scene.view_cloud_motion_m.xy,
        640.0 * body_size,
        scene.view_cloud_time_coordinates.x,
        5u,
        0.58,
    );
}

fn low_cloud_latent_value(position_m: vec2<f32>) -> f32 {
    let body_size = clamp(tuning.cloud_wisps.z, 0.25, 4.0);
    return spatiotemporal_fractal_value(
        vec2<u32>(0x38d01377u, 0x452821e6u),
        position_m - scene.view_cloud_motion_m.xy,
        640.0 * body_size,
        scene.view_cloud_time_coordinates.x,
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
    let span = max(1.0 - threshold, max(tuning.cloud_detail.w, 0.005));
    let unclamped_amount = (detail - threshold) / span;
    let raw_amount = clamp(unclamped_amount, 0.0, 1.0);
    let raw_amount_derivative = select(
        0.0,
        detail_derivative / span,
        unclamped_amount > 0.0 && unclamped_amount < 1.0,
    );
    let rounded_amount = raw_amount * raw_amount * (3.0 - 2.0 * raw_amount);
    let rounded_amount_derivative = 6.0 * raw_amount * (1.0 - raw_amount)
        * raw_amount_derivative;
    let blobiness = mix(
        clamp(tuning.cloud_detail.y, 0.0, 1.0),
        1.0,
        0.72 * convective,
    );
    let presence = smoothstep(0.0, 0.08, cover * occurrence);
    let amount = presence * mix(raw_amount, rounded_amount, 0.45 * blobiness);
    let amount_derivative = presence * mix(
        raw_amount_derivative,
        rounded_amount_derivative,
        0.45 * blobiness,
    );
    let optical_depth = amount * (0.35 + 0.90 * detail);
    let optical_derivative = amount_derivative * (0.35 + 0.90 * detail)
        + 0.90 * amount * detail_derivative;
    return vec3<f32>(amount, optical_depth, optical_derivative);
}

fn procedural_low_cloud_sample(position_m: vec2<f32>) -> LowCloudSample {
    let weather = view_center_cloud_weather();
    if visible_low_cloud_coverage(weather) <= 0.0 || tuning.cloud_wisps.w <= 0.0 {
        return LowCloudSample(0.0, 0.0, vec2<f32>(0.0));
    }
    let latent = low_cloud_latent_sample(position_m);
    let shape = low_cloud_shape(latent.value, weather);
    let normal_strength = mix(0.20, 1.0, clamp(tuning.cloud_wisps.y, 0.0, 1.0));
    return LowCloudSample(
        shape.x,
        shape.y,
        150.0 * normal_strength * shape.z * latent.gradient,
    );
}

fn procedural_low_cloud_optical_depth(position_m: vec2<f32>) -> f32 {
    let weather = view_center_cloud_weather();
    if visible_low_cloud_coverage(weather) <= 0.0 || tuning.cloud_wisps.w <= 0.0 {
        return 0.0;
    }
    return low_cloud_shape(low_cloud_latent_value(position_m), weather).y;
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
            + 0.95 * weather.convective
            + 1.35 * weather.rain,
    );
}

fn ambient_sky_conditions_at(position_m: vec2<f32>) -> RegionalCanopyConditions {
    if environment_feature_visibility().w <= 0.0 {
        return RegionalCanopyConditions(0.0, 0.0);
    }
    let wavelength_m = max(
        128000.0,
        0.75 * max(scene.world_size_m.x, scene.world_size_m.y),
    );
    let cloud_signal = 0.5 + 0.5 * spatiotemporal_fractal_value(
        vec2<u32>(0xa54ff53au, 0x510e527fu),
        position_m,
        wavelength_m,
        0.08 * scene.pressure_time_coordinate.x,
        2u,
        0.50,
    );
    let sky_fraction = smoothstep(0.20, 0.80, cloud_signal);
    return RegionalCanopyConditions(
        sky_fraction,
        mix(0.25, 4.20, sky_fraction * sky_fraction),
    );
}

fn procedural_upper_cloud_optical_depth(position_m: vec2<f32>) -> f32 {
    let weather = view_center_cloud_weather();
    let regional = regional_canopy_conditions(weather);
    let cover = clamp(
        max(regional.sky_fraction, 0.30 * weather.low_coverage + 0.70 * weather.rain),
        0.0,
        1.0,
    );
    if cover <= 0.0 {
        return 0.0;
    }
    let latent = spatiotemporal_fractal_value(
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        position_m - scene.view_cloud_motion_m.zw,
        12000.0,
        scene.view_cloud_time_coordinates.y,
        4u,
        0.58,
    );
    let amount = smoothstep(0.0, 0.12, cover) * clamp(cover + 0.30 * latent, 0.0, 1.0);
    return amount * regional.mean_optical_thickness;
}
