struct SceneUniforms {
    camera_and_view_m: vec4<f32>,
    weather: vec4<f32>,
    weather_structure: vec4<f32>,
    cloud_motion: vec4<f32>,
    match_seed: vec2<u32>,
    cloud_visual_seed: vec2<u32>,
    cloud_offsets_m: vec4<f32>,
    cloud_tuning: vec4<f32>,
    cloud_rendering: vec4<f32>,
    cloud_behavior: vec4<f32>,
    terrain_tuning: vec4<f32>,
    time_seconds: f32,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;

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

fn visual_cloud_noise(
    position_m: vec2<f32>,
    field_salt: vec2<u32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    initial_lifetime_seconds: f32,
) -> f32 {
    var noise = 0.0;
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
        let first = terrain_value_noise(first_seed, position_m, wavelength_m).value;
        let second = terrain_value_noise(second_seed, position_m, wavelength_m).value;
        noise += amplitude * mix(first, second, time_blend);
        amplitude_sum += amplitude;
        amplitude *= 0.58;
        wavelength_m *= 0.5;
    }
    return noise / amplitude_sum;
}

fn cloud_base_coordinates(
    position_m: vec2<f32>,
    high_altitude: bool,
) -> vec2<f32> {
    let offset_m = select(scene.cloud_offsets_m.xy, scene.cloud_offsets_m.zw, high_altitude);
    return position_m - offset_m * (1.0 - clamp(scene.cloud_behavior.x, 0.0, 1.0));
}

fn cloud_mask_coordinates(
    position_m: vec2<f32>,
    high_altitude: bool,
    elongation: f32,
) -> vec2<f32> {
    let offset_m = select(scene.cloud_offsets_m.xy, scene.cloud_offsets_m.zw, high_altitude);
    let translated_m = position_m - offset_m * clamp(scene.cloud_behavior.x, 0.0, 1.0);
    let wind_angle = select(scene.cloud_behavior.y, scene.cloud_behavior.z, high_altitude);
    let along_wind = vec2<f32>(cos(wind_angle), sin(wind_angle));
    let across_wind = vec2<f32>(-along_wind.y, along_wind.x);
    return vec2<f32>(
        dot(translated_m, along_wind) / max(elongation, 1.0),
        dot(translated_m, across_wind),
    );
}

struct CloudSample {
    density: f32,
    optical_depth: f32,
    darkness: f32,
    relief: f32,
    low_shadow_depth: f32,
    high_shadow_depth: f32,
};

