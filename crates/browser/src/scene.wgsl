struct SceneUniforms {
    camera_and_view_m: vec4<f32>,
    weather: vec4<f32>,
    weather_structure: vec4<f32>,
    sun_direction_and_intensity: vec4<f32>,
    match_seed: vec2<u32>,
    cloud_visual_seed: vec2<u32>,
    cloud_offsets_m: vec4<f32>,
    wind_vectors_m_s: vec4<f32>,
    time_seconds: f32,
};

struct SceneTuning {
    cloud_detail: vec4<f32>,
    cloud_formation: vec4<f32>,
    cloud_rendering: vec4<f32>,
    cloud_behavior: vec4<f32>,
    terrain_tuning: vec4<f32>,
};

struct PlaygroundUniforms {
    cloud_detail: vec4<f32>,
    cloud_formation: vec4<f32>,
    cloud_rendering: vec4<f32>,
    cloud_behavior: vec4<f32>,
    terrain_tuning: vec4<f32>,
    layer_visibility: vec4<f32>,
    landscape_visibility: vec4<f32>,
    wind_model: vec4<f32>,
    diagnostics: vec4<u32>,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;

@group(1) @binding(0)
var<uniform> playground: PlaygroundUniforms;

override CLOUD_DETAIL_CHARACTER: f32 = 0.35;
override CLOUD_DETAIL_BLOBINESS: f32 = 0.25;
override CLOUD_DETAIL_CONTRAST: f32 = 1.15;
override CLOUD_DETAIL_EDGE_SOFTNESS: f32 = 0.06;
override CLOUD_MASK_INFLUENCE: f32 = 0.85;
override CLOUD_MASK_SCALE: f32 = 1.0;
override CLOUD_MASK_ELONGATION: f32 = 2.4;
override CLOUD_MAX_VISIBLE_COVERAGE: f32 = 0.5;
override CLOUD_OPTICAL_DEPTH: f32 = 1.0;
override CLOUD_OPACITY: f32 = 1.0;
override CLOUD_DARKNESS: f32 = 1.0;
override CLOUD_RELIEF: f32 = 1.0;
override CLOUD_MOTION_IN_MASK: f32 = 0.65;
override TERRAIN_HORIZONTAL_SCALE: f32 = 1.0;
override TERRAIN_ELEVATION_SCALE: f32 = 3.0;
override TERRAIN_CLIFF_SLOPE: f32 = 0.08;

var<private> tuning: SceneTuning;
var<private> layer_visibility: vec4<f32>;
var<private> landscape_visibility: vec4<f32>;

fn production_tuning() -> SceneTuning {
    return SceneTuning(
        vec4<f32>(
            CLOUD_DETAIL_CHARACTER,
            CLOUD_DETAIL_BLOBINESS,
            CLOUD_DETAIL_CONTRAST,
            CLOUD_DETAIL_EDGE_SOFTNESS,
        ),
        vec4<f32>(
            CLOUD_MASK_INFLUENCE,
            CLOUD_MASK_SCALE,
            CLOUD_MASK_ELONGATION,
            CLOUD_MAX_VISIBLE_COVERAGE,
        ),
        vec4<f32>(
            CLOUD_OPTICAL_DEPTH,
            CLOUD_OPACITY,
            CLOUD_DARKNESS,
            CLOUD_RELIEF,
        ),
        vec4<f32>(CLOUD_MOTION_IN_MASK, 0.0, 0.0, 0.0),
        vec4<f32>(
            TERRAIN_HORIZONTAL_SCALE,
            TERRAIN_ELEVATION_SCALE,
            TERRAIN_CLIFF_SLOPE,
            0.0,
        ),
    );
}

fn playground_scene_tuning() -> SceneTuning {
    return SceneTuning(
        playground.cloud_detail,
        playground.cloud_formation,
        playground.cloud_rendering,
        playground.cloud_behavior,
        playground.terrain_tuning,
    );
}

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) is_terrain: f32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_position_m: vec2<f32>,
    @location(2) @interpolate(flat) is_terrain: f32,
};

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.color = input.color;
    output.world_position_m = scene.camera_and_view_m.xy + vec2<f32>(
        input.position.x * scene.camera_and_view_m.z * 0.5,
        -input.position.y * scene.camera_and_view_m.w * 0.5,
    );
    output.is_terrain = input.is_terrain;
    return output;
}

fn multiply_high_u32(a: u32, b: u32) -> u32 {
    let a_low = a & 0xffffu;
    let a_high = a >> 16u;
    let b_low = b & 0xffffu;
    let b_high = b >> 16u;
    let low_product = a_low * b_low;
    let middle = a_high * b_low + (low_product >> 16u);
    let middle_low = middle & 0xffffu;
    let middle_high = middle >> 16u;
    let upper_middle = a_low * b_high + middle_low;
    return a_high * b_high + middle_high + (upper_middle >> 16u);
}

fn multiply_u64(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    return vec2<u32>(
        a.x * b.x,
        multiply_high_u32(a.x, b.x) + a.x * b.y + a.y * b.x,
    );
}

fn shift_right_u64(value: vec2<u32>, amount: u32) -> vec2<u32> {
    return vec2<u32>(
        (value.x >> amount) | (value.y << (32u - amount)),
        value.y >> amount,
    );
}

