fn canonical_world_component(
    value_m: f32,
    period_m: f32,
    inverse_period_m: f32,
) -> f32 {
    let wrapped_m = value_m - floor(value_m * inverse_period_m) * period_m;
    return select(wrapped_m, wrapped_m - period_m, wrapped_m >= 0.5 * period_m);
}

fn canonical_world_position(position_m: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        canonical_world_component(
            position_m.x,
            scene.world_size_m.x,
            scene.inverse_world_size_m.x,
        ),
        position_m.y,
    );
}

fn camera_relative_position_m(position_m: vec2<f32>) -> vec2<f32> {
    let longitude_delta_m = position_m.x - scene.camera_and_view_m.x;
    return vec2<f32>(
        longitude_delta_m
            - round(longitude_delta_m * scene.inverse_world_size_m.x)
                * scene.world_size_m.x,
        position_m.y - scene.camera_and_view_m.y,
    );
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

fn add_u64(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let low = a.x + b.x;
    return vec2<u32>(low, a.y + b.y + select(0u, 1u, low < a.x));
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

struct TerrainNoiseSample {
    value: f32,
    gradient: vec2<f32>,
};

struct FractalNoiseSample {
    total: TerrainNoiseSample,
    broad: TerrainNoiseSample,
};

struct Simplex4dSample {
    value: f32,
    gradient: vec4<f32>,
};

// A lattice key XORs one product per lattice axis. Since (n + 1)C = nC + C
// modulo 2^64, every simplex vertex key derives from its cell's products by
// 64-bit addition.
const LATTICE_KEY_MULTIPLIERS = array<vec2<u32>, 4>(
    vec2<u32>(0x7f4a7c15u, 0x9e3779b9u),
    vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
    vec2<u32>(0x1ce4e5b9u, 0xbf58476du),
    vec2<u32>(0x133111ebu, 0x94d049bbu),
);

struct Simplex4dDomain {
    lattice_products: array<vec2<u32>, 4>,
    first: vec4<f32>,
    rank: vec4<i32>,
};

struct Simplex2dDomain {
    lattice_keys: array<vec2<u32>, 3>,
    offsets: array<vec2<f32>, 3>,
    x_cycles: i32,
    y_scale_per_m: f32,
};

struct TerrainSample {
    cluster: TerrainNoiseSample,
    elevation_m: f32,
    gradient: vec2<f32>,
    material_elevation_m: f32,
    material_gradient: vec2<f32>,
    ice_amount: f32,
    water_ice_amount: f32,
    bedrock_elevation_m: f32,
    maritime_temperature_anomaly_c: f32,
    coastal_rock_amount: f32,
    coastal_outcrop_amount: f32,
    coastal_sediment_amount: f32,
};

struct TerrainSurface {
    color: vec3<f32>,
    detail_gradient: vec2<f32>,
    roughness: f32,
};

fn noise_sample_or_zero(sample: TerrainNoiseSample, keep: bool) -> TerrainNoiseSample {
    return TerrainNoiseSample(
        select(0.0, sample.value, keep),
        select(vec2<f32>(0.0), sample.gradient, keep),
    );
}

fn select_terrain_surface(
    otherwise: TerrainSurface,
    chosen: TerrainSurface,
    condition: bool,
) -> TerrainSurface {
    return TerrainSurface(
        select(otherwise.color, chosen.color, condition),
        select(otherwise.detail_gradient, chosen.detail_gradient, condition),
        select(otherwise.roughness, chosen.roughness, condition),
    );
}

fn positive_mod(value: i32, period: i32) -> i32 {
    return ((value % period) + period) % period;
}

fn terrain_simplex_lattice_gradient(seed: vec2<u32>, lattice_key: vec2<u32>) -> vec2<f32> {
    return SIMPLEX_GRADIENT_DIRECTIONS[terrain_hash(seed ^ lattice_key).x % 24u];
}

fn terrain_simplex_domain(
    position_m: vec2<f32>,
    wavelength_m: f32,
) -> Simplex2dDomain {
    let skew_factor = 0.36602542;
    let unskew_factor = 0.21132487;
    let x_cycles = max(1, i32(round(scene.world_size_m.x / (sqrt(2.0) * wavelength_m))));
    let canonical_position = canonical_world_position(position_m);
    let positive_x = select(
        canonical_position.x,
        canonical_position.x + scene.world_size_m.x,
        canonical_position.x < 0.0,
    );
    let horizontal_phase = f32(x_cycles) * positive_x * scene.inverse_world_size_m.x;
    let y_scale_per_m = 1.0 / (sqrt(2.0) * wavelength_m);
    let vertical_phase = position_m.y * y_scale_per_m;
    let scaled = vec2<f32>(
        horizontal_phase + vertical_phase,
        -horizontal_phase + vertical_phase,
    );
    let skew = (scaled.x + scaled.y) * skew_factor;
    let cell = vec2<i32>(floor(scaled + vec2<f32>(skew)));
    let unskew = f32(cell.x + cell.y) * unskew_factor;
    let first_offset = scaled - (vec2<f32>(cell) - vec2<f32>(unskew));
    let middle_steps_x = first_offset.x > first_offset.y;
    let middle_lattice_offset = select(vec2<i32>(0, 1), vec2<i32>(1, 0), middle_steps_x);
    let middle_offset = first_offset
        - vec2<f32>(middle_lattice_offset)
        + vec2<f32>(unskew_factor);
    let last_offset = first_offset - vec2<f32>(1.0 - 2.0 * unskew_factor);
    // Lattice keys combine the wrapped x-y residue, the longitude axis of the
    // triangular lattice, with the unbounded x+y sum. Wrapping only the residue
    // preserves the cylindrical seam without repeating latitude. The first and
    // last vertices share a residue; the middle vertex steps one residue up or
    // down, and the sum rises by one per vertex.
    let residue_multiplier = LATTICE_KEY_MULTIPLIERS[0];
    let sum_multiplier = LATTICE_KEY_MULTIPLIERS[1];
    let residue_period = 2 * x_cycles;
    let lower_residue = positive_mod(
        cell.x - cell.y - select(1, 0, middle_steps_x),
        residue_period,
    );
    let lower_residue_product = multiply_u64(signed_u64(lower_residue), residue_multiplier);
    let upper_residue_product = select(
        add_u64(lower_residue_product, residue_multiplier),
        vec2<u32>(0u),
        lower_residue == residue_period - 1,
    );
    let first_residue_product = select(
        upper_residue_product,
        lower_residue_product,
        middle_steps_x,
    );
    let middle_residue_product = select(
        lower_residue_product,
        upper_residue_product,
        middle_steps_x,
    );
    let last_residue_product = first_residue_product;
    let first_sum_product = multiply_u64(signed_u64(cell.x + cell.y), sum_multiplier);
    let middle_sum_product = add_u64(first_sum_product, sum_multiplier);
    let last_sum_product = add_u64(middle_sum_product, sum_multiplier);
    return Simplex2dDomain(
        array<vec2<u32>, 3>(
            first_residue_product ^ first_sum_product,
            middle_residue_product ^ middle_sum_product,
            last_residue_product ^ last_sum_product,
        ),
        array<vec2<f32>, 3>(first_offset, middle_offset, last_offset),
        x_cycles,
        y_scale_per_m,
    );
}

fn terrain_simplex_noise(
    seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
) -> TerrainNoiseSample {
    let domain = terrain_simplex_domain(position_m, wavelength_m);
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    for (var vertex = 0u; vertex < 3u; vertex++) {
        let offset = domain.offsets[vertex];
        let attenuation = 0.5 - dot(offset, offset);
        if attenuation <= 0.0 {
            continue;
        }
        let gradient = terrain_simplex_lattice_gradient(seed, domain.lattice_keys[vertex]);
        let ramp = dot(gradient, offset);
        let attenuation_squared = attenuation * attenuation;
        let attenuation_cubed = attenuation_squared * attenuation;
        let attenuation_fourth = attenuation_squared * attenuation_squared;
        sample.value += attenuation_fourth * ramp;
        sample.gradient += attenuation_fourth * gradient
            - 8.0 * attenuation_cubed * ramp * offset;
    }
    sample.value *= 70.0;
    let scaled_gradient = 70.0 * sample.gradient;
    sample.gradient = vec2<f32>(
        f32(domain.x_cycles) * scene.inverse_world_size_m.x
            * (scaled_gradient.x - scaled_gradient.y),
        domain.y_scale_per_m * (scaled_gradient.x + scaled_gradient.y),
    );
    return sample;
}

fn terrain_simplex_noise_value(
    seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
) -> f32 {
    let domain = terrain_simplex_domain(position_m, wavelength_m);
    var value = 0.0;
    for (var vertex = 0u; vertex < 3u; vertex++) {
        let offset = domain.offsets[vertex];
        let attenuation = 0.5 - dot(offset, offset);
        if attenuation <= 0.0 {
            continue;
        }
        let gradient = terrain_simplex_lattice_gradient(seed, domain.lattice_keys[vertex]);
        let ramp = dot(gradient, offset);
        let attenuation_squared = attenuation * attenuation;
        value += attenuation_squared * attenuation_squared * ramp;
    }
    return 70.0 * value;
}

fn terrain_simplex_lattice_gradient_4d(
    seed: vec2<u32>,
    domain: Simplex4dDomain,
    lattice_offset: vec4<i32>,
) -> vec4<f32> {
    var key = seed;
    for (var axis = 0u; axis < 4u; axis++) {
        key ^= add_u64(
            domain.lattice_products[axis],
            select(
                vec2<u32>(0u),
                LATTICE_KEY_MULTIPLIERS[axis],
                lattice_offset[axis] != 0i,
            ),
        );
    }
    let hash = terrain_hash(key).x;
    let zero_axis = hash & 3u;
    var sign_bit = 2u;
    var gradient = vec4<f32>(0.0);
    for (var axis = 0u; axis < 4u; axis++) {
        if axis != zero_axis {
            gradient[axis] = select(1.0, -1.0, (hash & (1u << sign_bit)) != 0u);
            sign_bit += 1u;
        }
    }
    return gradient;
}

fn periodic_noise_circle_position_from_canonical(position_m: vec2<f32>) -> vec4<f32> {
    let angle = TAU * position_m.x * scene.inverse_world_size_m.x;
    return vec4<f32>(
        cos(angle),
        sin(angle),
        position_m.y,
        0.0,
    );
}

fn periodic_noise_circle_position(position_m: vec2<f32>) -> vec4<f32> {
    return periodic_noise_circle_position_from_canonical(
        canonical_world_position(position_m),
    );
}

fn simplex_noise_domain_4d(point: vec4<f32>) -> Simplex4dDomain {
    let skew_factor = 0.309017;
    let unskew_factor = 0.1381966;
    let skew = dot(point, vec4<f32>(skew_factor));
    let lattice = vec4<i32>(floor(point + vec4<f32>(skew)));
    let unskew = f32(lattice.x + lattice.y + lattice.z + lattice.w) * unskew_factor;
    let first = point - (vec4<f32>(lattice) - vec4<f32>(unskew));
    var rank = vec4<i32>(0);
    if first.x > first.y { rank.x += 1; } else { rank.y += 1; }
    if first.x > first.z { rank.x += 1; } else { rank.z += 1; }
    if first.x > first.w { rank.x += 1; } else { rank.w += 1; }
    if first.y > first.z { rank.y += 1; } else { rank.z += 1; }
    if first.y > first.w { rank.y += 1; } else { rank.w += 1; }
    if first.z > first.w { rank.z += 1; } else { rank.w += 1; }
    var lattice_products: array<vec2<u32>, 4>;
    for (var axis = 0u; axis < 4u; axis++) {
        lattice_products[axis] = multiply_u64(
            signed_u64(lattice[axis]),
            LATTICE_KEY_MULTIPLIERS[axis],
        );
    }
    return Simplex4dDomain(lattice_products, first, rank);
}

fn simplex_noise_sample_4d(seed: vec2<u32>, point: vec4<f32>) -> Simplex4dSample {
    let unskew_factor = 0.1381966;
    let domain = simplex_noise_domain_4d(point);
    var sample: Simplex4dSample;
    sample.value = 0.0;
    sample.gradient = vec4<f32>(0.0);
    for (var vertex = 0i; vertex < 5i; vertex++) {
        let threshold = 4i - vertex;
        var lattice_offset = vec4<i32>(0);
        if vertex > 0i {
            lattice_offset = select(
                vec4<i32>(0),
                vec4<i32>(1),
                domain.rank >= vec4<i32>(threshold),
            );
        }
        let offset = domain.first - vec4<f32>(lattice_offset)
            + vec4<f32>(f32(vertex) * unskew_factor);
        // The largest squared radius whose kernel vanishes on every simplex
        // boundary, which keeps the noise continuous. Keep the 4D kernels
        // here and in terrain-cache-generate.wgsl synchronized with Rust.
        let attenuation = 0.5 - dot(offset, offset);
        if attenuation > 0.0 {
            let gradient = terrain_simplex_lattice_gradient_4d(
                seed,
                domain,
                lattice_offset,
            );
            let ramp = dot(gradient, offset);
            let attenuation_squared = attenuation * attenuation;
            let attenuation_cubed = attenuation_squared * attenuation;
            let attenuation_fourth = attenuation_squared * attenuation_squared;
            sample.value += attenuation_fourth * ramp;
            sample.gradient += attenuation_fourth * gradient
                - 8.0 * attenuation_cubed * ramp * offset;
        }
    }
    // Keep this slope-RMS normalization synchronized with
    // `simplex_noise_sample_4d` in crates/game/src/lib.rs.
    sample.value *= 83.1;
    sample.gradient *= 83.1;
    return sample;
}

fn simplex_noise_value_4d(seed: vec2<u32>, point: vec4<f32>) -> f32 {
    let unskew_factor = 0.1381966;
    let domain = simplex_noise_domain_4d(point);
    var value = 0.0;
    for (var vertex = 0i; vertex < 5i; vertex++) {
        let threshold = 4i - vertex;
        var lattice_offset = vec4<i32>(0);
        if vertex > 0i {
            lattice_offset = select(
                vec4<i32>(0),
                vec4<i32>(1),
                domain.rank >= vec4<i32>(threshold),
            );
        }
        let offset = domain.first - vec4<f32>(lattice_offset)
            + vec4<f32>(f32(vertex) * unskew_factor);
        let attenuation = 0.5 - dot(offset, offset);
        if attenuation > 0.0 {
            let gradient = terrain_simplex_lattice_gradient_4d(
                seed,
                domain,
                lattice_offset,
            );
            let ramp = dot(gradient, offset);
            let attenuation_squared = attenuation * attenuation;
            value += attenuation_squared * attenuation_squared * ramp;
        }
    }
    return 83.1 * value;
}

fn terrain_simplex_noise_4d_from_circle(
    seed: vec2<u32>,
    circle_position: vec4<f32>,
    wavelength_m: f32,
) -> TerrainNoiseSample {
    let circle_radius = scene.world_size_m.x / (TAU * wavelength_m);
    let linear_y = circle_position.z / (sqrt(2.0) * wavelength_m);
    let simplex = simplex_noise_sample_4d(seed, vec4<f32>(
        circle_radius * circle_position.x,
        circle_radius * circle_position.y,
        linear_y,
        linear_y + 0.5,
    ));
    // Keep synchronized with `terrain_simplex_noise_4d` in crates/game/src/lib.rs,
    // which records its calibration.
    let cylindrical_scale = 114.3 / 83.1;
    return TerrainNoiseSample(
        cylindrical_scale * simplex.value,
        cylindrical_scale * vec2<f32>(
            -circle_position.y * simplex.gradient.x
                + circle_position.x * simplex.gradient.y,
            (simplex.gradient.z + simplex.gradient.w) / sqrt(2.0),
        ) / wavelength_m,
    );
}

fn terrain_simplex_noise_value_4d_from_circle(
    seed: vec2<u32>,
    circle_position: vec4<f32>,
    wavelength_m: f32,
) -> f32 {
    let circle_radius = scene.world_size_m.x / (TAU * wavelength_m);
    let linear_y = circle_position.z / (sqrt(2.0) * wavelength_m);
    return (114.3 / 83.1) * simplex_noise_value_4d(seed, vec4<f32>(
        circle_radius * circle_position.x,
        circle_radius * circle_position.y,
        linear_y,
        linear_y + 0.5,
    ));
}

fn terrain_simplex_noise_4d(
    seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
) -> TerrainNoiseSample {
    return terrain_simplex_noise_4d_from_circle(
        seed,
        periodic_noise_circle_position(position_m),
        wavelength_m,
    );
}

fn terrain_simplex_fractal_sample(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    persistence: f32,
    circle_octave_count: u32,
) -> TerrainNoiseSample {
    return terrain_simplex_fractal_sample_from_circle(
        field_salt,
        position_m,
        periodic_noise_circle_position(position_m),
        initial_wavelength_m,
        octave_count,
        persistence,
        circle_octave_count,
    );
}

fn terrain_simplex_fractal_sample_from_circle(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    persistence: f32,
    circle_octave_count: u32,
) -> TerrainNoiseSample {
    let sample = terrain_simplex_octave_sum_from_circle(
        field_salt,
        position_m,
        circle_position,
        initial_wavelength_m,
        0u,
        octave_count,
        persistence,
        circle_octave_count,
    );
    let amplitude_sum = fractal_amplitude_sum(octave_count, persistence);
    return TerrainNoiseSample(
        sample.value / amplitude_sum,
        sample.gradient / amplitude_sum,
    );
}

fn fractal_amplitude_sum(octave_count: u32, persistence: f32) -> f32 {
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    for (var octave = 0u; octave < octave_count; octave++) {
        amplitude_sum += amplitude;
        amplitude *= persistence;
    }
    return amplitude_sum;
}

// The amplitude-weighted sum of octaves `first_octave` up to `end_octave`,
// before the fractal's normalization.
fn terrain_simplex_octave_sum_from_circle(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
    initial_wavelength_m: f32,
    first_octave: u32,
    end_octave: u32,
    persistence: f32,
    circle_octave_count: u32,
) -> TerrainNoiseSample {
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    var amplitude = pow(persistence, f32(first_octave));
    var wavelength_m = initial_wavelength_m * tuning.terrain_tuning.x
        * exp2(-f32(first_octave));
    for (var octave = first_octave; octave < end_octave; octave++) {
        let octave_seed = scene.match_seed
            ^ field_salt
            ^ OCTAVE_SALTS[octave];
        var octave_sample: TerrainNoiseSample;
        if octave < circle_octave_count {
            octave_sample = terrain_simplex_noise_4d_from_circle(
                octave_seed,
                circle_position,
                wavelength_m,
            );
        } else {
            octave_sample = terrain_simplex_noise(octave_seed, position_m, wavelength_m);
        }
        sample.value += amplitude * octave_sample.value;
        sample.gradient += amplitude * octave_sample.gradient;
        amplitude *= persistence;
        wavelength_m *= 0.5;
    }
    return sample;
}

fn terrain_simplex_fractal_value(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    persistence: f32,
    circle_octave_count: u32,
) -> f32 {
    return terrain_simplex_fractal_value_from_circle(
        field_salt,
        position_m,
        periodic_noise_circle_position(position_m),
        initial_wavelength_m,
        octave_count,
        persistence,
        circle_octave_count,
    );
}

fn terrain_simplex_fractal_value_from_circle(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    persistence: f32,
    circle_octave_count: u32,
) -> f32 {
    var value = 0.0;
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m * tuning.terrain_tuning.x;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_seed = scene.match_seed
            ^ field_salt
            ^ OCTAVE_SALTS[octave];
        var octave_value: f32;
        if octave < circle_octave_count {
            octave_value = terrain_simplex_noise_value_4d_from_circle(
                octave_seed,
                circle_position,
                wavelength_m,
            );
        } else {
            octave_value = terrain_simplex_noise_value(
                octave_seed,
                position_m,
                wavelength_m,
            );
        }
        value += amplitude * octave_value;
        amplitude_sum += amplitude;
        amplitude *= persistence;
        wavelength_m *= 0.5;
    }
    return value / amplitude_sum;
}

fn spatiotemporal_fractal_sample(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    initial_time_coordinate: f32,
    octave_count: u32,
    persistence: f32,
) -> TerrainNoiseSample {
    return spatiotemporal_fractal_sample_with_broad(
        field_salt,
        position_m,
        initial_wavelength_m,
        initial_time_coordinate,
        octave_count,
        persistence,
    ).total;
}

fn spatiotemporal_fractal_sample_with_broad(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    initial_time_coordinate: f32,
    octave_count: u32,
    persistence: f32,
) -> FractalNoiseSample {
    var sample = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    var broad = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    var time_coordinate = initial_time_coordinate;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_seed = scene.match_seed
            ^ field_salt
            ^ OCTAVE_SALTS[octave];
        let octave_sample = spatiotemporal_noise_sample(
            octave_seed,
            position_m,
            wavelength_m,
            time_coordinate,
        );
        if octave == 0u { broad = octave_sample; }
        sample.value += amplitude * octave_sample.value;
        sample.gradient += amplitude * octave_sample.gradient;
        amplitude_sum += amplitude;
        amplitude *= persistence;
        wavelength_m *= 0.5;
        time_coordinate *= 2.0;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
    return FractalNoiseSample(sample, broad);
}

fn spatiotemporal_fractal_value(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    initial_time_coordinate: f32,
    octave_count: u32,
    persistence: f32,
) -> f32 {
    return spatiotemporal_fractal_value_with_broad(
        field_salt,
        position_m,
        initial_wavelength_m,
        initial_time_coordinate,
        octave_count,
        persistence,
    ).x;
}

fn spatiotemporal_fractal_value_with_broad(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    initial_time_coordinate: f32,
    octave_count: u32,
    persistence: f32,
) -> vec2<f32> {
    var value = 0.0;
    var broad = 0.0;
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    var time_coordinate = initial_time_coordinate;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_seed = scene.match_seed
            ^ field_salt
            ^ OCTAVE_SALTS[octave];
        let octave_value = spatiotemporal_noise_value(
            octave_seed,
            position_m,
            wavelength_m,
            time_coordinate,
        );
        if octave == 0u { broad = octave_value; }
        value += amplitude * octave_value;
        amplitude_sum += amplitude;
        amplitude *= persistence;
        wavelength_m *= 0.5;
        time_coordinate *= 2.0;
    }
    return vec2<f32>(value / amplitude_sum, broad);
}

