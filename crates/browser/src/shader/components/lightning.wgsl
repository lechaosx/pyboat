fn lightning_delta_m(position_m: vec2<f32>, source: vec4<f32>) -> vec2<f32> {
    var delta = source.xy - camera_relative_position_m(position_m);
    delta.x -= round(delta.x / scene.world_size_m.x) * scene.world_size_m.x;
    return delta;
}

// A blocker at fraction t of the finite ray hides receivers below
// (blocker_height - t * source_height) / (1 - t).
fn finite_lightning_horizon(position_m: vec2<f32>, source: vec4<f32>) -> f32 {
    let delta = lightning_delta_m(position_m, source);
    let reach_m = length(delta);
    if reach_m < 1.0 {
        return LANDFORM_UNSHADOWED_HEIGHT_M;
    }
    let direction = delta / reach_m;
    var horizon_m = LANDFORM_UNSHADOWED_HEIGHT_M;
    let highest_blocker_m = 252.0 * TERRAIN_ELEVATION_SCALE;
    let last_fraction = min((highest_blocker_m + 5.0) / (source.z + 5.0), 1.0);
    let blocker_reach_m = reach_m * last_fraction;
    var distance_m = min(LANDFORM_HORIZON_STEP_M, blocker_reach_m);
    for (var step = 0u; step < LANDFORM_HORIZON_MAX_STEPS; step++) {
        if distance_m >= reach_m { break; }
        let fraction = distance_m / reach_m;
        let blocker_m = sampled_broad_landform_elevation_m(position_m + direction * distance_m);
        horizon_m = max(horizon_m, (blocker_m - fraction * source.z) / (1.0 - fraction));
        if distance_m >= blocker_reach_m { break; }
        distance_m = min(blocker_reach_m, distance_m + max(LANDFORM_HORIZON_STEP_M, LANDFORM_HORIZON_STEP_RATIO * distance_m));
    }
    return horizon_m;
}

struct LightningLight {
    direction: vec3<f32>,
    irradiance: vec3<f32>,
};

fn lightning_light(receiver: CelestialReceiverLight, source_index: u32) -> LightningLight {
    let source = scene.lightning_sources[source_index];
    let delta = vec3<f32>(lightning_delta_m(receiver.position_m, source), source.z - receiver.height_m);
    let squared_distance = dot(delta, delta);
    let direction = delta * inverseSqrt(squared_distance);
    let terrain_visibility = select(0.0, 1.0,
        receiver.height_m > sampled_lightning_horizon(receiver.position_m, source_index));
    let boat_visibility = boat_shadow_transmittance(
        vec3<f32>(camera_relative_position_m(receiver.position_m), receiver.height_m), source_index + 2u);
    return LightningLight(direction, vec3<f32>(0.85, 0.92, 1.0)
        * (source.w * source.z * source.z / squared_distance)
        * terrain_visibility * boat_visibility);
}

fn lightning_matte_light(receiver: CelestialReceiverLight, normal: vec3<f32>) -> vec3<f32> {
    var light = vec3<f32>(0.0);
    for (var source_index = 0u; source_index < 4u; source_index++) {
        if scene.lightning_sources[source_index].w > 0.0 {
            let source = lightning_light(receiver, source_index);
            light += source.irradiance * max(dot(normal, source.direction), 0.0);
        }
    }
    return light;
}
