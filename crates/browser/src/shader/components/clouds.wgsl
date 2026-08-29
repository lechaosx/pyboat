struct CloudTracePositions {
    current: vec2<f32>,
    next: vec2<f32>,
};

fn trace_cloud_positions_m(
    position_m: vec2<f32>,
    current_duration_seconds: f32,
    next_duration_seconds: f32,
    wind_m_s: vec2<f32>,
) -> CloudTracePositions {
    return CloudTracePositions(
        position_m + current_duration_seconds * wind_m_s,
        position_m + next_duration_seconds * wind_m_s,
    );
}

fn visual_cloud_noise_sample(
    position_m: vec2<f32>,
    wind_m_s: vec2<f32>,
    field_salt: vec2<u32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    anchor_epoch: vec2<u32>,
    anchor_phase: f32,
    anchor_weights: vec2<f32>,
    refresh_period_seconds: f32,
    pixel_footprint_m: f32,
) -> TerrainNoiseSample {
    let phase = clamp(anchor_phase, 0.0, 1.0);
    let traced = trace_cloud_positions_m(
        position_m,
        -phase * refresh_period_seconds,
        (1.0 - phase) * refresh_period_seconds,
        wind_m_s,
    );
    return visual_transported_noise_sample(
        traced.current,
        traced.next,
        field_salt,
        initial_wavelength_m,
        octave_count,
        anchor_epoch,
        anchor_weights,
        pixel_footprint_m,
    );
}

fn visual_cloud_noise_value(
    position_m: vec2<f32>,
    wind_m_s: vec2<f32>,
    field_salt: vec2<u32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    anchor_epoch: vec2<u32>,
    anchor_phase: f32,
    anchor_weights: vec2<f32>,
    refresh_period_seconds: f32,
    pixel_footprint_m: f32,
) -> f32 {
    let phase = clamp(anchor_phase, 0.0, 1.0);
    let traced = trace_cloud_positions_m(
        position_m,
        -phase * refresh_period_seconds,
        (1.0 - phase) * refresh_period_seconds,
        wind_m_s,
    );
    return visual_transported_noise_value(
        traced.current,
        traced.next,
        field_salt,
        initial_wavelength_m,
        octave_count,
        anchor_epoch,
        anchor_weights,
        pixel_footprint_m,
    );
}

struct LowCloudSample {
    amount: f32,
    optical_depth: f32,
    height_m: f32,
    darkness: f32,
    relief: f32,
    gradient: vec2<f32>,
    detail_gate: f32,
};

struct LowCloudShape {
    detail_character: f32,
    unclamped_detail_amount: f32,
    detail_amount: f32,
    body_threshold: f32,
    amount_span: f32,
    raw_amount: f32,
    cloud_amount: f32,
    detail_blobiness: f32,
    weather_presence: f32,
};

fn low_cloud_shape(
    weather: CloudWeatherSample,
    cover: f32,
    raw_detail_value: f32,
) -> LowCloudShape {
    let convective = smoothstep(0.12, 0.68, weather.instability);
    let detail_character = mix(
        clamp(tuning.cloud_detail.x, 0.0, 1.0),
        1.0,
        0.70 * convective,
    );
    let billow_rounding = 0.05;
    let rounded_magnitude = (
        sqrt(raw_detail_value * raw_detail_value + billow_rounding * billow_rounding)
            - billow_rounding
    ) / (sqrt(1.0 + billow_rounding * billow_rounding) - billow_rounding);
    let rounded_detail_value = 2.0 * rounded_magnitude - 1.0;
    let detail_value = mix(
        raw_detail_value,
        rounded_detail_value + 0.10,
        detail_character,
    );
    let detail_contrast = max(tuning.cloud_detail.z, 0.01);
    let unclamped_detail_amount = 0.5 + 0.5 * detail_contrast * detail_value;
    let detail_amount = clamp(unclamped_detail_amount, 0.0, 1.0);
    let target_coverage = clamp(tuning.cloud_wisps.x * cover, 0.0, 0.95);
    let body_threshold = mix(0.78, 0.32, sqrt(target_coverage));
    let edge_softness = max(tuning.cloud_detail.w, 0.005);
    let detail_blobiness = mix(
        clamp(tuning.cloud_detail.y, 0.0, 1.0),
        1.0,
        0.72 * convective,
    );
    let weather_presence = smoothstep(0.0, 0.08, cover);
    let amount_span = max(1.0 - body_threshold, edge_softness);
    let raw_amount = clamp(
        (detail_amount - body_threshold) / amount_span,
        0.0,
        1.0,
    );
    let rounded_amount = raw_amount * raw_amount * (3.0 - 2.0 * raw_amount);
    let cloud_amount = weather_presence * mix(
        raw_amount,
        rounded_amount,
        0.45 * detail_blobiness,
    );
    return LowCloudShape(
        detail_character,
        unclamped_detail_amount,
        detail_amount,
        body_threshold,
        amount_span,
        raw_amount,
        cloud_amount,
        detail_blobiness,
        weather_presence,
    );
}