fn spatial_weather_fractal_sample(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    circle_octave_count: u32,
) -> TerrainNoiseSample {
    return spatial_weather_fractal_sample_from_circle(
        field_salt,
        position_m,
        periodic_noise_circle_position(position_m),
        initial_wavelength_m,
        octave_count,
        circle_octave_count,
    );
}

fn spatial_weather_fractal_sample_from_circle(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    circle_octave_count: u32,
) -> TerrainNoiseSample {
    var sample = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_seed = scene.match_seed
            ^ field_salt
            ^ OCTAVE_SALTS[octave];
        var octave_sample: TerrainNoiseSample;
        if octave < circle_octave_count {
            octave_sample = terrain_simplex_noise_4d_from_circle(
                octave_seed,
                circle_position,
                wavelength_m,
            );
        } else {
            octave_sample = terrain_simplex_noise(octave_seed, position_m, wavelength_m);
        }
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

fn spatial_weather_fractal(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    circle_octave_count: u32,
) -> f32 {
    return spatial_weather_fractal_from_circle(
        field_salt,
        position_m,
        periodic_noise_circle_position(position_m),
        initial_wavelength_m,
        octave_count,
        circle_octave_count,
    );
}

fn spatial_weather_fractal_from_circle(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    circle_octave_count: u32,
) -> f32 {
    var value = 0.0;
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_seed = scene.match_seed
            ^ field_salt
            ^ OCTAVE_SALTS[octave];
        var octave_value: f32;
        if octave < circle_octave_count {
            octave_value = terrain_simplex_noise_value_4d_from_circle(
                octave_seed,
                circle_position,
                wavelength_m,
            );
        } else {
            octave_value = terrain_simplex_noise_value(
                octave_seed,
                position_m,
                wavelength_m,
            );
        }
        value += amplitude * octave_value;
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.5;
    }
    return value / amplitude_sum;
}

