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

fn surface_gust_velocity_m_s(position_m: vec2<f32>, resolved_m_s: vec2<f32>) -> vec2<f32> {
    return surface_gust_velocity_m_s_at(
        position_m,
        procedural_pressure_sample_with_surface_gust(position_m),
        resolved_m_s,
        upper_wind_m_s_at(position_m),
    );
}
