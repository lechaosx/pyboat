override TERRAIN_GEOMETRY_OUTPUT: u32 = 0u;

@fragment
fn fragment_terrain_geometry(input: VertexOutput) -> @location(0) vec4<f32> {
    let periodic_input = initialize_playground_base(input);
    if TERRAIN_GEOMETRY_OUTPUT == 0u {
        return diagnostic_terrain_elevation(periodic_input);
    }
    if TERRAIN_GEOMETRY_OUTPUT == 1u {
        return diagnostic_terrain_slope(periodic_input);
    }
    return diagnostic_terrain_normals(periodic_input);
}