fn signed_u64(value: i32) -> vec2<u32> {
    return vec2<u32>(
        bitcast<u32>(value),
        select(0u, 0xffffffffu, value < 0),
    );
}

fn terrain_hash(initial_value: vec2<u32>) -> vec2<u32> {
    var value = initial_value;
    value = multiply_u64(
        value ^ shift_right_u64(value, 30u),
        vec2<u32>(0x1ce4e5b9u, 0xbf58476du),
    );
    value = multiply_u64(
        value ^ shift_right_u64(value, 27u),
        vec2<u32>(0x133111ebu, 0x94d049bbu),
    );
    return value ^ shift_right_u64(value, 31u);
}

fn terrain_hash_unit(value: vec2<u32>) -> f32 {
    let hash = terrain_hash(value);
    return f32(hash.y) * 2.3283064365386963e-10
        + f32(hash.x) * 5.421010862427522e-20;
}

fn terrain_lattice_value(seed: vec2<u32>, x: i32, y: i32) -> f32 {
    let key = seed
        ^ multiply_u64(
            signed_u64(x),
            vec2<u32>(0x7f4a7c15u, 0x9e3779b9u),
        )
        ^ multiply_u64(
            signed_u64(y),
            vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        );
    return 2.0 * terrain_hash_unit(key) - 1.0;
}

struct TerrainNoiseSample {
    value: f32,
    gradient: vec2<f32>,
};

struct TerrainSample {
    elevation_m: f32,
    gradient: vec2<f32>,
};

fn terrain_value_noise(
    seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
) -> TerrainNoiseSample {
    let scaled_position = position_m / wavelength_m;
    let cell = vec2<i32>(floor(scaled_position));
    let fraction = scaled_position - vec2<f32>(cell);
    let blend = fraction * fraction * (vec2<f32>(3.0) - 2.0 * fraction);
    let blend_derivative = 6.0 * fraction * (vec2<f32>(1.0) - fraction) / wavelength_m;
    let bottom_left = terrain_lattice_value(seed, cell.x, cell.y);
    let bottom_right = terrain_lattice_value(seed, cell.x + 1, cell.y);
    let top_left = terrain_lattice_value(seed, cell.x, cell.y + 1);
    let top_right = terrain_lattice_value(seed, cell.x + 1, cell.y + 1);
    let bottom = mix(bottom_left, bottom_right, blend.x);
    let top = mix(top_left, top_right, blend.x);

    var sample: TerrainNoiseSample;
    sample.value = mix(bottom, top, blend.y);
    sample.gradient = vec2<f32>(
        mix(bottom_right - bottom_left, top_right - top_left, blend.y)
            * blend_derivative.x,
        (top - bottom) * blend_derivative.y,
    );
    return sample;
}

fn terrain_fractal_sample(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
) -> TerrainNoiseSample {
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_seed = scene.match_seed
            ^ field_salt
            ^ multiply_u64(
                vec2<u32>(octave, 0u),
                vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            );
        let octave_sample = terrain_value_noise(octave_seed, position_m, wavelength_m);
        sample.value += amplitude * octave_sample.value;
        sample.gradient += amplitude * octave_sample.gradient;
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.5;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
    return sample;
}

fn visual_cloud_noise_sample(
    position_m: vec2<f32>,
    field_salt: vec2<u32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    initial_lifetime_seconds: f32,
) -> TerrainNoiseSample {
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_seed = scene.cloud_visual_seed
            ^ field_salt
            ^ multiply_u64(
                vec2<u32>(octave, 0u),
                vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            );
        let lifetime_seconds = initial_lifetime_seconds
            * sqrt(wavelength_m / initial_wavelength_m);
        let scaled_time = scene.time_seconds / lifetime_seconds;
        let time_cell = i32(floor(scaled_time));
        let time_blend = smoothstep(0.0, 1.0, fract(scaled_time));
        let first_seed = octave_seed
            ^ multiply_u64(
                signed_u64(time_cell),
                vec2<u32>(0x94d049bbu, 0x133111ebu),
            );
        let second_seed = octave_seed
            ^ multiply_u64(
                signed_u64(time_cell + 1),
                vec2<u32>(0x94d049bbu, 0x133111ebu),
            );
        let first = terrain_value_noise(first_seed, position_m, wavelength_m);
        let second = terrain_value_noise(second_seed, position_m, wavelength_m);
        sample.value += amplitude * mix(first.value, second.value, time_blend);
        let gradient_weight = amplitude * sqrt(wavelength_m / initial_wavelength_m);
        sample.gradient += gradient_weight * mix(first.gradient, second.gradient, time_blend);
        amplitude_sum += amplitude;
        amplitude *= 0.58;
        wavelength_m *= 0.5;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
    return sample;
}

fn low_cloud_detail_coordinates(position_m: vec2<f32>) -> vec2<f32> {
    return position_m
        - scene.cloud_offsets_m.xy * (1.0 - clamp(tuning.cloud_behavior.x, 0.0, 1.0));
}

