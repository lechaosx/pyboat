fn scene_surface_at(
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
    water_height_m: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> SceneSurface {
    let terrain = procedural_terrain_sample_with_climate_from_circle(
        position_m,
        circle_position,
        atmosphere.climate,
    );
    return composed_scene_surface(
        position_m,
        terrain,
        direct_sea_ice_detail(position_m, terrain),
        water_height_m,
        atmosphere,
    );
}
