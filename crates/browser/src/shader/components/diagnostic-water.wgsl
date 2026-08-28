fn diagnostic_water_tide_height(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(signed_debug_color(tide_height_m() / 2.0), 1.0);
}

fn diagnostic_water_ocean_current(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(wind_debug_color(ocean_forcing_at(input.world_position_m).xy), 1.0);
}

fn diagnostic_water_visual_normals(input: VertexOutput) -> vec4<f32> {
    let detail = visual_water_normal_sample(input.world_position_m);
    let normal = normalize(vec3<f32>(-detail.gradient, 1.0));
    return vec4<f32>(0.5 + 0.5 * normal, 1.0);
}

fn diagnostic_water_physical_wave_normals(input: VertexOutput) -> vec4<f32> {
    let waves = physical_water_wave_sample(input.world_position_m);
    let normal = normalize(vec3<f32>(-waves.gradient, 1.0));
    return vec4<f32>(0.5 + 0.5 * normal, 1.0);
}

fn diagnostic_water_significant_wave_height(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(scalar_debug_color(
        ocean_forcing_at(input.world_position_m).z / 6.0,
    ), 1.0);
}

fn diagnostic_water_precipitation_contact_normals(input: VertexOutput) -> vec4<f32> {
    let contact = precipitation_water_contact_sample(input.world_position_m);
    let normal = normalize(vec3<f32>(-20.0 * contact.gradient, 1.0));
    return vec4<f32>(0.5 + 0.5 * normal, 1.0);
}