fn low_cloud_mask_coordinates(position_m: vec2<f32>) -> vec2<f32> {
    let translated_m = position_m
        - scene.cloud_offsets_m.xy * clamp(tuning.cloud_behavior.x, 0.0, 1.0);
    let wind_angle = atan2(scene.wind_vectors_m_s.y, scene.wind_vectors_m_s.x);
    let along_wind = vec2<f32>(cos(wind_angle), sin(wind_angle));
    let across_wind = vec2<f32>(-along_wind.y, along_wind.x);
    let mask_scale = max(tuning.cloud_formation.y, 0.01);
    let elongation = max(tuning.cloud_formation.z, 1.0);
    return vec2<f32>(
        dot(translated_m, along_wind) / elongation,
        dot(translated_m, across_wind),
    ) / mask_scale;
}

fn low_cloud_mask_gradient_to_world(gradient: vec2<f32>) -> vec2<f32> {
    let wind_angle = atan2(scene.wind_vectors_m_s.y, scene.wind_vectors_m_s.x);
    let along_wind = vec2<f32>(cos(wind_angle), sin(wind_angle));
    let across_wind = vec2<f32>(-along_wind.y, along_wind.x);
    let mask_scale = max(tuning.cloud_formation.y, 0.01);
    let elongation = max(tuning.cloud_formation.z, 1.0);
    return along_wind * gradient.x / (elongation * mask_scale)
        + across_wind * gradient.y / mask_scale;
}

fn smoothstep_gradient(
    lower: f32,
    upper: f32,
    value: f32,
    gradient: vec2<f32>,
) -> vec2<f32> {
    let amount = clamp((value - lower) / (upper - lower), 0.0, 1.0);
    let inside = select(0.0, 1.0, value > lower && value < upper);
    return gradient * inside * 6.0 * amount * (1.0 - amount) / (upper - lower);
}

struct LowCloudSample {
    density: f32,
    optical_depth: f32,
    height_m: f32,
    darkness: f32,
    relief: f32,
    gradient: vec2<f32>,
    detail_amount: f32,
    detail_gate: f32,
    mask_amount: f32,
    mask_gate: f32,
};

struct UpperCloudSample {
    optical_depth: f32,
};

fn procedural_low_cloud_sample(position_m: vec2<f32>) -> LowCloudSample {
    let cover = scene.weather.z;
    let rain = scene.weather.w;
    if cover <= 0.0 {
        var empty: LowCloudSample;
        empty.density = 0.0;
        empty.optical_depth = 0.0;
        empty.height_m = 0.0;
        empty.darkness = 0.0;
        empty.relief = 0.0;
        empty.gradient = vec2<f32>(0.0);
        empty.detail_amount = 0.0;
        empty.detail_gate = 0.0;
        empty.mask_amount = 0.0;
        empty.mask_gate = 0.0;
        return empty;
    }
    let raw_detail = visual_cloud_noise_sample(
        low_cloud_detail_coordinates(position_m),
        vec2<u32>(0x38d01377u, 0x452821e6u),
        144.0,
        7u,
        150.0,
    );
    let detail_character = clamp(tuning.cloud_detail.x, 0.0, 1.0);
    let billow_sign = select(-1.0, 1.0, raw_detail.value >= 0.0);
    var detail: TerrainNoiseSample;
    detail.value = mix(
        raw_detail.value,
        2.0 * abs(raw_detail.value) - 1.0,
        detail_character,
    );
    detail.gradient = mix(
        raw_detail.gradient,
        2.0 * billow_sign * raw_detail.gradient,
        detail_character,
    );
    let detail_contrast = max(tuning.cloud_detail.z, 0.01);
    let unclamped_detail_amount = 0.5 + 0.5 * detail_contrast * detail.value;
    let detail_amount = clamp(unclamped_detail_amount, 0.0, 1.0);
    let detail_gradient = select(
        vec2<f32>(0.0),
        0.5 * detail_contrast * detail.gradient,
        unclamped_detail_amount > 0.0 && unclamped_detail_amount < 1.0,
    );

    let mask_noise = visual_cloud_noise_sample(
        low_cloud_mask_coordinates(position_m),
        vec2<u32>(0xa24baed4u, 0x94d049bbu),
        640.0,
        4u,
        600.0,
    );
    let target_coverage = clamp(
        tuning.cloud_formation.w * smoothstep(0.02, 0.95, cover),
        0.0,
        0.95,
    );
    let body_threshold = mix(0.78, 0.32, sqrt(target_coverage));
    let edge_softness = max(tuning.cloud_detail.w, 0.005);
    let detail_gate = smoothstep(
        body_threshold,
        body_threshold + edge_softness,
        detail_amount,
    );
    let mask_amount = clamp(0.5 + 0.5 * mask_noise.value, 0.0, 1.0);
    let mask_gradient = low_cloud_mask_gradient_to_world(0.5 * mask_noise.gradient);
    let mask_gate = smoothstep(
        body_threshold,
        body_threshold + edge_softness,
        mask_amount,
    );
    let material_density = smoothstep(0.08, 0.72, detail_amount);
    let detail_envelope = mix(
        material_density,
        detail_gate,
        clamp(tuning.cloud_detail.y, 0.0, 1.0),
    );
    let mask_envelope = mix(
        1.0,
        mask_gate,
        clamp(tuning.cloud_formation.x, 0.0, 1.0),
    );
    let weather_presence = smoothstep(0.0, 0.08, cover);
    let density = weather_presence * detail_envelope * mask_envelope * material_density;
    let detail_gate_gradient = smoothstep_gradient(
        body_threshold,
        body_threshold + edge_softness,
        detail_amount,
        detail_gradient,
    );
    let mask_gate_gradient = smoothstep_gradient(
        body_threshold,
        body_threshold + edge_softness,
        mask_amount,
        mask_gradient,
    );
    let material_gradient = smoothstep_gradient(
        0.08,
        0.72,
        detail_amount,
        detail_gradient,
    );
    let detail_envelope_gradient = mix(
        material_gradient,
        detail_gate_gradient,
        clamp(tuning.cloud_detail.y, 0.0, 1.0),
    );
    let mask_envelope_gradient = mask_gate_gradient
        * clamp(tuning.cloud_formation.x, 0.0, 1.0);
    let density_gradient = weather_presence * (
        detail_envelope_gradient * mask_envelope * material_density
        + detail_envelope * mask_envelope_gradient * material_density
        + detail_envelope * mask_envelope * material_gradient
    );
    let height_detail = 0.35 + 0.65 * detail_amount;
    let height_m = 320.0 * density * density * height_detail;
    let height_gradient = 320.0 * (
        2.0 * density * density_gradient * height_detail
        + 0.65 * density * density * detail_gradient
    );

    var sample: LowCloudSample;
    sample.density = density;
    sample.optical_depth = density
        * (0.16 + 0.62 * detail_amount)
        * (1.0 + 0.65 * rain);
    sample.height_m = height_m;
    sample.darkness = density * (0.05 + 0.65 * rain);
    sample.relief = height_m / 320.0;
    sample.gradient = height_gradient;
    sample.detail_amount = detail_amount;
    sample.detail_gate = detail_gate;
    sample.mask_amount = mask_amount;
    sample.mask_gate = mask_gate;
    return sample;
}