fn procedural_low_cloud_sample_with_weather(
    position_m: vec2<f32>,
    weather: CloudWeatherSample,
    pixel_footprint_m: f32,
) -> LowCloudSample {
    let cover = visible_low_cloud_coverage(weather);
    if cover <= 0.0 {
        var empty: LowCloudSample;
        empty.amount = 0.0;
        empty.optical_depth = 0.0;
        empty.height_m = 0.0;
        empty.darkness = 0.0;
        empty.relief = 0.0;
        empty.gradient = vec2<f32>(0.0);
        empty.detail_gate = 0.0;
        return empty;
    }
    let raw_detail = visual_cloud_noise_sample(
        position_m,
        weather.low_advection_wind_m_s,
        vec2<u32>(0x38d01377u, 0x452821e6u),
        320.0,
        12u,
        scene.low_cloud_epoch,
        scene.cloud_anchor_phase_and_inverse_world_size_m.x,
        scene.cloud_anchor_weights.xy,
        1800.0,
        pixel_footprint_m,
    );
    let shape = low_cloud_shape(weather, cover, raw_detail.value);
    let billow_rounding = 0.05;
    let rounded_detail_gradient = 2.0 * raw_detail.value * raw_detail.gradient
        / (
            sqrt(raw_detail.value * raw_detail.value + billow_rounding * billow_rounding)
                * (sqrt(1.0 + billow_rounding * billow_rounding) - billow_rounding)
        );
    let shaped_detail_gradient = mix(
        raw_detail.gradient,
        rounded_detail_gradient,
        shape.detail_character,
    );
    let detail_contrast = max(tuning.cloud_detail.z, 0.01);
    let detail_gradient = select(
        vec2<f32>(0.0),
        0.5 * detail_contrast * shaped_detail_gradient,
        shape.unclamped_detail_amount > 0.0 && shape.unclamped_detail_amount < 1.0,
    );
    let amount_inside = select(
        0.0,
        1.0,
        shape.detail_amount > shape.body_threshold && shape.detail_amount < 1.0,
    );
    let raw_amount_gradient = amount_inside * detail_gradient / shape.amount_span;
    let rounded_amount_gradient = 6.0
        * shape.raw_amount * (1.0 - shape.raw_amount) * raw_amount_gradient;
    let amount_gradient = shape.weather_presence * mix(
        raw_amount_gradient,
        rounded_amount_gradient,
        0.45 * shape.detail_blobiness,
    );
    let height_detail = 0.35 + 0.65 * shape.detail_amount;
    let height_scale_m = 180.0;
    let height_m = height_scale_m * shape.cloud_amount * height_detail;
    let height_gradient = height_scale_m * (
        amount_gradient * height_detail
        + 0.65 * shape.cloud_amount * detail_gradient
    );

    var sample: LowCloudSample;
    sample.amount = shape.cloud_amount;
    sample.optical_depth = shape.cloud_amount * (0.35 + 0.90 * shape.detail_amount);
    sample.height_m = height_m;
    sample.darkness = 0.06 * shape.cloud_amount;
    sample.relief = height_m / height_scale_m;
    sample.gradient = height_gradient;
    sample.detail_gate = shape.raw_amount;
    return sample;
}