fn procedural_cloud_sample(position_m: vec2<f32>) -> CloudSample {
    let cover = scene.weather.z;
    let moisture = scene.weather_structure.x;
    let instability = scene.weather_structure.y;
    let high_cloudiness = scene.weather_structure.z;
    let rain = scene.weather.w;

    let low_base_position_m = cloud_base_coordinates(position_m, false);
    let high_base_position_m = cloud_base_coordinates(position_m, true);

    let cumulus_body_noise = visual_cloud_noise(
        low_base_position_m,
        vec2<u32>(0x38d01377u, 0x452821e6u),
        96.0 * scene.cloud_tuning.x,
        6u,
        150.0,
    );
    let cumulus_billow_noise = visual_cloud_noise(
        low_base_position_m,
        vec2<u32>(0x16d2a771u, 0x9e3779b9u),
        28.0 * scene.cloud_tuning.x,
        5u,
        75.0,
    );
    let cumulus_mask_noise = visual_cloud_noise(
        cloud_mask_coordinates(position_m, false, 1.5),
        vec2<u32>(0xa24baed4u, 0x94d049bbu),
        384.0 * scene.cloud_tuning.x,
        3u,
        480.0,
    );
    let cumulus_noise = cumulus_body_noise
        + 0.22 * scene.cloud_tuning.y * cumulus_billow_noise
        + 0.22 * cumulus_mask_noise;
    let cumulus_threshold = mix(0.42, -0.08, cover);
    let cumulus_body = smoothstep(
        cumulus_threshold,
        cumulus_threshold + 0.055,
        cumulus_noise,
    );
    let cumulus_billows = smoothstep(
        cumulus_threshold + 0.02,
        cumulus_threshold + 0.24,
        cumulus_noise + 0.18 * abs(cumulus_billow_noise),
    );
    let cumulus = cover * cumulus_body * (0.52 + 0.48 * cumulus_billows);

    let stratus_base_noise = visual_cloud_noise(
        low_base_position_m,
        vec2<u32>(0x34e90c6cu, 0xbe5466cfu),
        112.0 * scene.cloud_tuning.x,
        5u,
        210.0,
    );
    let stratus_mask_noise = visual_cloud_noise(
        cloud_mask_coordinates(position_m, false, scene.cloud_tuning.z),
        vec2<u32>(0x34e90c6cu, 0xbe5466cfu),
        512.0 * scene.cloud_tuning.x,
        3u,
        600.0,
    );
    let stratus_ridge = 1.0 - abs(stratus_mask_noise);
    let stratus_threshold = mix(0.90, 0.72, cover * moisture);
    let stratus_envelope = smoothstep(stratus_threshold, stratus_threshold + 0.10, stratus_ridge);
    let stratus_detail = smoothstep(-0.34, 0.28, stratus_base_noise);
    let stratus = cover
        * moisture
        * stratus_envelope
        * (0.42 + 0.58 * stratus_detail);

    let storm_suppression = 1.0 - 0.88 * rain * smoothstep(0.45, 0.90, instability);
    let low_cloud = storm_suppression
        * mix(stratus, cumulus, smoothstep(0.22, 0.72, instability));

    let cirrus_feather = visual_cloud_noise(
        high_base_position_m,
        vec2<u32>(0x9e3779b9u, 0x7f4a7c15u),
        72.0 * scene.cloud_tuning.x,
        5u,
        180.0,
    );
    let cirrus_mask_noise = visual_cloud_noise(
        cloud_mask_coordinates(position_m, true, scene.cloud_tuning.w),
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        640.0 * scene.cloud_tuning.x,
        3u,
        720.0,
    );
    let cirrus_envelope_amount = smoothstep(-0.05, 0.42, cirrus_mask_noise);
    let cirrus_ridge = 1.0 - abs(cirrus_mask_noise);
    let cirrus_filament = smoothstep(0.94, 0.985, cirrus_ridge)
        * (0.48 + 0.52 * smoothstep(-0.35, 0.30, cirrus_feather));
    let cirrus = 0.58
        * high_cloudiness
        * cirrus_filament
        * (0.55 + 0.45 * cirrus_envelope_amount);

    let squall_base_noise = visual_cloud_noise(
        low_base_position_m,
        vec2<u32>(0x8cb92ba7u, 0x632be59bu),
        120.0 * scene.cloud_tuning.x,
        5u,
        180.0,
    );
    let squall_mask_noise = visual_cloud_noise(
        cloud_mask_coordinates(position_m, false, max(4.0, 1.6 * scene.cloud_tuning.z)),
        vec2<u32>(0x632be59bu, 0x8cb92ba7u),
        420.0 * scene.cloud_tuning.x,
        3u,
        540.0,
    );
    let squall_ridge = 1.0 - abs(squall_mask_noise);
    let squall_band = smoothstep(0.55, 0.85, squall_ridge);
    let squall_front = rain
        * smoothstep(0.30, 0.62, instability)
        * (1.0 - smoothstep(0.78, 0.96, instability))
        * squall_band
        * smoothstep(-0.25, 0.25, squall_base_noise);
    let squall_mass = squall_front;

    let storm_macro = visual_cloud_noise(
        low_base_position_m,
        vec2<u32>(0x1b56c4e9u, 0xd1b54a32u),
        180.0 * scene.cloud_tuning.x,
        5u,
        240.0,
    );
    let storm_mask_noise = visual_cloud_noise(
        cloud_mask_coordinates(position_m, false, 2.5),
        vec2<u32>(0x4cf5ad43u, 0xbf58476du),
        480.0 * scene.cloud_tuning.x,
        3u,
        660.0,
    );
    let storm_potential = storm_macro
        + 0.18 * cumulus_billow_noise
        + 0.24 * storm_mask_noise;
    let storm_anvil = rain
        * smoothstep(0.72, 0.94, instability)
        * smoothstep(-0.10, 0.34, storm_potential);
    let storm_mass = sqrt(storm_anvil);
    let storm_depth_variation = mix(
        0.72,
        1.0,
        smoothstep(-0.08, 0.48, storm_potential),
    );

    var sample: CloudSample;
    sample.density = max(max(low_cloud, cirrus), max(squall_mass, storm_mass));
    let low_optical_depth = mix(
        0.42 * stratus,
        cumulus * (0.18 + 0.55 * cumulus_billows),
        smoothstep(0.22, 0.72, instability),
    );
    sample.low_shadow_depth = max(
        low_optical_depth,
        max(1.12 * squall_mass, 1.18 * storm_mass * storm_depth_variation),
    );
    sample.high_shadow_depth = cirrus
        * (0.14
            + 0.16 * cirrus_envelope_amount
            + 0.14 * (0.5 + 0.5 * cirrus_feather));
    sample.optical_depth = clamp(
        max(
            sample.low_shadow_depth,
            sample.high_shadow_depth,
        ),
        0.0,
        1.5,
    );
    sample.darkness = clamp(
        0.10 * low_cloud
            + 0.48 * smoothstep(0.55, 1.35, sample.optical_depth)
            + 0.48 * squall_mass
            + 0.48 * storm_mass
            + rain * 0.16 * sample.density,
        0.0,
        1.0,
    );
    let cellular_relief = cumulus_body
        * mix(0.25, 1.0, instability)
        * (0.35 * cumulus_billows
            + 0.65
                * (0.5 + 0.5 * scene.cloud_tuning.y * cumulus_billow_noise));
    let storm_relief = storm_mass
        * (0.48 + 0.52 * (0.5 + 0.5 * cumulus_billow_noise));
    sample.relief = clamp(
        max(cellular_relief, storm_relief)
            + 0.32 * stratus * (0.5 + 0.5 * stratus_base_noise)
            + 0.20 * cirrus * (0.5 + 0.5 * cirrus_feather),
        0.0,
        1.0,
    );
    return sample;
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
    let terrain_position_m = position_m / scene.terrain_tuning.x;
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
    let elevation_scale = scene.terrain_tuning.y
        * (1.0 + 19.0 * island_ruggedness.value + 29.0 * trench_depth.value);
    var base: TerrainNoiseSample;
    base.value = unscaled.value * elevation_scale;
    base.gradient = unscaled.gradient * elevation_scale
        + unscaled.value
            * scene.terrain_tuning.y
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
        terrain.gradient /= scene.terrain_tuning.x;
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
        + 2.0 * rock_mask_value * reef_peak.value;
    terrain.gradient += 0.6 * (
        seabed_mask_gradient * reef.value + reef.gradient * seabed_mask_value
    ) + 2.0 * (
        rock_mask_gradient * reef_peak.value + reef_peak.gradient * rock_mask_value
    );
    terrain.gradient /= scene.terrain_tuning.x;
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
    let light_direction = normalize(vec3<f32>(-0.55, -0.75, 1.0));
    let terrain_normal = normalize(vec3<f32>(-terrain_gradient, 1.0));
    let terrain_light = smoothstep(-0.15, 0.88, dot(terrain_normal, light_direction));
    let texture_normal = normalize(vec3<f32>(
        -terrain_gradient - 1.8 * rock_texture.gradient - 0.35 * fine_texture.gradient,
        1.0,
    ));
    let texture_light = clamp(dot(texture_normal, light_direction), 0.0, 1.0);
    let texture_grade = clamp(
        0.48 + 0.16 * rock_texture.value + 0.07 * fine_texture.value,
        0.0,
        1.0,
    );
    let rock_color = mix(
        vec3<f32>(0.25, 0.23, 0.20),
        vec3<f32>(0.49, 0.46, 0.40),
        0.18 * smoothstep(20.0, 240.0, elevation_m) + 0.82 * texture_grade,
    );
    return rock_color
        * mix(0.48, 1.20, terrain_light)
        * mix(0.94, 1.06, texture_light);
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
    let surface_grade = 0.5 + 0.5 * surface;
    let vegetation = terrain_fractal_sample(
        vec2<u32>(0x510e527fu, 0x9b05688cu),
        position_m,
        1536.0,
        4u,
    ).value;
    let terrain_normal = normalize(vec3<f32>(-terrain_gradient, 1.0));
    let light_direction = normalize(vec3<f32>(-0.55, -0.75, 1.0));
    let hill_light = clamp(dot(terrain_normal, light_direction), 0.0, 1.0);
    let hill_grade = mix(0.62, 1.18, hill_light);
    let altitude_exposure = smoothstep(50.0, 200.0, elevation_m);
    let rock_slope_threshold = scene.terrain_tuning.z * mix(3.0, 0.8, altitude_exposure);
    if slope >= rock_slope_threshold {
        return terrain_rock_color(position_m, terrain_gradient, elevation_m);
    }

    let snow_line_m = 360.0 + 45.0 * vegetation;
    if elevation_m >= snow_line_m {
        return mix(
            vec3<f32>(0.72, 0.76, 0.77),
            vec3<f32>(0.96, 0.97, 0.96),
            0.55 + 0.20 * surface_grade,
        ) * mix(0.78, 1.08, hill_light);
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
        ) * mix(0.86, 1.06, hill_light);
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
    return mix(grass_color, forest_color, forest_amount * canopy_cluster);
}