fn procedural_upper_cloud_sample(position_m: vec2<f32>) -> UpperCloudSample {
    let upper_coverage = clamp(
        max(
            scene.weather_structure.z,
            0.30 * scene.weather.z + 0.70 * scene.weather.w,
        ),
        0.0,
        1.0,
    );
    if upper_coverage <= 0.0 {
        var empty: UpperCloudSample;
        empty.optical_depth = 0.0;
        return empty;
    }
    let upper_noise = visual_cloud_noise_sample(
        position_m - scene.cloud_offsets_m.zw,
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        512.0,
        4u,
        1400.0,
    );
    let upper_threshold = mix(0.62, 0.12, upper_coverage) - 0.18 * scene.weather.w;
    let body = smoothstep(
        upper_threshold,
        upper_threshold + 0.10,
        upper_noise.value,
    );
    var sample: UpperCloudSample;
    sample.optical_depth = body * upper_coverage * (0.40 + 0.80 * scene.weather.w);
    return sample;
}

fn sun_direction() -> vec3<f32> {
    return normalize(scene.sun_direction_and_intensity.xyz);
}

fn direct_sun_amount() -> f32 {
    return max(scene.sun_direction_and_intensity.w, 0.0)
        * smoothstep(0.0, 0.08, sun_direction().z);
}

fn projected_cloud_position(
    position_m: vec2<f32>,
    source_height_m: f32,
    cloud_height_m: f32,
) -> vec2<f32> {
    let sun = sun_direction();
    return position_m
        + (cloud_height_m - source_height_m) * sun.xy / max(sun.z, 0.001);
}

fn terrain_smoothstep_sample(
    lower: f32,
    upper: f32,
    input: TerrainNoiseSample,
) -> TerrainNoiseSample {
    let normalized = (input.value - lower) / (upper - lower);
    let amount = clamp(normalized, 0.0, 1.0);
    var sample: TerrainNoiseSample;
    sample.value = amount * amount * (3.0 - 2.0 * amount);
    sample.gradient = vec2<f32>(0.0);
    if normalized > 0.0 && normalized < 1.0 {
        sample.gradient = input.gradient * 6.0 * amount * (1.0 - amount) / (upper - lower);
    }
    return sample;
}

