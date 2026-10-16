fn scene_surface_at(
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
    water_height_m: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> SceneSurface {
    return composed_scene_surface(
        position_m,
        procedural_terrain_sample_with_climate_from_circle(
            position_m,
            circle_position,
            atmosphere.climate,
        ),
        water_height_m,
        atmosphere,
    );
}
