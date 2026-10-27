fn scalar_debug_color(value: f32) -> vec3<f32> {
    let amount = clamp(value, 0.0, 1.0);
    return mix(vec3<f32>(0.015, 0.025, 0.035), vec3<f32>(0.92, 0.96, 0.94), amount);
}

fn signed_debug_color(value: f32) -> vec3<f32> {
    let amount = clamp(value, -1.0, 1.0);
    return select(
        mix(vec3<f32>(0.07, 0.10, 0.14), vec3<f32>(0.15, 0.52, 0.86), -amount),
        mix(vec3<f32>(0.07, 0.10, 0.14), vec3<f32>(0.92, 0.52, 0.16), amount),
        amount >= 0.0,
    );
}

fn pressure_anomaly_debug_color(anomaly_hpa: f32, range_hpa: f32) -> vec3<f32> {
    let anomaly = clamp(anomaly_hpa / range_hpa, -1.0, 1.0);
    let neutral = vec3<f32>(0.58, 0.59, 0.60);
    return select(
        mix(neutral, vec3<f32>(0.08, 0.30, 0.88), -anomaly),
        mix(neutral, vec3<f32>(0.90, 0.12, 0.08), anomaly),
        anomaly >= 0.0,
    );
}

fn approximate_shelter(position_m: vec2<f32>, wind_m_s: vec2<f32>) -> f32 {
    // This diagnostic favors shader cost over parity. Gameplay shelter is sampled
    // by `game` and remains authoritative.
    let speed = length(wind_m_s);
    if speed < 0.01 {
        return 1.0;
    }
    let upwind = -wind_m_s / speed;
    var obstruction = 0.0;
    for (var sample_index = 0u; sample_index < 4u; sample_index++) {
        let distance_m = 32.0 * exp2(f32(sample_index) * 1.3333333);
        let terrain = procedural_terrain_sample(position_m + distance_m * upwind);
        obstruction = max(
            obstruction,
            smoothstep(0.0, 1.5, terrain.elevation_m) / (1.0 + distance_m / 256.0),
        );
    }
    return 1.0 - 0.70 * obstruction;
}

fn wind_debug_color(wind_m_s: vec2<f32>) -> vec3<f32> {
    let speed = length(wind_m_s);
    if speed < 0.01 {
        return vec3<f32>(0.06, 0.075, 0.085);
    }
    let direction = wind_m_s / speed;
    let direction_color = vec3<f32>(
        0.5 + 0.5 * direction.x,
        0.5 + 0.5 * direction.y,
        0.5 - 0.5 * direction.x,
    );
    return direction_color * mix(0.35, 1.0, smoothstep(0.0, 20.0, speed));
}
