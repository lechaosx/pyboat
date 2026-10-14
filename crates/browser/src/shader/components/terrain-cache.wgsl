// Broad terrain noises' circle octaves kept between frames in a toroidal
// clipmap on world-anchored lattices. Each texel holds the cached octaves'
// summed value, gradient, and cross derivative, so bicubic Hermite
// reconstruction is accurate to fourth order. `scene.terrain_cache_grid_m` is
// the active level's first valid texel centre relative to the camera, then its
// spacing; `scene.terrain_cache_texels` is that texel's toroidal slot, then the
// valid extent; `scene.terrain_cache_level.x` is the level. Each level has one
// layer per broad noise and one layer of per-noise flags for cells that cross a
// jump in the 4D simplex kernel. Positions outside the valid extent, and
// flagged cells, evaluate the noises directly.
@group(0) @binding(4)
var terrain_cache: texture_2d_array<f32>;

@group(0) @binding(5)
var terrain_cache_jumps: texture_2d_array<f32>;

fn terrain_broad_noise(
    field: u32,
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
) -> TerrainNoiseSample {
    let grid = scene.terrain_cache_grid_m;
    let cached_octaves = terrain_cache_octaves(
        field,
        max(grid.z, grid.w),
        tuning.terrain_tuning.x,
    );
    let texel_position = (camera_relative_position_m(position_m) - grid.xy) / grid.zw;
    let texel = floor(texel_position);
    // The texture side is a power of two, so masking wraps negative offsets too.
    let slot_mask = textureDimensions(terrain_cache).x - 1u;
    let first = scene.terrain_cache_texels.xy + vec2<u32>(vec2<i32>(texel));
    let lower = first & vec2<u32>(slot_mask);
    let inside = cached_octaves > 0u
        && all(texel >= vec2<f32>(0.0))
        && all(texel + 1.0 < vec2<f32>(scene.terrain_cache_texels.zw));
    let level = scene.terrain_cache_level.x;
    let cached = inside
        && textureLoad(terrain_cache_jumps, lower, level, 0)[field] == 0.0;
    var sample = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    if !cached || EVALUATE_ALL_BRANCHES {
        sample = direct_terrain_broad_noise(field, position_m, circle_position);
    }
    if cached || EVALUATE_ALL_BRANCHES {
        let upper = (first + 1u) & vec2<u32>(slot_mask);
        let layer = level * TERRAIN_BROAD_NOISE_COUNT + field;
        let partial = terrain_cache_hermite(
            textureLoad(terrain_cache, lower, layer, 0),
            textureLoad(terrain_cache, vec2<u32>(upper.x, lower.y), layer, 0),
            textureLoad(terrain_cache, vec2<u32>(lower.x, upper.y), layer, 0),
            textureLoad(terrain_cache, upper, layer, 0),
            texel_position - texel,
            grid.zw,
        );
        let octaves = TERRAIN_BROAD_NOISE_OCTAVES[field];
        let tail = terrain_broad_octave_sum(
            field,
            position_m,
            circle_position,
            cached_octaves,
            octaves,
        );
        let amplitude_sum = fractal_amplitude_sum(
            octaves,
            TERRAIN_BROAD_NOISE_PERSISTENCE[field],
        );
        sample = noise_sample_or_zero(sample, !cached);
        sample.value += select(0.0, (partial.value + tail.value) / amplitude_sum, cached);
        sample.gradient += select(
            vec2<f32>(0.0),
            (partial.gradient + tail.gradient) / amplitude_sum,
            cached,
        );
    }
    return sample;
}

// Corners are (value, x slope, y slope, cross derivative) at the lower-left,
// lower-right, upper-left, and upper-right texels.
fn terrain_cache_hermite(
    lower_left: vec4<f32>,
    lower_right: vec4<f32>,
    upper_left: vec4<f32>,
    upper_right: vec4<f32>,
    fraction: vec2<f32>,
    spacing_m: vec2<f32>,
) -> TerrainNoiseSample {
    let t = fraction;
    let t2 = t * t;
    let t3 = t2 * t;
    // Cubic Hermite basis per axis for the lower and upper texel: value
    // weights, slope weights in metres, and their derivatives along the axis.
    let upper_value = 3.0 * t2 - 2.0 * t3;
    let lower_value = 1.0 - upper_value;
    let lower_slope = (t3 - 2.0 * t2 + t) * spacing_m;
    let upper_slope = (t3 - t2) * spacing_m;
    let upper_value_derivative = 6.0 * (t - t2);
    let lower_value_derivative = -upper_value_derivative;
    let lower_slope_derivative = (3.0 * t2 - 4.0 * t + 1.0) * spacing_m;
    let upper_slope_derivative = (3.0 * t2 - 2.0 * t) * spacing_m;
    let corners = array<vec4<f32>, 4>(lower_left, lower_right, upper_left, upper_right);
    var value = 0.0;
    var gradient = vec2<f32>(0.0);
    for (var corner = 0u; corner < 4u; corner++) {
        let right = (corner & 1u) == 1u;
        let up = corner >= 2u;
        let x_value = select(lower_value.x, upper_value.x, right);
        let x_slope = select(lower_slope.x, upper_slope.x, right);
        let x_value_derivative = select(lower_value_derivative.x, upper_value_derivative.x, right);
        let x_slope_derivative = select(lower_slope_derivative.x, upper_slope_derivative.x, right);
        let y_value = select(lower_value.y, upper_value.y, up);
        let y_slope = select(lower_slope.y, upper_slope.y, up);
        let y_value_derivative = select(lower_value_derivative.y, upper_value_derivative.y, up);
        let y_slope_derivative = select(lower_slope_derivative.y, upper_slope_derivative.y, up);
        let texel = corners[corner];
        value += dot(
            vec4<f32>(x_value * y_value, x_slope * y_value, x_value * y_slope, x_slope * y_slope),
            texel,
        );
        gradient += vec2<f32>(
            dot(
                vec4<f32>(
                    x_value_derivative * y_value,
                    x_slope_derivative * y_value,
                    x_value_derivative * y_slope,
                    x_slope_derivative * y_slope,
                ),
                texel,
            ),
            dot(
                vec4<f32>(
                    x_value * y_value_derivative,
                    x_slope * y_value_derivative,
                    x_value * y_slope_derivative,
                    x_slope * y_slope_derivative,
                ),
                texel,
            ),
        );
    }
    return TerrainNoiseSample(value, gradient / spacing_m);
}
