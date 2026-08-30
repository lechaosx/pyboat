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

fn low_cloud_kernel_field_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    let kernel_count = u32(clamp(
        round(scene.low_cloud_kernel_count_and_padding.x),
        0.0,
        256.0,
    ));
    for (var kernel_index = 0u; kernel_index < kernel_count; kernel_index++) {
        let extent = scene.low_cloud_kernel_vectors[2u * kernel_index];
        let shape = scene.low_cloud_kernel_vectors[2u * kernel_index + 1u];
        let radius_m = extent.z;
        if radius_m <= 0.0 || extent.w <= 0.0 {
            continue;
        }
        var offset_m = position_m - extent.xy;
        offset_m.x -= scene.world_size_m.x * round(
            offset_m.x * scene.cloud_anchor_phase_and_inverse_world_size_m.z,
        );
        let axis = shape.xy;
        let perpendicular = vec2<f32>(-axis.y, axis.x);
        let aspect_ratio = max(shape.z, 0.01);
        let along_m = dot(offset_m, axis);
        let across_m = dot(offset_m, perpendicular);
        let inverse_radius_squared = 1.0 / (radius_m * radius_m);
        let normalized_radius_squared = inverse_radius_squared * (
            along_m * along_m / (aspect_ratio * aspect_ratio)
                + across_m * across_m
        );
        if normalized_radius_squared < 1.0 {
            let interior = 1.0 - normalized_radius_squared;
            let amplitude = extent.w;
            sample.value += amplitude * interior * interior * interior;
            let normalized_radius_gradient = 2.0 * inverse_radius_squared * (
                along_m / (aspect_ratio * aspect_ratio) * axis
                    + across_m * perpendicular
            );
            sample.gradient -= 3.0 * amplitude * interior * interior
                * normalized_radius_gradient;
        }
    }
    return sample;
}

fn low_cloud_kernel_shape(field_value: f32) -> LowCloudShape {
    let detail_character = clamp(tuning.cloud_detail.x, 0.0, 1.0);
    let characterized_field = mix(
        field_value,
        sqrt(max(field_value, 0.0)),
        detail_character,
    );
    let detail_contrast = max(tuning.cloud_detail.z, 0.01);
    let unclamped_detail_amount = detail_contrast * characterized_field;
    let detail_amount = clamp(unclamped_detail_amount, 0.0, 1.0);
    let target_coverage = clamp(tuning.cloud_wisps.x, 0.0, 0.95);
    let body_threshold = mix(0.58, 0.28, sqrt(target_coverage));
    let edge_softness = max(tuning.cloud_detail.w, 0.005);
    let amount_span = max(
        0.005,
        (1.0 - body_threshold) * clamp(edge_softness / 0.06, 0.1, 2.0),
    );
    let raw_amount = clamp(
        (detail_amount - body_threshold) / amount_span,
        0.0,
        1.0,
    );
    let rounded_amount = raw_amount * raw_amount * (3.0 - 2.0 * raw_amount);
    let detail_blobiness = clamp(tuning.cloud_detail.y, 0.0, 1.0);
    let cloud_amount = mix(
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
        1.0,
    );
}

fn procedural_low_cloud_sample(position_m: vec2<f32>) -> LowCloudSample {
    if scene.low_cloud_kernel_count_and_padding.x <= 0.0 {
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
    let kernel_field = low_cloud_kernel_field_sample(position_m);
    let shape = low_cloud_kernel_shape(kernel_field.value);
    let detail_contrast = max(tuning.cloud_detail.z, 0.01);
    let characterized_gradient_scale = mix(
        1.0,
        0.5 / sqrt(max(kernel_field.value, 0.0001)),
        shape.detail_character,
    );
    let pillow_gradient = characterized_gradient_scale * kernel_field.gradient;
    let detail_gradient = select(
        vec2<f32>(0.0),
        detail_contrast * pillow_gradient,
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
        + shape.cloud_amount * (0.35 * detail_gradient + 0.18 * pillow_gradient)
    );

    var sample: LowCloudSample;
    sample.amount = shape.cloud_amount;
    sample.optical_depth = shape.cloud_amount * (0.35 + 0.90 * shape.detail_amount);
    sample.height_m = height_m;
    sample.darkness = 0.06 * shape.cloud_amount;
    sample.relief = height_m / height_scale_m;
    let normal_strength = mix(0.20, 1.0, clamp(tuning.cloud_wisps.y, 0.0, 1.0));
    sample.gradient = normal_strength * height_gradient;
    sample.detail_gate = shape.raw_amount;
    return sample;
}

fn procedural_low_cloud_optical_depth(position_m: vec2<f32>) -> f32 {
    if scene.low_cloud_kernel_count_and_padding.x <= 0.0 {
        return 0.0;
    }
    let kernel_field = low_cloud_kernel_field_sample(position_m);
    let shape = low_cloud_kernel_shape(kernel_field.value);
    return shape.cloud_amount * (0.35 + 0.90 * shape.detail_amount);
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

fn ambient_canopy_conditions(weather: CloudWeatherSample) -> RegionalCanopyConditions {
    let regional = regional_canopy_conditions(weather);
    return RegionalCanopyConditions(
        0.55 * regional.sky_fraction,
        mix(0.35, regional.mean_optical_thickness, 0.55),
    );
}

fn upper_cloud_kernel_field_value(position_m: vec2<f32>) -> f32 {
    var value = 0.0;
    let kernel_count = u32(clamp(
        round(scene.upper_cloud_kernel_count_and_padding.x),
        0.0,
        96.0,
    ));
    for (var kernel_index = 0u; kernel_index < kernel_count; kernel_index++) {
        let extent = scene.upper_cloud_kernel_vectors[2u * kernel_index];
        let shape = scene.upper_cloud_kernel_vectors[2u * kernel_index + 1u];
        let radius_m = extent.z;
        if radius_m <= 0.0 || extent.w <= 0.0 {
            continue;
        }
        var offset_m = position_m - extent.xy;
        offset_m.x -= scene.world_size_m.x * round(
            offset_m.x * scene.cloud_anchor_phase_and_inverse_world_size_m.z,
        );
        let axis = shape.xy;
        let perpendicular = vec2<f32>(-axis.y, axis.x);
        let aspect_ratio = max(shape.z, 0.01);
        let along_m = dot(offset_m, axis);
        let across_m = dot(offset_m, perpendicular);
        let normalized_radius_squared = (
            along_m * along_m / (aspect_ratio * aspect_ratio)
                + across_m * across_m
        ) / (radius_m * radius_m);
        if normalized_radius_squared < 1.0 {
            let interior = 1.0 - normalized_radius_squared;
            value += extent.w * interior * interior * interior;
        }
    }
    return value;
}

fn procedural_upper_cloud_optical_depth_filtered(
    position_m: vec2<f32>,
    _pixel_footprint_m: f32,
) -> f32 {
    if scene.upper_cloud_kernel_count_and_padding.x <= 0.0 {
        return 0.0;
    }
    let field = upper_cloud_kernel_field_value(position_m);
    return smoothstep(0.02, 0.20, field) * min(field, 4.0);
}

fn procedural_upper_cloud_optical_depth(position_m: vec2<f32>) -> f32 {
    return procedural_upper_cloud_optical_depth_filtered(
        position_m,
        visual_pixel_footprint_m,
    );
}
