fn initialize_occluder_field_generation(position_m: vec2<f32>) {
    visual_pixel_footprint_m = scene.world_phases_and_pixel_footprint_m.w;
    tuning = playground_scene_tuning();
    landscape_visibility = playground.landscape_visibility;
    surface_position_m = position_m;
}
