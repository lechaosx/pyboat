struct SceneUniforms {
    camera_and_view_m: vec4<f32>,
    weather: vec4<f32>,
    weather_structure: vec4<f32>,
    cloud_motion: vec4<f32>,
    match_seed: vec2<u32>,
    cloud_visual_seed: vec2<u32>,
    cloud_offsets_m: vec4<f32>,
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

fn cloud_coordinates(position_m: vec2<f32>, high_altitude: bool) -> vec2<f32> {
    if high_altitude {
        return position_m - scene.cloud_offsets_m.zw;
    }
    return position_m - scene.cloud_offsets_m.xy;
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

fn directional_cloud_noise(
    position_m: vec2<f32>,
    high_altitude: bool,
    field_salt: vec2<u32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    initial_lifetime_seconds: f32,
    filter_half_span_m: f32,
) -> f32 {
    let center_m = cloud_coordinates(position_m, high_altitude);
    let layer_wind = select(scene.cloud_motion.xy, scene.cloud_motion.zw, high_altitude);
    let wind_speed = length(layer_wind);
    if wind_speed < 0.5 {
        return visual_cloud_noise(
            center_m,
            field_salt,
            initial_wavelength_m,
            octave_count,
            initial_lifetime_seconds,
        );
    }
    let wind_direction = layer_wind / wind_speed;
    var filtered = 0.0;
    var weight_sum = 0.0;
    for (var sample_index = 0u; sample_index < 9u; sample_index++) {
        let along_wind = f32(sample_index) / 8.0 * 2.0 - 1.0;
        let weight = 1.0 - 0.55 * abs(along_wind);
        filtered += weight * visual_cloud_noise(
            center_m + filter_half_span_m * along_wind * wind_direction,
            field_salt,
            initial_wavelength_m,
            octave_count,
            initial_lifetime_seconds,
        );
        weight_sum += weight;
    }
    return filtered / weight_sum;
}

struct CloudSample {
    density: f32,
    optical_depth: f32,
    darkness: f32,
    relief: f32,
};

fn procedural_cloud_sample(position_m: vec2<f32>) -> CloudSample {
    let cover = scene.weather.z;
    let moisture = scene.weather_structure.x;
    let instability = scene.weather_structure.y;
    let high_cloudiness = scene.weather_structure.z;
    let rain = scene.weather.w;

    let cumulus_body_noise = visual_cloud_noise(
        cloud_coordinates(position_m, false),
        vec2<u32>(0x38d01377u, 0x452821e6u),
        96.0,
        7u,
        150.0,
    );
    let cumulus_billow_noise = visual_cloud_noise(
        cloud_coordinates(position_m, false),
        vec2<u32>(0x16d2a771u, 0x9e3779b9u),
        28.0,
        6u,
        75.0,
    );
    let cumulus_noise = cumulus_body_noise + 0.22 * cumulus_billow_noise;
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

    let stratus_noise = directional_cloud_noise(
        position_m,
        false,
        vec2<u32>(0x34e90c6cu, 0xbe5466cfu),
        240.0,
        6u,
        420.0,
        720.0,
    );
    let stratus_breakup = visual_cloud_noise(
        cloud_coordinates(position_m, false),
        vec2<u32>(0xa24baed4u, 0x94d049bbu),
        96.0,
        5u,
        210.0,
    );
    let stratus_ridge = 1.0 - smoothstep(0.010, 0.042, abs(stratus_noise));
    let stratus_breakup_amount = smoothstep(-0.16, 0.38, stratus_breakup);
    let stratus = cover
        * moisture
        * smoothstep(0.18, 0.46, stratus_ridge)
        * (0.16 + 0.84 * stratus_breakup_amount);

    let storm_suppression = 1.0 - 0.88 * rain * smoothstep(0.45, 0.90, instability);
    let low_cloud = storm_suppression
        * mix(stratus, cumulus, smoothstep(0.22, 0.72, instability));

    let cirrus_noise = directional_cloud_noise(
        position_m,
        true,
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        320.0,
        5u,
        360.0,
        640.0,
    );
    let cirrus_envelope = visual_cloud_noise(
        cloud_coordinates(position_m, true),
        vec2<u32>(0x4cf5ad43u, 0xbf58476du),
        640.0,
        4u,
        720.0,
    );
    let cirrus_feather = visual_cloud_noise(
        cloud_coordinates(position_m, true),
        vec2<u32>(0x9e3779b9u, 0x7f4a7c15u),
        72.0,
        5u,
        180.0,
    );
    let cirrus_filament = smoothstep(0.045, 0.075, cirrus_noise)
        - smoothstep(0.13, 0.17, cirrus_noise);
    let cirrus_envelope_amount = smoothstep(
        0.04,
        0.34,
        cirrus_envelope + 0.10 * cirrus_feather,
    );
    let cirrus = 0.58
        * high_cloudiness
        * cirrus_filament
        * (0.38 + 0.62 * cirrus_envelope_amount);

    let squall_noise = directional_cloud_noise(
        position_m,
        false,
        vec2<u32>(0x632be59bu, 0x8cb92ba7u),
        420.0,
        5u,
        300.0,
        720.0,
    );
    let squall_band = smoothstep(0.075, 0.125, squall_noise);
    let squall_front = rain
        * smoothstep(0.30, 0.62, instability)
        * (1.0 - smoothstep(0.78, 0.96, instability))
        * squall_band
        * mix(0.65, 1.0, smoothstep(-0.30, 0.20, cumulus_body_noise));
    let squall_mass = sqrt(squall_front);

    let storm_macro = visual_cloud_noise(
        cloud_coordinates(position_m, false),
        vec2<u32>(0x1b56c4e9u, 0xd1b54a32u),
        180.0,
        5u,
        240.0,
    );
    let storm_potential = storm_macro + 0.18 * cumulus_billow_noise;
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
    sample.optical_depth = clamp(
        max(
            max(
                low_optical_depth,
                cirrus * (0.18 + 0.20 * (0.5 + 0.5 * cirrus_feather)),
            ),
            max(1.12 * squall_mass, 1.18 * storm_mass * storm_depth_variation),
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
        * (0.35 * cumulus_billows + 0.65 * (0.5 + 0.5 * cumulus_billow_noise));
    let storm_relief = storm_mass
        * (0.48 + 0.52 * (0.5 + 0.5 * cumulus_billow_noise));
    sample.relief = clamp(
        max(cellular_relief, storm_relief)
            + 0.32 * stratus * (0.5 + 0.5 * stratus_breakup)
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
    let cluster = terrain_fractal_sample(
        vec2<u32>(0x78bd642fu, 0xa0761d64u),
        position_m,
        8192.0,
        3u,
    );
    let island = terrain_fractal_sample(
        vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        position_m,
        1024.0,
        5u,
    );
    let geology = terrain_fractal_sample(
        vec2<u32>(0x299f31d0u, 0xa4093822u),
        position_m,
        8192.0,
        3u,
    );
    let geology_amount = terrain_smoothstep_sample(0.31, 0.56, geology);
    var unscaled: TerrainNoiseSample;
    unscaled.value = 18.0 * (0.68 * cluster.value + 0.32 * island.value - 0.35);
    unscaled.gradient = 18.0 * (0.68 * cluster.gradient + 0.32 * island.gradient);
    let elevation_scale = 1.0 + 49.0 * geology_amount.value;
    var base: TerrainNoiseSample;
    base.value = unscaled.value * elevation_scale;
    base.gradient = unscaled.gradient * elevation_scale
        + unscaled.value * 49.0 * geology_amount.gradient;

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
        return terrain;
    }
    let seabed_mask_gradient = seabed_deep_gate.gradient * seabed_shore_gate_value
        - seabed_shore_rise.gradient * seabed_deep_gate.value;
    let rock_mask_gradient = rock_deep_gate.gradient * rock_shore_gate_value
        - rock_shore_rise.gradient * rock_deep_gate.value;
    let reef = terrain_fractal_sample(
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        position_m,
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
    return terrain;
}

fn terrain_color(elevation_m: f32, slope: f32) -> vec3<f32> {
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
    if slope >= 0.08 {
        return vec3<f32>(0.35, 0.31, 0.25);
    }
    if elevation_m < 0.4 {
        return vec3<f32>(0.76, 0.67, 0.43);
    }
    return vec3<f32>(0.15, 0.28, 0.17);
}

fn cloud_color(surface_color: vec3<f32>, cloud_sample: CloudSample) -> vec3<f32> {
    let density = cloud_sample.density;
    let edge_light = (1.0 - smoothstep(0.35, 0.80, cloud_sample.darkness))
        * clamp((dpdx(density) - dpdy(density)) * 0.65, -0.10, 0.10);
    let top_light = (cloud_sample.relief - 0.38) * 0.20;
    let depth_shadow = smoothstep(0.55, 1.45, cloud_sample.optical_depth) * 0.10;
    let cloud = mix(
        vec3<f32>(0.86, 0.89, 0.90),
        vec3<f32>(0.12, 0.15, 0.17),
        cloud_sample.darkness,
    ) + edge_light + top_light - depth_shadow;
    let body = smoothstep(0.08, 0.18, density);
    let opacity = body * (1.0 - exp(-1.35 * cloud_sample.optical_depth));
    return mix(surface_color, cloud, opacity);
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    var surface_color = input.color;
    if input.is_terrain > 0.5 {
        let terrain = procedural_terrain_sample(input.world_position_m);
        surface_color = terrain_color(terrain.elevation_m, length(terrain.gradient));
        let cloud_sample = procedural_cloud_sample(input.world_position_m);
        surface_color = cloud_color(surface_color, cloud_sample);
    }
    return vec4<f32>(surface_color, 1.0);
}
