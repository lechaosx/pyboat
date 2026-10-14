struct LowCloudSample {
    body_amount: f32,
    optical_depth: f32,
    gradient: vec2<f32>,
};

fn visible_low_cloud_position_m(position_m: vec2<f32>) -> vec2<f32> {
    let camera_delta_m = camera_relative_position_m(position_m);
    let projection_scale = 1.0 - clamp(tuning.cloud_rendering.w, 0.0, 2.0)
        * LOW_CLOUD_BASE_HEIGHT_M / LOW_CLOUD_PROJECTION_EYE_HEIGHT_M;
    return canonical_world_position(
        scene.camera_and_view_m.xy + projection_scale * camera_delta_m,
    );
}

fn low_cloud_latent_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    let body_size = clamp(tuning.cloud_wisps.z, 0.25, 4.0);
    return spatiotemporal_fractal_sample(
        vec2<u32>(0x6c8e9cf5u, 0x7a1d3b2fu),
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
        vec2<u32>(0x6c8e9cf5u, 0x7a1d3b2fu),
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
    let present = visible_low_cloud_coverage(weather) > 0.0 && tuning.cloud_wisps.w > 0.0;
    if !present && !EVALUATE_ALL_BRANCHES {
        return LowCloudSample(0.0, 0.0, vec2<f32>(0.0));
    }
    let latent = low_cloud_latent_sample(field_position_m);
    let shape = select(vec3<f32>(0.0), low_cloud_shape(latent.value, weather), present);
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
    let present = visible_low_cloud_coverage(weather) > 0.0 && tuning.cloud_wisps.w > 0.0;
    if !present && !EVALUATE_ALL_BRANCHES {
        return 0.0;
    }
    return select(
        0.0,
        low_cloud_shape(low_cloud_latent_value(field_position_m), weather).y,
        present,
    );
}

struct RegionalCanopyConditions {
    sky_fraction: f32,
    mean_optical_thickness: f32,
}

fn regional_canopy_conditions_from_drivers(
    drivers: RegionalCloudDrivers,
) -> RegionalCanopyConditions {
    if environment_feature_visibility().w <= 0.0 {
        return RegionalCanopyConditions(0.0, 0.0);
    }
    let condensation = drivers.condensation;
    let pressure_lift = drivers.pressure_lift;
    let instability = drivers.instability;
    let cloud_potential = 0.55 * condensation
        + 0.25 * pressure_lift
        + 0.20 * condensation * instability;
    let sky_fraction = smoothstep(0.30, 0.88, cloud_potential);
    let storm_depth = smoothstep(
        0.15,
        0.45,
        condensation * pressure_lift * instability,
    );
    return RegionalCanopyConditions(
        sky_fraction,
        0.18
            + 0.50 * condensation
            + 0.70 * pressure_lift
            + 0.45 * condensation * instability
            + 3.6 * storm_depth,
    );
}

fn regional_canopy_conditions(weather: CloudWeatherSample) -> RegionalCanopyConditions {
    return regional_canopy_conditions_from_drivers(weather.regional);
}

fn regional_canopy_conditions_at(position_m: vec2<f32>) -> RegionalCanopyConditions {
    return regional_canopy_conditions_from_drivers(regional_cloud_drivers_at(position_m));
}

fn procedural_upper_cloud_optical_depth(
    position_m: vec2<f32>,
    regional: RegionalCanopyConditions,
) -> f32 {
    let field_position_m = position_m - scene.view_cloud_motion_m.zw;
    let cover = regional.sky_fraction;
    if cover <= 0.0 && !EVALUATE_ALL_BRANCHES {
        return 0.0;
    }
    let latent = spatiotemporal_fractal_value(
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        field_position_m,
        4000.0,
        scene.cloud_time_coordinates.y,
        2u,
        0.35,
    );
    let threshold = 0.70 * (0.5 - cover) + 0.04;
    let amount = smoothstep(threshold - 0.08, threshold + 0.08, latent);
    return select(
        0.0,
        amount * mix(0.72, 1.0, 0.5 + 0.5 * latent) * regional.mean_optical_thickness,
        cover > 0.0,
    );
}
