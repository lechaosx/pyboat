fn initialize_cloud_shadow_cache_generation(position_m: vec2<f32>) {
    visual_pixel_footprint_m = scene.world_phases_and_pixel_footprint_m.w;
    tuning = playground_scene_tuning();
    layer_visibility = vec4<f32>(1.0);
    landscape_visibility = playground.landscape_visibility;
    atmosphere_model = playground.wind_model;
    climate_visibility = playground.climate_visibility;
    surface_position_m = position_m;
}

fn initialize_cloud_shadow_cache_discovery(position_m: vec2<f32>) {
    initialize_cloud_shadow_cache_generation(position_m);
    initialize_surface_tide_value();
    initialize_celestial_lighting();
}
