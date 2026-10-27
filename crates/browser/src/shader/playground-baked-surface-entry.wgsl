// The level's spacing serves as the pixel footprint, so material octaves too
// short for its lattice fade out rather than alias.
fn initialize_baked_surface_generation(position_m: vec2<f32>, spacing_m: f32) {
    visual_pixel_footprint_m = spacing_m;
    tuning = playground_scene_tuning();
    landscape_visibility = playground.landscape_visibility;
    surface_position_m = position_m;
}
