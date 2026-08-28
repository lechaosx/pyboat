fn diagnostic_filtered_pressure(position_m: vec2<f32>) -> PressureFieldSample {
    return procedural_pressure_sample_filtered(position_m, visual_pixel_footprint_m);
}

fn diagnostic_resolved_surface_wind(
    position_m: vec2<f32>,
    pressure: PressureFieldSample,
) -> vec2<f32> {
    return atmosphere_model.x * wind_from_pressure_gradient(
        pressure.gradient_m_s / 6000.0,
        surface_latitude_degrees_at(position_m),
    );
}

fn diagnostic_upper_wind(
    position_m: vec2<f32>,
    pressure: PressureFieldSample,
) -> vec2<f32> {
    let pressure_wind = atmosphere_model.x * wind_from_pressure_gradient(
        pressure.synoptic_gradient_m_s / 6000.0,
        surface_latitude_degrees_at(position_m),
    );
    let temperature_gradient = surface_air_temperature_anomaly_sample_at(position_m).gradient;
    return 1.15 * pressure_wind
        + 1600.0 * atmosphere_model.z * rotate_left(temperature_gradient);
}

fn diagnostic_surface_wind(
    position_m: vec2<f32>,
    pressure: PressureFieldSample,
    resolved_surface: vec2<f32>,
    upper: vec2<f32>,
) -> vec2<f32> {
    return resolved_surface + surface_gust_velocity_m_s_at(
        position_m,
        pressure,
        resolved_surface,
        upper,
    );
}

fn diagnostic_temperature_gradient(input: VertexOutput) -> vec4<f32> {
    let gradient = 1600.0
        * surface_air_temperature_anomaly_sample_at(input.world_position_m).gradient;
    return vec4<f32>(wind_debug_color(gradient), 1.0);
}

fn diagnostic_surface_air_temperature(input: VertexOutput) -> vec4<f32> {
    return vec4<f32>(signed_debug_color(surface_current_air_temperature_c() / 40.0), 1.0);
}

fn diagnostic_pressure_gradient(input: VertexOutput) -> vec4<f32> {
    let pressure = diagnostic_filtered_pressure(input.world_position_m);
    return vec4<f32>(wind_debug_color(pressure.gradient_m_s), 1.0);
}

fn diagnostic_pressure_broad_anomaly(input: VertexOutput) -> vec4<f32> {
    let pressure = procedural_pressure_sample(input.world_position_m);
    return vec4<f32>(pressure_anomaly_debug_color(
        pressure.climate_potential_m2_s / 6000.0,
        8.0,
    ), 1.0);
}

fn diagnostic_pressure_full_anomaly(input: VertexOutput) -> vec4<f32> {
    let pressure = diagnostic_filtered_pressure(input.world_position_m);
    return vec4<f32>(pressure_anomaly_debug_color(
        pressure.potential_m2_s / 6000.0,
        8.0,
    ), 1.0);
}

fn diagnostic_pressure_synoptic_anomaly(input: VertexOutput) -> vec4<f32> {
    let pressure = procedural_pressure_sample(input.world_position_m);
    return vec4<f32>(pressure_anomaly_debug_color(
        pressure.synoptic_potential_m2_s / 6000.0,
        3.0,
    ), 1.0);
}

fn diagnostic_pressure_local_anomaly(input: VertexOutput) -> vec4<f32> {
    let pressure = procedural_pressure_sample(input.world_position_m);
    return vec4<f32>(pressure_anomaly_debug_color(
        pressure.local_potential_m2_s / 6000.0,
        0.5,
    ), 1.0);
}

fn diagnostic_wind_surface(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let pressure = diagnostic_filtered_pressure(position_m);
    let resolved_surface = diagnostic_resolved_surface_wind(position_m, pressure);
    let upper = diagnostic_upper_wind(position_m, pressure);
    return vec4<f32>(wind_debug_color(diagnostic_surface_wind(
        position_m,
        pressure,
        resolved_surface,
        upper,
    )), 1.0);
}

fn diagnostic_wind_upper(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let pressure = diagnostic_filtered_pressure(position_m);
    return vec4<f32>(wind_debug_color(diagnostic_upper_wind(position_m, pressure)), 1.0);
}

fn diagnostic_wind_shear(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let pressure = diagnostic_filtered_pressure(position_m);
    let resolved_surface = diagnostic_resolved_surface_wind(position_m, pressure);
    let upper = diagnostic_upper_wind(position_m, pressure);
    let surface = diagnostic_surface_wind(position_m, pressure, resolved_surface, upper);
    return vec4<f32>(wind_debug_color(upper - surface), 1.0);
}

fn diagnostic_wind_terrain_shelter(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let pressure = diagnostic_filtered_pressure(position_m);
    let resolved_surface = diagnostic_resolved_surface_wind(position_m, pressure);
    let upper = diagnostic_upper_wind(position_m, pressure);
    let surface = diagnostic_surface_wind(position_m, pressure, resolved_surface, upper);
    let shelter = approximate_shelter(position_m, surface);
    return vec4<f32>(mix(
        vec3<f32>(0.84, 0.17, 0.10),
        vec3<f32>(0.10, 0.72, 0.65),
        shelter,
    ), 1.0);
}

fn diagnostic_wind_surface_gust(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let pressure = procedural_pressure_sample(position_m);
    let resolved_surface = resolved_surface_wind_m_s_at(position_m);
    let upper = upper_wind_m_s_at(position_m);
    let gust = surface_gust_velocity_m_s_at(position_m, pressure, resolved_surface, upper);
    return vec4<f32>(wind_debug_color(gust), 1.0);
}

fn diagnostic_wind_resolved_surface(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let pressure = diagnostic_filtered_pressure(position_m);
    let resolved_surface = diagnostic_resolved_surface_wind(position_m, pressure);
    return vec4<f32>(wind_debug_color(resolved_surface), 1.0);
}

fn diagnostic_wind_broad_synoptic(input: VertexOutput) -> vec4<f32> {
    let position_m = input.world_position_m;
    let pressure = diagnostic_filtered_pressure(position_m);
    let broad_synoptic = atmosphere_model.x * wind_from_pressure_gradient(
        pressure.broad_synoptic_gradient_m_s / 6000.0,
        surface_latitude_degrees_at(position_m),
    );
    return vec4<f32>(wind_debug_color(broad_synoptic), 1.0);
}