fn procedural_low_cloud_optical_depth_with_weather(
    position_m: vec2<f32>,
    weather: CloudWeatherSample,
    pixel_footprint_m: f32,
) -> f32 {
    let cover = visible_low_cloud_coverage(weather);
    if cover <= 0.0 {
        return 0.0;
    }
    let raw_detail_value = visual_cloud_noise_value(
        position_m,
        weather.low_advection_wind_m_s,
        vec2<u32>(0x38d01377u, 0x452821e6u),
        320.0,
        12u,
        scene.low_cloud_epoch,
        scene.cloud_anchor_phase_and_inverse_world_size_m.x,
        scene.cloud_anchor_weights.xy,
        1800.0,
        pixel_footprint_m,
    );
    let shape = low_cloud_shape(weather, cover, raw_detail_value);
    return shape.cloud_amount * (0.35 + 0.90 * shape.detail_amount);
}

fn procedural_low_cloud_sample(position_m: vec2<f32>) -> LowCloudSample {
    return procedural_low_cloud_sample_with_weather(
        position_m,
        low_cloud_weather_at(position_m),
        visual_pixel_footprint_m,
    );
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

fn procedural_upper_cloud_optical_depth_filtered(
    position_m: vec2<f32>,
    pixel_footprint_m: f32,
) -> f32 {
    let weather = upper_cloud_weather_at(position_m);
    let regional = regional_canopy_conditions(weather);
    let upper_coverage = clamp(
        max(
            regional.sky_fraction,
            0.30 * weather.low_coverage + 0.70 * weather.rain,
        ),
        0.0,
        1.0,
    );
    if upper_coverage <= 0.0 {
        return 0.0;
    }
    let phase = clamp(scene.cloud_anchor_phase_and_inverse_world_size_m.y, 0.0, 1.0);
    let traced = trace_cloud_positions_m(
        position_m,
        -phase * 3600.0,
        (1.0 - phase) * 3600.0,
        weather.upper_advection_wind_m_s,
    );
    let current_circle_position = periodic_noise_circle_position(traced.current);
    let next_circle_position = periodic_noise_circle_position(traced.next);
    let system_noise = visual_transported_noise_value_from_circles(
        current_circle_position,
        next_circle_position,
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        16384.0,
        4u,
        scene.upper_cloud_epoch,
        scene.cloud_anchor_weights.zw,
        pixel_footprint_m,
    );
    let local_noise = visual_transported_noise_value_from_circles(
        current_circle_position,
        next_circle_position,
        vec2<u32>(0x4cf5ad43u, 0x2745937fu),
        2048.0,
        4u,
        scene.upper_cloud_epoch,
        scene.cloud_anchor_weights.zw,
        pixel_footprint_m,
    );
    let rain = weather.rain;
    let system_amount = clamp(
        upper_coverage + 0.28 * system_noise,
        0.0,
        1.0,
    );
    let connected_cover = max(weather.stratiform, 0.70 * rain);
    let layered_amount = clamp(
        system_amount + mix(0.18, 0.05, connected_cover) * local_noise,
        0.0,
        1.0,
    );
    let local_cells = smoothstep(0.42, 0.72, 0.5 + 0.5 * local_noise);
    let cellular_amount = system_amount * mix(0.20, 1.0, local_cells);
    let cellular_character = weather.convective * (1.0 - 0.65 * rain);
    let cloud_amount = smoothstep(0.0, 0.12, upper_coverage) * mix(
        layered_amount,
        max(0.20 * layered_amount, cellular_amount),
        cellular_character,
    );
    return cloud_amount * regional.mean_optical_thickness;
}

fn procedural_upper_cloud_optical_depth(position_m: vec2<f32>) -> f32 {
    return procedural_upper_cloud_optical_depth_filtered(
        position_m,
        visual_pixel_footprint_m,
    );
}
