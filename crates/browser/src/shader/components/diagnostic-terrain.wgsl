fn diagnostic_terrain_elevation(input: VertexOutput) -> vec4<f32> {
    let terrain = procedural_terrain_sample(input.world_position_m);
    return vec4<f32>(signed_debug_color(terrain.elevation_m / 12.0), 1.0);
}

fn diagnostic_terrain_slope(input: VertexOutput) -> vec4<f32> {
    let terrain = procedural_terrain_sample(input.world_position_m);
    return vec4<f32>(scalar_debug_color(length(terrain.gradient) / 0.25), 1.0);
}

fn diagnostic_terrain_normals(input: VertexOutput) -> vec4<f32> {
    let terrain = procedural_terrain_sample(input.world_position_m);
    let normal = normalize(vec3<f32>(-terrain.gradient, 1.0));
    return vec4<f32>(0.5 + 0.5 * normal, 1.0);
}

fn diagnostic_terrain_broad_landform(input: VertexOutput) -> vec4<f32> {
    let elevation_m = broad_landform_elevation_m(input.world_position_m);
    return vec4<f32>(signed_debug_color(elevation_m / 8.0), 1.0);
}

fn diagnostic_terrain_island_clusters(input: VertexOutput) -> vec4<f32> {
    let cluster = terrain_simplex_fractal_value(
        vec2<u32>(0x78bd642fu, 0xa0761d64u),
        input.world_position_m,
        8192.0,
        3u,
        0.5,
        TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
    );
    return vec4<f32>(signed_debug_color(cluster), 1.0);
}

fn diagnostic_terrain_island_detail(input: VertexOutput) -> vec4<f32> {
    let island = terrain_simplex_fractal_value(
        vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        input.world_position_m,
        1536.0,
        5u,
        0.35,
        TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT,
    );
    return vec4<f32>(signed_debug_color(island), 1.0);
}

fn diagnostic_terrain_coastal_reef(input: VertexOutput) -> vec4<f32> {
    let reef = terrain_simplex_fractal_value(
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        input.world_position_m,
        32.0,
        6u,
        0.55,
        FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
    );
    return vec4<f32>(signed_debug_color(reef), 1.0);
}

fn diagnostic_material_vegetation(input: VertexOutput) -> vec4<f32> {
    let vegetation = material_fractal_value(
        vec2<u32>(0x510e527fu, 0x9b05688cu),
        input.world_position_m,
        1536.0,
        4u,
    );
    return vec4<f32>(signed_debug_color(vegetation), 1.0);
}

fn diagnostic_material_coastal_character(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let substrate = terrain_simplex_fractal_value(
        vec2<u32>(0x7f4a7c15u, 0x9e3779b9u),
        position_m,
        1536.0,
        3u,
        0.5,
        TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
    );
    let ruggedness_noise = terrain_simplex_fractal_value(
        vec2<u32>(0x1ce4e5b9u, 0xbf58476du),
        position_m,
        2048.0,
        3u,
        0.5,
        TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
    );
    let rockiness = smoothstep(-0.25, 0.35, substrate);
    let ruggedness = smoothstep(-0.25, 0.25, ruggedness_noise);
    let sediment = (1.0 - rockiness) * (1.0 - ruggedness);
    // RGB preserves the independent continuous causes: substrate rockiness,
    // depositional sediment, and rugged exposure.
    return vec4<f32>(rockiness, sediment, ruggedness, 1.0);
}
