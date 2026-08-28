fn canonical_world_component(value_m: f32, period_m: f32) -> f32 {
    let wrapped_m = value_m - floor(value_m / period_m) * period_m;
    return select(wrapped_m, wrapped_m - period_m, wrapped_m >= 0.5 * period_m);
}

fn canonical_world_position(position_m: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        canonical_world_component(position_m.x, scene.world_size_m.x),
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

struct TerrainSample {
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

fn terrain_simplex_noise(
    seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
) -> TerrainNoiseSample {
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
    let normalized_position = positive_position / scene.world_size_m;
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

    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    let lattice_offsets = array<vec2<i32>, 3>(
        vec2<i32>(0),
        middle_lattice_offset,
        vec2<i32>(1),
    );
    let offsets = array<vec2<f32>, 3>(first_offset, middle_offset, last_offset);
    for (var vertex = 0u; vertex < 3u; vertex++) {
        let offset = offsets[vertex];
        let attenuation = 0.5 - dot(offset, offset);
        if attenuation <= 0.0 {
            continue;
        }
        let lattice = cell + lattice_offsets[vertex];
        let gradient = terrain_simplex_lattice_gradient(
            seed,
            lattice.x,
            lattice.y,
            x_cycles,
            y_cycles,
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
        f32(x_cycles) / scene.world_size_m.x * (scaled_gradient.x - scaled_gradient.y),
        f32(y_cycles) / (sqrt(3.0) * scene.world_size_m.y)
            * (scaled_gradient.x + scaled_gradient.y),
    );
    return sample;
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

fn terrain_simplex_noise_4d(
    seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
) -> TerrainNoiseSample {
    let canonical_position = canonical_world_position(position_m);
    let angle = TAU * canonical_position / scene.world_size_m;
    let circle_radius = scene.world_size_m / (TAU * wavelength_m);
    let point = vec4<f32>(
        circle_radius.x * cos(angle.x),
        circle_radius.x * sin(angle.x),
        circle_radius.y * cos(angle.y),
        circle_radius.y * sin(angle.y),
    );
    let simplex = simplex_noise_sample_4d(seed, point);
    return TerrainNoiseSample(
        simplex.value,
        vec2<f32>(
            -sin(angle.x) * simplex.gradient.x + cos(angle.x) * simplex.gradient.y,
            -sin(angle.y) * simplex.gradient.z + cos(angle.y) * simplex.gradient.w,
        ) / wavelength_m,
    );
}

fn simplex_noise_sample_4d(seed: vec2<u32>, point: vec4<f32>) -> Simplex4dSample {
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
                rank >= vec4<i32>(threshold),
            );
        }
        let offset = first - vec4<f32>(lattice_offset)
            + vec4<f32>(f32(vertex) * unskew_factor);
        let attenuation = 0.6 - dot(offset, offset);
        if attenuation > 0.0 {
            let gradient = terrain_simplex_lattice_gradient_4d(seed, lattice + lattice_offset);
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

fn terrain_simplex_fractal_sample(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
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
            ^ multiply_u64(
                vec2<u32>(octave, 0u),
                vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            );
        var octave_sample: TerrainNoiseSample;
        if octave < circle_octave_count {
            octave_sample = terrain_simplex_noise_4d(octave_seed, position_m, wavelength_m);
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
            ^ multiply_u64(
                vec2<u32>(octave, 0u),
                vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            );
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

fn spatial_weather_fractal(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    circle_octave_count: u32,
) -> f32 {
    return spatial_weather_fractal_sample(
        field_salt,
        position_m,
        initial_wavelength_m,
        octave_count,
        circle_octave_count,
    ).value;
}

fn spatial_weather_fractal_sample(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
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
            ^ multiply_u64(
                vec2<u32>(octave, 0u),
                vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            );
        var octave_sample: TerrainNoiseSample;
        if octave < circle_octave_count {
            octave_sample = terrain_simplex_noise_4d(
                octave_seed,
                position_m,
                wavelength_m,
            );
        } else {
            octave_sample = terrain_simplex_noise(
                octave_seed,
                position_m,
                wavelength_m,
            );
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
                ^ multiply_u64(
                    vec2<u32>(octave, 0u),
                    vec2<u32>(0xd192ed03u, 0xd1b54a32u),
                );
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

fn visual_transported_noise_sample(
    current_position_m: vec2<f32>,
    next_position_m: vec2<f32>,
    field_salt: vec2<u32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    anchor_epoch: vec2<u32>,
    anchor_phase: f32,
    pixel_footprint_m: f32,
) -> TerrainNoiseSample {
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    let next_epoch = anchor_epoch + vec2<u32>(1u, select(0u, 1u, anchor_epoch.x == 0xffffffffu));
    let phase = clamp(anchor_phase, 0.0, 1.0);
    let smoother_phase = phase * phase * phase
        * (phase * (phase * 6.0 - 15.0) + 10.0);
    let angle = 0.5 * PI * smoother_phase;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_visibility = smoothstep(
            pixel_footprint_m,
            2.0 * pixel_footprint_m,
            wavelength_m,
        );
        if octave_visibility > 0.0 {
            let octave_seed = scene.match_seed
                ^ field_salt
                ^ multiply_u64(
                    vec2<u32>(octave, 0u),
                    vec2<u32>(0xd192ed03u, 0xd1b54a32u),
                );
            let current_sample = terrain_simplex_noise_4d(
                octave_seed ^ multiply_u64(
                    anchor_epoch,
                    vec2<u32>(0x94d049bbu, 0x133111ebu),
                ),
                current_position_m,
                wavelength_m,
            );
            let next_sample = terrain_simplex_noise_4d(
                octave_seed ^ multiply_u64(
                    next_epoch,
                    vec2<u32>(0x94d049bbu, 0x133111ebu),
                ),
                next_position_m,
                wavelength_m,
            );
            var octave_sample: TerrainNoiseSample;
            octave_sample.value = cos(angle) * current_sample.value
                + sin(angle) * next_sample.value;
            octave_sample.gradient = cos(angle) * current_sample.gradient
                + sin(angle) * next_sample.gradient;
            sample.value += amplitude * octave_visibility * octave_sample.value;
            sample.gradient += amplitude * octave_visibility * octave_sample.gradient;
        }
        amplitude_sum += amplitude;
        amplitude *= 0.58;
        wavelength_m *= 0.5;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
    return sample;
}
