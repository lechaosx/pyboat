fn sampled_low_cloud_sample(position_m: vec2<f32>) -> LowCloudSample {
    return procedural_low_cloud_sample(position_m, cloud_weather_at(position_m));
}

fn low_cloud_weather_at(position_m: vec2<f32>) -> CloudWeatherSample {
    return cloud_weather_at(position_m);
}

fn sampled_upper_cloud_optical_depth_at(position_m: vec2<f32>) -> f32 {
    let canopy = regional_canopy_conditions_at(position_m);
    return procedural_upper_cloud_optical_depth(position_m, canopy);
}

fn sampled_broad_landform_elevation_m(position_m: vec2<f32>) -> f32 {
    return broad_landform_elevation_m(position_m);
}

fn sampled_cloud_shadow_depths_at(
    projection: CloudShadowProjection,
    source_index: u32,
) -> CloudShadowDepths {
    return procedural_cloud_shadow_depths_at(projection);
}
