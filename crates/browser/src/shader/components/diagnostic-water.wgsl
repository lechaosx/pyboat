fn diagnostic_water_tide_height(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(signed_debug_color(tide_height_m() / 2.0), 1.0);
}

fn diagnostic_water_ocean_current(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(wind_debug_color(ocean_forcing_at(input.world_position_m).xy), 1.0);
}

fn diagnostic_water_visual_normals(input: VertexOutput) -> vec4<f32> {
    let detail = visual_water_slopes(input.world_position_m);
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

// The wind sea's peak period over water, from 0 to 20 s.
fn diagnostic_water_wave_peak_period(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let wind_m_s = resolved_surface_wind_m_s_from_pressure(
        position_m,
        surface_atmosphere_sample_at(position_m).pressure,
    );
    let period_s = TAU / sqrt(9.81 * wind_sea(length(wind_m_s)).peak_wavenumber_per_m);
    let water = ocean_forcing_at(position_m).z > 0.0;
    return vec4<f32>(scalar_debug_color(select(0.0, period_s / 20.0, water)), 1.0);
}

// The wind sea's direction over water, coloured as a wind of its
// significant height in metres.
fn diagnostic_water_wave_direction(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let wind_m_s = resolved_surface_wind_m_s_from_pressure(
        position_m,
        surface_atmosphere_sample_at(position_m).pressure,
    );
    let height_m = ocean_forcing_at(position_m).z;
    return vec4<f32>(
        wind_debug_color(wind_m_s / max(length(wind_m_s), 1.0e-6) * height_m),
        1.0,
    );
}

fn diagnostic_water_precipitation_contact_normals(input: VertexOutput) -> vec4<f32> {
    let contact = precipitation_water_contact_sample(input.world_position_m);
    let normal = normalize(vec3<f32>(-20.0 * contact.gradient, 1.0));
    return vec4<f32>(0.5 + 0.5 * normal, 1.0);
}