fn cloud_color(surface_color: vec3<f32>, cloud_sample: CloudSample) -> vec3<f32> {
    let density = cloud_sample.density;
    let edge_light = (1.0 - smoothstep(0.35, 0.80, cloud_sample.darkness))
        * clamp((dpdx(density) - dpdy(density)) * 0.65, -0.10, 0.10);
    let top_light = scene.cloud_rendering.w * (cloud_sample.relief - 0.38) * 0.20;
    let depth_shadow = smoothstep(0.55, 1.45, cloud_sample.optical_depth) * 0.10;
    let cloud = mix(
        vec3<f32>(0.86, 0.89, 0.90),
        vec3<f32>(0.12, 0.15, 0.17),
        clamp(cloud_sample.darkness * scene.cloud_rendering.z, 0.0, 1.0),
    ) + edge_light + top_light - depth_shadow;
    let body = smoothstep(0.08, 0.18, density);
    let optical_depth = scene.cloud_rendering.x * cloud_sample.optical_depth;
    let opacity = scene.cloud_rendering.y * body * (1.0 - exp(-1.35 * optical_depth));
    return mix(surface_color, cloud, opacity);
}

fn cloud_shadow_color(
    surface_color: vec3<f32>,
    position_m: vec2<f32>,
    elevation_m: f32,
) -> vec3<f32> {
    let low_presence = max(scene.weather.z, scene.weather.w);
    let high_presence = 0.58 * scene.weather_structure.z;
    let high_fraction = high_presence / max(low_presence + high_presence, 0.001);
    let low_height_m = 650.0
        + 650.0 * scene.weather_structure.y
        + 550.0 * scene.weather.w;
    let cloud_height_m = mix(low_height_m, 6200.0, high_fraction);
    let projection_height_m = max(cloud_height_m - max(elevation_m, 0.0), 50.0);
    let light_direction = normalize(vec3<f32>(-0.55, -0.75, 1.0));
    let projected_position_m = position_m
        + projection_height_m * light_direction.xy / light_direction.z;
    let projected_cloud = procedural_cloud_sample(projected_position_m);
    let shadow_depth = mix(
        projected_cloud.low_shadow_depth,
        projected_cloud.high_shadow_depth,
        high_fraction,
    );
    let shadow_strength = 0.34
        * scene.cloud_rendering.y
        * (1.0 - exp(-1.25 * scene.cloud_rendering.x * shadow_depth));
    return surface_color * mix(1.0, 0.68, shadow_strength);
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
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
        let cloud_sample = procedural_cloud_sample(input.world_position_m);
        surface_color = cloud_color(surface_color, cloud_sample);
    }
    return vec4<f32>(surface_color, 1.0);
}