fn material_fractal_sample(
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
        let octave_visibility = smoothstep(
            visual_pixel_footprint_m,
            2.0 * visual_pixel_footprint_m,
            wavelength_m,
        );
        if octave_visibility > 0.0 || EVALUATE_ALL_BRANCHES {
            let octave_seed = scene.match_seed
                ^ field_salt
                ^ OCTAVE_SALTS[octave];
            let octave_sample = terrain_simplex_noise(
                octave_seed,
                position_m,
                wavelength_m,
            );
            sample.value += amplitude * octave_visibility * octave_sample.value;
            sample.gradient += amplitude * octave_visibility * octave_sample.gradient;
        }
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.53;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
    return sample;
}

fn material_fractal_value(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
) -> f32 {
    var value = 0.0;
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_visibility = smoothstep(
            visual_pixel_footprint_m,
            2.0 * visual_pixel_footprint_m,
            wavelength_m,
        );
        if octave_visibility > 0.0 || EVALUATE_ALL_BRANCHES {
            let octave_seed = scene.match_seed
                ^ field_salt
                ^ OCTAVE_SALTS[octave];
            value += amplitude * octave_visibility * terrain_simplex_noise_value(
                octave_seed,
                position_m,
                wavelength_m,
            );
        }
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.53;
    }
    return value / amplitude_sum;
}
