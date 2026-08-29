fn diagnostic_climate_solid_ice(input: VertexOutput) -> vec4<f32> {
    let terrain = procedural_terrain_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(terrain.ice_amount), 1.0);
}

fn diagnostic_climate_autumn_vegetation(input: VertexOutput) -> vec4<f32> {
    let terrain = procedural_terrain_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(
        autumn_vegetation_amount(terrain.elevation_m),
    ), 1.0);
}

fn diagnostic_climate_annual_temperature(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(scalar_debug_color(
        (surface_annual_mean_air_temperature_c() + 15.0) / 42.9,
    ), 1.0);
}

fn diagnostic_climate_spatial_snow(input: VertexOutput) -> vec4<f32> {
    let terrain = procedural_terrain_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(local_snow_cover(terrain.elevation_m)), 1.0);
}

fn diagnostic_climate_spatial_ice(input: VertexOutput) -> vec4<f32> {
    let maritime_temperature = 16.0
        * (1.0 - smoothstep(75.0, 90.0, abs(surface_latitude_degrees())))
        * terrain_simplex_fractal_value(
            vec2<u32>(0x78bd642fu, 0xa0761d64u),
            surface_position_m,
            8192.0,
            3u,
            0.5,
            TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
        );
    let water_ice = 1.0 - smoothstep(
        -2.0,
        0.0,
        water_temperature_c() + maritime_temperature,
    );
    return vec4<f32>(scalar_debug_color(water_ice), 1.0);
}

fn diagnostic_climate_composition(input: VertexOutput) -> vec4<f32> {
    let terrain = procedural_terrain_sample(input.world_position_m);
    return vec4<f32>(
        clamp((surface_annual_mean_air_temperature_c() + 15.0) / 42.9, 0.0, 1.0),
        local_snow_cover(terrain.elevation_m),
        terrain.ice_amount,
        1.0,
    );
}
