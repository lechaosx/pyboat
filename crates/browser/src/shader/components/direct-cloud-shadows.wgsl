fn initialize_shadow_cache_debug() {}

fn shadow_cache_debug_mode() -> u32 {
    return 0u;
}

fn shadow_cache_debug_atlas(fragment_position_px: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(0.0);
}

fn shadow_cache_debug_tiles(position_m: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(0.0);
}

fn apply_shadow_cache_debug(color: vec4<f32>) -> vec4<f32> {
    return color;
}

fn record_shadow_cache_lookup(layer: u32, hit: bool) {}

fn sampled_upper_cloud_optical_depth_at(position_m: vec2<f32>) -> f32 {
    let canopy = regional_canopy_conditions_at(position_m);
    return procedural_upper_cloud_optical_depth(position_m, canopy);
}

fn sampled_broad_landform_elevation_m(position_m: vec2<f32>) -> f32 {
    return broad_landform_elevation_m(position_m);
}

fn sampled_cloud_shadow_depths_at(
    projection: CloudShadowProjection,
) -> CloudShadowDepths {
    return procedural_cloud_shadow_depths_at(projection);
}
