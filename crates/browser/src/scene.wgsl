struct SceneUniforms {
    camera_and_view_m: vec4<f32>,
    match_seed: vec2<u32>,
    padding: vec2<u32>,
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

fn procedural_terrain_sample(position_m: vec2<f32>) -> TerrainSample {
    var cluster_noise = 0.0;
    var cluster_gradient = vec2<f32>(0.0);
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = 8192.0;
    for (var octave = 0u; octave < 3u; octave++) {
        let octave_seed = scene.match_seed
            ^ vec2<u32>(0x78bd642fu, 0xa0761d64u)
            ^ multiply_u64(
                vec2<u32>(octave, 0u),
                vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            );
        let octave_sample = terrain_value_noise(octave_seed, position_m, wavelength_m);
        cluster_noise += amplitude * octave_sample.value;
        cluster_gradient += amplitude * octave_sample.gradient;
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.5;
    }
    cluster_noise /= amplitude_sum;
    cluster_gradient /= amplitude_sum;

    var island_noise = 0.0;
    var island_gradient = vec2<f32>(0.0);
    amplitude = 1.0;
    amplitude_sum = 0.0;
    wavelength_m = 1024.0;
    for (var octave = 0u; octave < 5u; octave++) {
        let octave_seed = scene.match_seed
            ^ vec2<u32>(0xa0b428dbu, 0xe7037ed1u)
            ^ multiply_u64(
                vec2<u32>(octave, 0u),
                vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            );
        let octave_sample = terrain_value_noise(octave_seed, position_m, wavelength_m);
        island_noise += amplitude * octave_sample.value;
        island_gradient += amplitude * octave_sample.gradient;
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.5;
    }
    island_noise /= amplitude_sum;
    island_gradient /= amplitude_sum;

    var terrain: TerrainSample;
    terrain.elevation_m = 18.0 * (0.68 * cluster_noise + 0.32 * island_noise - 0.35);
    terrain.gradient = 18.0 * (0.68 * cluster_gradient + 0.32 * island_gradient);
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
    if elevation_m < 0.4 {
        return vec3<f32>(0.76, 0.67, 0.43);
    }
    if slope >= 0.01 {
        return vec3<f32>(0.35, 0.31, 0.25);
    }
    return vec3<f32>(0.15, 0.28, 0.17);
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    if input.is_terrain > 0.5 {
        let terrain = procedural_terrain_sample(input.world_position_m);
        return vec4<f32>(terrain_color(terrain.elevation_m, length(terrain.gradient)), 1.0);
    }
    return vec4<f32>(input.color, 1.0);
}