fn procedural_terrain_sample(position_m: vec2<f32>) -> TerrainSample {
    let terrain_position_m = position_m / tuning.terrain_tuning.x;
    let cluster = terrain_fractal_sample(
        vec2<u32>(0x78bd642fu, 0xa0761d64u),
        terrain_position_m,
        8192.0,
        3u,
    );
    let island = terrain_fractal_sample(
        vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        terrain_position_m,
        1024.0,
        5u,
    );
    var unscaled: TerrainNoiseSample;
    unscaled.value = 18.0 * (0.68 * cluster.value + 0.32 * island.value - 0.35);
    unscaled.gradient = 18.0 * (0.68 * cluster.gradient + 0.32 * island.gradient);
    let island_ruggedness = terrain_smoothstep_sample(0.32, 0.58, cluster);
    var negative_cluster: TerrainNoiseSample;
    negative_cluster.value = -cluster.value;
    negative_cluster.gradient = -cluster.gradient;
    let trench_depth = terrain_smoothstep_sample(0.50, 0.74, negative_cluster);
    let elevation_scale = tuning.terrain_tuning.y
        * (1.0 + 19.0 * island_ruggedness.value + 29.0 * trench_depth.value);
    var base: TerrainNoiseSample;
    base.value = unscaled.value * elevation_scale;
    base.gradient = unscaled.gradient * elevation_scale
        + unscaled.value
            * tuning.terrain_tuning.y
            * (19.0 * island_ruggedness.gradient + 29.0 * trench_depth.gradient);

    var terrain: TerrainSample;
    terrain.elevation_m = base.value;
    terrain.gradient = base.gradient;
    let seabed_deep_gate = terrain_smoothstep_sample(-4.0, -2.5, base);
    let seabed_shore_rise = terrain_smoothstep_sample(-0.5, -0.1, base);
    let seabed_shore_gate_value = 1.0 - seabed_shore_rise.value;
    let seabed_mask_value = seabed_deep_gate.value * seabed_shore_gate_value;
    let rock_deep_gate = terrain_smoothstep_sample(-2.0, -0.8, base);
    let rock_shore_rise = terrain_smoothstep_sample(-0.4, -0.1, base);
    let rock_shore_gate_value = 1.0 - rock_shore_rise.value;
    let rock_mask_value = rock_deep_gate.value * rock_shore_gate_value;
    if seabed_mask_value == 0.0 && rock_mask_value == 0.0 {
        terrain.gradient /= tuning.terrain_tuning.x;
        return terrain;
    }
    let seabed_mask_gradient = seabed_deep_gate.gradient * seabed_shore_gate_value
        - seabed_shore_rise.gradient * seabed_deep_gate.value;
    let rock_mask_gradient = rock_deep_gate.gradient * rock_shore_gate_value
        - rock_shore_rise.gradient * rock_deep_gate.value;
    let reef = terrain_fractal_sample(
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        terrain_position_m,
        32.0,
        4u,
    );
    let reef_peak = terrain_smoothstep_sample(0.65, 0.85, reef);
    terrain.elevation_m += 0.6 * seabed_mask_value * reef.value
        * landscape_visibility.x
        + 2.0 * rock_mask_value * reef_peak.value * landscape_visibility.x;
    terrain.gradient += landscape_visibility.x * 0.6 * (
        seabed_mask_gradient * reef.value + reef.gradient * seabed_mask_value
    ) + landscape_visibility.x * 2.0 * (
        rock_mask_gradient * reef_peak.value + reef_peak.gradient * rock_mask_value
    );
    terrain.gradient /= tuning.terrain_tuning.x;
    return terrain;
}

fn terrain_rock_color(
    position_m: vec2<f32>,
    terrain_gradient: vec2<f32>,
    elevation_m: f32,
) -> vec3<f32> {
    let rock_texture = terrain_fractal_sample(
        vec2<u32>(0x1f83d9abu, 0x5be0cd19u),
        position_m,
        48.0,
        4u,
    );
    let fine_texture = terrain_fractal_sample(
        vec2<u32>(0xcbbb9d5du, 0x629a292au),
        position_m,
        12.0,
        3u,
    );
    let light_direction = sun_direction();
    let terrain_normal = normalize(vec3<f32>(-terrain_gradient, 1.0));
    let terrain_light = smoothstep(-0.15, 0.88, dot(terrain_normal, light_direction));
    let texture_normal = normalize(vec3<f32>(
        -terrain_gradient - 1.8 * rock_texture.gradient - 0.35 * fine_texture.gradient,
        1.0,
    ));
    let texture_light = clamp(dot(texture_normal, light_direction), 0.0, 1.0);
    let texture_grade = clamp(
        0.48 + landscape_visibility.z * (0.16 * rock_texture.value + 0.07 * fine_texture.value),
        0.0,
        1.0,
    );
    let rock_color = mix(
        vec3<f32>(0.25, 0.23, 0.20),
        vec3<f32>(0.49, 0.46, 0.40),
        0.18 * smoothstep(20.0, 240.0, elevation_m) + 0.82 * texture_grade,
    );
    let lighting_amount = landscape_visibility.w
        * clamp(direct_sun_amount(), 0.0, 1.0);
    return rock_color
        * mix(1.0, mix(0.48, 1.20, terrain_light), lighting_amount)
        * mix(1.0, mix(0.94, 1.06, texture_light), lighting_amount);
}

