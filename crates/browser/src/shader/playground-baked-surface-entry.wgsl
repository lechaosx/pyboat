// The level's spacing serves as the pixel footprint, so material octaves too
// short for its lattice fade out rather than alias.
fn initialize_baked_surface_generation(position_m: vec2<f32>, spacing_m: f32) {
    visual_pixel_footprint_m = spacing_m;
    tuning = playground_scene_tuning();
    layer_visibility = vec4<f32>(1.0);
    landscape_visibility = playground.landscape_visibility;
    atmosphere_model = playground.wind_model;
    climate_visibility = playground.climate_visibility;
    surface_position_m = position_m;
}
