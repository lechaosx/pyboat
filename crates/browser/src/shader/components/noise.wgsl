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

struct TerrainNoiseSample {
    value: f32,
    gradient: vec2<f32>,
};

struct Simplex4dSample {
    value: f32,
    gradient: vec4<f32>,
};

struct Simplex4dDomain {
    lattice: vec4<i32>,
    first: vec4<f32>,
    rank: vec4<i32>,
};

struct Simplex2dDomain {
    cell: vec2<i32>,
    lattice_offsets: array<vec2<i32>, 3>,
    offsets: array<vec2<f32>, 3>,
    x_cycles: i32,
    y_cycles: i32,
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

fn positive_mod(value: i32, period: i32) -> i32 {
    return ((value % period) + period) % period;
}

fn terrain_simplex_lattice_gradient(
    seed: vec2<u32>,
    x: i32,
    y: i32,
    x_cycles: i32,
    y_cycles: i32,
) -> vec2<f32> {
    // These residues are the integer axes of the triangular lattice. Integer
    // cycle counts make each procedural calibration period an exact lattice shift.
    let periodic_x = positive_mod(x - y, 2 * x_cycles);
    let periodic_y = positive_mod(x + y, 2 * y_cycles);
    let key = seed
        ^ multiply_u64(
            signed_u64(periodic_x),
            vec2<u32>(0x7f4a7c15u, 0x9e3779b9u),
        )
        ^ multiply_u64(
            signed_u64(periodic_y),
            vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        );
    return SIMPLEX_GRADIENT_DIRECTIONS[terrain_hash(key).x % 24u];
}

fn terrain_simplex_domain(
    position_m: vec2<f32>,
    wavelength_m: f32,
) -> Simplex2dDomain {
    let skew_factor = 0.36602542;
    let unskew_factor = 0.21132487;
    let x_cycles = max(1, i32(round(scene.world_size_m.x / (sqrt(2.0) * wavelength_m))));
    let y_cycles = max(1, i32(round(
        scene.world_size_m.y * sqrt(1.5) / wavelength_m,
    )));
    let canonical_position = canonical_world_position(position_m);
    let positive_position = select(
        canonical_position,
        canonical_position + scene.world_size_m,
        canonical_position < vec2<f32>(0.0),
    );
    let normalized_position = positive_position
        * scene.inverse_world_size_m;
    let scaled = vec2<f32>(
        f32(x_cycles) * normalized_position.x
            + f32(y_cycles) * normalized_position.y / sqrt(3.0),
        -f32(x_cycles) * normalized_position.x
            + f32(y_cycles) * normalized_position.y / sqrt(3.0),
    );
    let skew = (scaled.x + scaled.y) * skew_factor;
    let cell = vec2<i32>(floor(scaled + vec2<f32>(skew)));
    let unskew = f32(cell.x + cell.y) * unskew_factor;
    let first_offset = scaled - (vec2<f32>(cell) - vec2<f32>(unskew));
    let middle_lattice_offset = select(
        vec2<i32>(0, 1),
        vec2<i32>(1, 0),
        first_offset.x > first_offset.y,
    );
    let middle_offset = first_offset
        - vec2<f32>(middle_lattice_offset)
        + vec2<f32>(unskew_factor);
    let last_offset = first_offset - vec2<f32>(1.0 - 2.0 * unskew_factor);
    return Simplex2dDomain(
        cell,
        array<vec2<i32>, 3>(vec2<i32>(0), middle_lattice_offset, vec2<i32>(1)),
        array<vec2<f32>, 3>(first_offset, middle_offset, last_offset),
        x_cycles,
        y_cycles,
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
        let lattice = domain.cell + domain.lattice_offsets[vertex];
        let gradient = terrain_simplex_lattice_gradient(
            seed,
            lattice.x,
            lattice.y,
            domain.x_cycles,
            domain.y_cycles,
        );
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
        f32(domain.y_cycles) * scene.inverse_world_size_m.y
            / sqrt(3.0)
            * (scaled_gradient.x + scaled_gradient.y),
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
        let lattice = domain.cell + domain.lattice_offsets[vertex];
        let gradient = terrain_simplex_lattice_gradient(
            seed,
            lattice.x,
            lattice.y,
            domain.x_cycles,
            domain.y_cycles,
        );
        let ramp = dot(gradient, offset);
        let attenuation_squared = attenuation * attenuation;
        value += attenuation_squared * attenuation_squared * ramp;
    }
    return 70.0 * value;
}

fn terrain_simplex_lattice_gradient_4d(
    seed: vec2<u32>,
    lattice: vec4<i32>,
) -> vec4<f32> {
    let key = seed
        ^ multiply_u64(signed_u64(lattice.x), vec2<u32>(0x7f4a7c15u, 0x9e3779b9u))
        ^ multiply_u64(signed_u64(lattice.y), vec2<u32>(0xa0b428dbu, 0xe7037ed1u))
        ^ multiply_u64(signed_u64(lattice.z), vec2<u32>(0x1ce4e5b9u, 0xbf58476du))
        ^ multiply_u64(signed_u64(lattice.w), vec2<u32>(0x133111ebu, 0x94d049bbu));
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
    let angle = TAU * position_m
        * scene.inverse_world_size_m;
    return vec4<f32>(
        cos(angle.x),
        sin(angle.x),
        cos(angle.y),
        sin(angle.y),
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
    return Simplex4dDomain(lattice, first, rank);
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
        let attenuation = 0.6 - dot(offset, offset);
        if attenuation > 0.0 {
            let gradient = terrain_simplex_lattice_gradient_4d(
                seed,
                domain.lattice + lattice_offset,
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
    // Keep this RMS calibration synchronized with the portable Rust kernel.
    sample.value *= 33.4;
    sample.gradient *= 33.4;
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
        let attenuation = 0.6 - dot(offset, offset);
        if attenuation > 0.0 {
            let gradient = terrain_simplex_lattice_gradient_4d(
                seed,
                domain.lattice + lattice_offset,
            );
            let ramp = dot(gradient, offset);
            let attenuation_squared = attenuation * attenuation;
            value += attenuation_squared * attenuation_squared * ramp;
        }
    }
    return 33.4 * value;
}

fn terrain_simplex_noise_4d_from_circle(
    seed: vec2<u32>,
    circle_position: vec4<f32>,
    wavelength_m: f32,
) -> TerrainNoiseSample {
    let circle_radius = scene.world_size_m / (TAU * wavelength_m);
    let simplex = simplex_noise_sample_4d(seed, vec4<f32>(
        circle_radius.x * circle_position.x,
        circle_radius.x * circle_position.y,
        circle_radius.y * circle_position.z,
        circle_radius.y * circle_position.w,
    ));
    return TerrainNoiseSample(
        simplex.value,
        vec2<f32>(
            -circle_position.y * simplex.gradient.x
                + circle_position.x * simplex.gradient.y,
            -circle_position.w * simplex.gradient.z
                + circle_position.z * simplex.gradient.w,
        ) / wavelength_m,
    );
}

fn terrain_simplex_noise_value_4d_from_circle(
    seed: vec2<u32>,
    circle_position: vec4<f32>,
    wavelength_m: f32,
) -> f32 {
    let circle_radius = scene.world_size_m / (TAU * wavelength_m);
    return simplex_noise_value_4d(seed, vec4<f32>(
        circle_radius.x * circle_position.x,
        circle_radius.x * circle_position.y,
        circle_radius.y * circle_position.z,
        circle_radius.y * circle_position.w,
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
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m * tuning.terrain_tuning.x;
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
        amplitude *= persistence;
        wavelength_m *= 0.5;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
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
    var sample = TerrainNoiseSample(0.0, vec2<f32>(0.0));
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
        sample.value += amplitude * octave_sample.value;
        sample.gradient += amplitude * octave_sample.gradient;
        amplitude_sum += amplitude;
        amplitude *= persistence;
        wavelength_m *= 0.5;
        time_coordinate *= 2.0;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
    return sample;
}

fn spatiotemporal_fractal_value(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    initial_time_coordinate: f32,
    octave_count: u32,
    persistence: f32,
) -> f32 {
    var value = 0.0;
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    var time_coordinate = initial_time_coordinate;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_seed = scene.match_seed
            ^ field_salt
            ^ OCTAVE_SALTS[octave];
        value += amplitude * spatiotemporal_noise_value(
            octave_seed,
            position_m,
            wavelength_m,
            time_coordinate,
        );
        amplitude_sum += amplitude;
        amplitude *= persistence;
        wavelength_m *= 0.5;
        time_coordinate *= 2.0;
    }
    return value / amplitude_sum;
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
        if octave_visibility > 0.0 {
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
        if octave_visibility > 0.0 {
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