fn terrain_color(
    position_m: vec2<f32>,
    elevation_m: f32,
    terrain_gradient: vec2<f32>,
) -> vec3<f32> {
    let slope = length(terrain_gradient);
    if elevation_m < 0.0 {
        let depth_m = -elevation_m;
        let shallow_color = vec3<f32>(0.12, 0.42, 0.5);
        let open_water_color = vec3<f32>(0.04, 0.19, 0.25);
        let deep_water_color = vec3<f32>(0.025, 0.11, 0.15);
        let water_color = mix(
            shallow_color,
            open_water_color,
            smoothstep(0.0, 1.0, depth_m),
        );
        return mix(water_color, deep_water_color, smoothstep(1.0, 4.0, depth_m));
    }

    let surface = terrain_fractal_sample(
        vec2<u32>(0x6a09e667u, 0xbb67ae85u),
        position_m,
        96.0,
        4u,
    ).value;
    let surface_grade = 0.5 + 0.5 * landscape_visibility.z * surface;
    let vegetation = terrain_fractal_sample(
        vec2<u32>(0x510e527fu, 0x9b05688cu),
        position_m,
        1536.0,
        4u,
    ).value;
    let terrain_normal = normalize(vec3<f32>(-terrain_gradient, 1.0));
    let light_direction = sun_direction();
    let hill_light = clamp(dot(terrain_normal, light_direction), 0.0, 1.0);
    let lighting_amount = landscape_visibility.w
        * clamp(direct_sun_amount(), 0.0, 1.0);
    let hill_grade = mix(1.0, mix(0.62, 1.18, hill_light), lighting_amount);
    let altitude_exposure = smoothstep(50.0, 200.0, elevation_m);
    let rock_slope_threshold = tuning.terrain_tuning.z * mix(3.0, 0.8, altitude_exposure);
    if slope >= rock_slope_threshold {
        return terrain_rock_color(position_m, terrain_gradient, elevation_m);
    }

    let snow_line_m = 360.0 + 45.0 * vegetation;
    if elevation_m >= snow_line_m {
        return mix(
            vec3<f32>(0.72, 0.76, 0.77),
            vec3<f32>(0.96, 0.97, 0.96),
            0.55 + 0.20 * surface_grade,
        ) * mix(1.0, mix(0.78, 1.08, hill_light), lighting_amount);
    }

    let alpine_rock_line_m = 110.0 + 45.0 * vegetation;
    if elevation_m >= alpine_rock_line_m {
        return terrain_rock_color(position_m, terrain_gradient, elevation_m);
    }

    let beach_geology = terrain_fractal_sample(
        vec2<u32>(0x3c6ef372u, 0xa54ff53au),
        position_m,
        768.0,
        2u,
    ).value;
    let beach_top_m = mix(0.08, 1.4, smoothstep(-0.30, 0.45, beach_geology));
    if elevation_m < beach_top_m {
        return mix(
            vec3<f32>(0.67, 0.56, 0.34),
            vec3<f32>(0.84, 0.76, 0.50),
            0.35 + 0.30 * surface_grade,
        ) * mix(1.0, mix(0.86, 1.06, hill_light), lighting_amount);
    }

    let tree_line = 1.0 - smoothstep(150.0, 250.0, elevation_m);
    let forest_amount = smoothstep(-0.18, 0.35, vegetation)
        * tree_line
        * (1.0 - smoothstep(0.04, 0.08, slope));
    let canopy_detail = terrain_fractal_sample(
        vec2<u32>(0x428a2f98u, 0x71374491u),
        position_m,
        56.0,
        3u,
    ).value;
    let canopy_cluster = smoothstep(-0.22, 0.38, canopy_detail + 0.32 * vegetation);
    let grass_color = mix(
        vec3<f32>(0.09, 0.24, 0.10),
        vec3<f32>(0.32, 0.38, 0.17),
        0.20 * surface_grade
            + 0.20 * (0.5 + 0.5 * vegetation)
            + 0.60 * smoothstep(8.0, 150.0, elevation_m),
    ) * hill_grade;
    let forest_color = mix(
        vec3<f32>(0.03, 0.12, 0.06),
        vec3<f32>(0.09, 0.27, 0.11),
        0.22 + 0.40 * canopy_cluster,
    ) * hill_grade;
    return mix(
        grass_color,
        forest_color,
        forest_amount * canopy_cluster * landscape_visibility.y,
    );
}

fn cloud_color(
    surface_color: vec3<f32>,
    cloud_sample: LowCloudSample,
    upper_transmittance: f32,
) -> vec3<f32> {
    let density = cloud_sample.density;
    let cloud_normal = normalize(vec3<f32>(
        -tuning.cloud_rendering.w * cloud_sample.gradient,
        1.0,
    ));
    let direct_light = max(dot(cloud_normal, sun_direction()), 0.0)
        * clamp(direct_sun_amount(), 0.0, 1.0);
    let sunlight = direct_sun_amount() * upper_transmittance;
    let relief = cloud_sample.relief / max(density, 0.001);
    let edge_light = sunlight
        * (1.0 - smoothstep(0.30, 0.78, density))
        * smoothstep(0.10, 0.55, density)
        * 0.18;
    let depth_shadow = smoothstep(0.45, 1.35, cloud_sample.optical_depth) * 0.14;
    let cloud_material = mix(
        vec3<f32>(0.68, 0.72, 0.74),
        vec3<f32>(0.96, 0.97, 0.96),
        0.30 + 0.45 * relief + 0.25 * direct_light,
    );
    let cloud = mix(
        cloud_material,
        vec3<f32>(0.12, 0.15, 0.17),
        clamp(cloud_sample.darkness * tuning.cloud_rendering.z, 0.0, 1.0),
    ) * (0.70 + sunlight * mix(0.12, 0.36, direct_light))
        + edge_light
        - depth_shadow;
    let body = smoothstep(0.08, 0.18, density);
    let optical_depth = tuning.cloud_rendering.x * cloud_sample.optical_depth;
    let opacity = tuning.cloud_rendering.y * body * (1.0 - exp(-1.35 * optical_depth));
    return mix(surface_color, cloud, opacity);
}

