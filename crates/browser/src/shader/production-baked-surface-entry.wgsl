// The level's spacing serves as the pixel footprint, so material octaves too
// short for its lattice fade out rather than alias.
fn initialize_baked_surface_generation(position_m: vec2<f32>, spacing_m: f32) {
    visual_pixel_footprint_m = spacing_m;
    tuning = production_tuning();
    landscape_visibility = vec4<f32>(1.0);
    atmosphere_model = vec4<f32>(1.0);
    climate_visibility = vec4<f32>(1.0);
    surface_position_m = position_m;
}