fn upper_cloud_transmittance_at_low_cloud(
    position_m: vec2<f32>,
    low_height_m: f32,
    upper_height_m: f32,
) -> f32 {
    if direct_sun_amount() <= 0.0 {
        return 1.0;
    }
    let upper = procedural_upper_cloud_sample(projected_cloud_position(
        position_m,
        low_height_m,
        upper_height_m,
    ));
    return mix(
        1.0,
        exp(-0.85 * tuning.cloud_rendering.x * upper.optical_depth),
        layer_visibility.w,
    );
}

fn cloud_shadow_color(
    surface_color: vec3<f32>,
    position_m: vec2<f32>,
    elevation_m: f32,
) -> vec3<f32> {
    let low_height_m = 650.0
        + 650.0 * scene.weather_structure.y
        + 550.0 * scene.weather.w;
    let upper_height_m = 6200.0;
    let surface_height_m = max(elevation_m, 0.0);
    let shadow_strength = direct_sun_amount();
    if shadow_strength <= 0.0 {
        return surface_color;
    }
    var shadow_depth = 0.0;
    if layer_visibility.y > 0.0 {
        let projected_low = procedural_low_cloud_sample(
            projected_cloud_position(position_m, surface_height_m, low_height_m),
        );
        shadow_depth += layer_visibility.y * projected_low.optical_depth;
    }
    if layer_visibility.z > 0.0 {
        let projected_upper = procedural_upper_cloud_sample(
            projected_cloud_position(position_m, surface_height_m, upper_height_m),
        );
        shadow_depth += layer_visibility.z * projected_upper.optical_depth;
    }
    let shadow_transmittance = exp(
        -0.85 * tuning.cloud_rendering.x * shadow_depth,
    );
    return surface_color * mix(
        1.0,
        shadow_transmittance,
        tuning.cloud_rendering.y * shadow_strength,
    );
}

fn final_scene_color(input: VertexOutput) -> vec4<f32> {
    var surface_color = input.color;
    if input.is_terrain > 0.5 {
        let terrain = procedural_terrain_sample(input.world_position_m);
        surface_color = terrain_color(
            input.world_position_m,
            terrain.elevation_m,
            terrain.gradient,
        );
        surface_color = cloud_shadow_color(
            surface_color,
            input.world_position_m,
            terrain.elevation_m,
        );
        if layer_visibility.x > 0.0 {
            let low_height_m = 650.0
                + 650.0 * scene.weather_structure.y
                + 550.0 * scene.weather.w;
            let upper_height_m = 6200.0;
            let low_cloud = procedural_low_cloud_sample(input.world_position_m);
            var upper_transmittance = 1.0;
            if low_cloud.density > 0.01 && layer_visibility.w > 0.0 {
                upper_transmittance = upper_cloud_transmittance_at_low_cloud(
                    input.world_position_m,
                    low_height_m + tuning.cloud_rendering.w * low_cloud.height_m,
                    upper_height_m,
                );
            }
            surface_color = mix(
                surface_color,
                cloud_color(surface_color, low_cloud, upper_transmittance),
                layer_visibility.x,
            );
        }
    }
    return vec4<f32>(surface_color, 1.0);
}

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

fn approximate_gust(position_m: vec2<f32>) -> vec2<f32> {
    let drift = scene.time_seconds * vec2<f32>(5.0, -3.0);
    let x = visual_cloud_noise_sample(
        position_m - drift,
        vec2<u32>(0x6a09e667u, 0xbb67ae85u),
        256.0,
        3u,
        90.0,
    ).value;
    let y = visual_cloud_noise_sample(
        position_m - drift,
        vec2<u32>(0x3c6ef372u, 0xa54ff53au),
        256.0,
        3u,
        90.0,
    ).value;
    return 3.0 * vec2<f32>(x, y);
}

fn approximate_wind(position_m: vec2<f32>) -> vec2<f32> {
    let synoptic = scene.wind_vectors_m_s.zw;
    let regional = scene.wind_vectors_m_s.xy - synoptic / 1.18;
    return playground.wind_model.x * synoptic
        + playground.wind_model.y * regional
        + playground.wind_model.z * approximate_gust(position_m);
}

fn approximate_shelter(position_m: vec2<f32>, wind_m_s: vec2<f32>) -> f32 {
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
    return 1.0 - 0.70 * clamp(playground.wind_model.w, 0.0, 1.0) * obstruction;
}

fn wind_debug_color(position_m: vec2<f32>, wind_m_s: vec2<f32>) -> vec3<f32> {
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
    let stripe = 0.78 + 0.22 * smoothstep(
        0.60,
        1.0,
        cos(dot(position_m, direction) * 0.08 - scene.time_seconds * 2.0),
    );
    return direction_color * mix(0.35, 1.0, smoothstep(0.0, 20.0, speed)) * stripe;
}

fn playground_debug_color(input: VertexOutput, mode: u32) -> vec4<f32> {
    let position_m = input.world_position_m;
    if mode >= 1u && mode <= 8u {
        let low = procedural_low_cloud_sample(position_m);
        if mode == 1u { return vec4<f32>(scalar_debug_color(low.detail_amount), 1.0); }
        if mode == 2u { return vec4<f32>(scalar_debug_color(low.detail_gate), 1.0); }
        if mode == 3u { return vec4<f32>(scalar_debug_color(low.mask_amount), 1.0); }
        if mode == 4u { return vec4<f32>(scalar_debug_color(low.mask_gate), 1.0); }
        if mode == 5u { return vec4<f32>(scalar_debug_color(low.density), 1.0); }
        if mode == 6u { return vec4<f32>(scalar_debug_color(low.optical_depth), 1.0); }
        if mode == 7u {
            let normal = normalize(vec3<f32>(-low.gradient, 1.0));
            return vec4<f32>(0.5 + 0.5 * normal, 1.0);
        }
        let upper = procedural_upper_cloud_sample(position_m);
        return vec4<f32>(scalar_debug_color(upper.optical_depth), 1.0);
    }
    if mode == 12u {
        let low = procedural_low_cloud_sample(position_m);
        return vec4<f32>(scalar_debug_color(low.height_m / 320.0), 1.0);
    }
    if mode >= 9u && mode <= 11u {
        let terrain = procedural_terrain_sample(position_m);
        let surface_height_m = max(terrain.elevation_m, 0.0);
        let low_height_m = 650.0
            + 650.0 * scene.weather_structure.y
            + 550.0 * scene.weather.w;
        let low = procedural_low_cloud_sample(projected_cloud_position(
            position_m, surface_height_m, low_height_m,
        ));
        let upper = procedural_upper_cloud_sample(projected_cloud_position(
            position_m, surface_height_m, 6200.0,
        ));
        if mode == 9u {
            return vec4<f32>(scalar_debug_color(low.optical_depth), 1.0);
        }
        if mode == 10u {
            return vec4<f32>(scalar_debug_color(upper.optical_depth), 1.0);
        }
        let transmittance = exp(
            -0.85 * tuning.cloud_rendering.x * (low.optical_depth + upper.optical_depth),
        );
        return vec4<f32>(scalar_debug_color(transmittance), 1.0);
    }
    if mode >= 16u && mode <= 21u {
        let surface = scene.wind_vectors_m_s.xy;
        let synoptic = scene.wind_vectors_m_s.zw;
        let regional = surface - synoptic / 1.18;
        let gust = approximate_gust(position_m);
        let combined = approximate_wind(position_m);
        if mode == 16u { return vec4<f32>(wind_debug_color(position_m, surface), 1.0); }
        if mode == 17u { return vec4<f32>(wind_debug_color(position_m, synoptic), 1.0); }
        if mode == 18u { return vec4<f32>(wind_debug_color(position_m, regional), 1.0); }
        if mode == 19u { return vec4<f32>(wind_debug_color(position_m, gust), 1.0); }
        if mode == 20u { return vec4<f32>(wind_debug_color(position_m, combined), 1.0); }
        let shelter = approximate_shelter(position_m, combined);
        return vec4<f32>(mix(
            vec3<f32>(0.84, 0.17, 0.10),
            vec3<f32>(0.10, 0.72, 0.65),
            shelter,
        ), 1.0);
    }
    let terrain_position_m = position_m / tuning.terrain_tuning.x;
    let terrain = procedural_terrain_sample(position_m);
    if mode == 32u {
        return vec4<f32>(signed_debug_color(terrain.elevation_m / 12.0), 1.0);
    }
    if mode == 33u {
        return vec4<f32>(scalar_debug_color(length(terrain.gradient) / 0.25), 1.0);
    }
    if mode == 34u {
        let normal = normalize(vec3<f32>(-terrain.gradient, 1.0));
        return vec4<f32>(0.5 + 0.5 * normal, 1.0);
    }
    if mode == 35u {
        let cluster = terrain_fractal_sample(
            vec2<u32>(0x78bd642fu, 0xa0761d64u), terrain_position_m, 8192.0, 3u,
        ).value;
        return vec4<f32>(signed_debug_color(cluster), 1.0);
    }
    if mode == 36u {
        let island = terrain_fractal_sample(
            vec2<u32>(0xa0b428dbu, 0xe7037ed1u), terrain_position_m, 1024.0, 5u,
        ).value;
        return vec4<f32>(signed_debug_color(island), 1.0);
    }
    if mode == 37u {
        let reef = terrain_fractal_sample(
            vec2<u32>(0x75374cc3u, 0x589965ccu), terrain_position_m, 32.0, 4u,
        ).value;
        return vec4<f32>(signed_debug_color(reef), 1.0);
    }
    if mode == 38u {
        let vegetation = terrain_fractal_sample(
            vec2<u32>(0x510e527fu, 0x9b05688cu), position_m, 1536.0, 4u,
        ).value;
        return vec4<f32>(signed_debug_color(vegetation), 1.0);
    }
    if mode == 39u {
        let geology = terrain_fractal_sample(
            vec2<u32>(0x3c6ef372u, 0xa54ff53au), position_m, 768.0, 2u,
        ).value;
        return vec4<f32>(signed_debug_color(geology), 1.0);
    }
    return final_scene_color(input);
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    tuning = production_tuning();
    layer_visibility = vec4<f32>(1.0);
    landscape_visibility = vec4<f32>(1.0);
    return final_scene_color(input);
}

@fragment
fn fragment_playground(input: VertexOutput) -> @location(0) vec4<f32> {
    tuning = playground_scene_tuning();
    layer_visibility = playground.layer_visibility;
    landscape_visibility = playground.landscape_visibility;
    if playground.diagnostics.x == 0u {
        return final_scene_color(input);
    }
    return playground_debug_color(input, playground.diagnostics.x);
}
