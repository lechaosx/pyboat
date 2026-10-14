// Keep synchronized with `FillRect` and `MAX_FILL_RECTS` in terrain_cache.rs.
struct TerrainCacheFillRect {
    // Column within one longitude lap, then unbounded row, on the level's lattice.
    first_texel: vec2<i32>,
    texels: vec2<u32>,
    first_slot: vec2<u32>,
    spacing_m: vec2<f32>,
    first_invocation: u32,
    level: u32,
    lap_columns: u32,
    padding: u32,
};

struct TerrainCacheFill {
    rect_count: u32,
    // Zero keeps the production tuning.
    horizontal_scale: f32,
    padding: vec2<u32>,
    rects: array<TerrainCacheFillRect, 12>,
};

@group(1) @binding(0)
var<uniform> terrain_cache_fill: TerrainCacheFill;

@group(1) @binding(1)
var terrain_cache_output: texture_storage_2d_array<rgba32float, write>;

@group(1) @binding(2)
var terrain_cache_jump_output: texture_storage_2d_array<rgba8unorm, write>;

// Cells whose cached octaves change by more than this fraction of a noise's
// amplitude sum when a corner is evaluated with another corner's simplex
// evaluate that noise directly. The tolerance sits just above single-precision
// noise, so only boundaries where the changing vertices carry no kernel weight
// stay cached; that keeps about two thirds of the cells crossing a simplex
// boundary cached, where flagging every crossing cell cost up to 16% more.
const TERRAIN_CACHE_JUMP_TOLERANCE: f32 = 1e-6;

// One invocation per scheduled texel; rects take consecutive invocations. Each
// texel also flags the cell it spans to the next column and row.
@compute @workgroup_size(64)
fn generate_terrain_cache(@builtin(global_invocation_id) invocation_id: vec3<u32>) {
    var rect = terrain_cache_fill.rects[0];
    for (var index = 1u; index < terrain_cache_fill.rect_count; index++) {
        if invocation_id.x >= terrain_cache_fill.rects[index].first_invocation {
            rect = terrain_cache_fill.rects[index];
        }
    }
    let local = invocation_id.x - rect.first_invocation;
    if terrain_cache_fill.rect_count == 0u || local >= rect.texels.x * rect.texels.y {
        return;
    }
    tuning = production_tuning();
    if terrain_cache_fill.horizontal_scale > 0.0 {
        tuning.terrain_tuning.x = terrain_cache_fill.horizontal_scale;
    }
    let offset = vec2<u32>(local % rect.texels.x, local / rect.texels.x);
    let columns = vec2<u32>(
        (u32(rect.first_texel.x) + offset.x) % rect.lap_columns,
        (u32(rect.first_texel.x) + offset.x + 1u) % rect.lap_columns,
    );
    let rows = vec2<i32>(rect.first_texel.y + i32(offset.y)) + vec2<i32>(0, 1);
    let x_m = vec2<f32>(columns) * rect.spacing_m.x;
    let y_m = vec2<f32>(rows) * rect.spacing_m.y;
    let corners = TerrainCacheCell(
        terrain_cache_circle_position(vec2<f32>(x_m.x, y_m.x)),
        terrain_cache_circle_position(vec2<f32>(x_m.y, y_m.x)),
        terrain_cache_circle_position(vec2<f32>(x_m.x, y_m.y)),
        terrain_cache_circle_position(vec2<f32>(x_m.y, y_m.y)),
    );
    let size = textureDimensions(terrain_cache_output).x;
    let slot = (rect.first_slot + offset) % size;
    var jumps = vec4<f32>(0.0);
    for (var field = 0u; field < TERRAIN_BROAD_NOISE_COUNT; field++) {
        let octaves = terrain_cache_octaves(
            field,
            max(rect.spacing_m.x, rect.spacing_m.y),
            tuning.terrain_tuning.x,
        );
        let persistence = TERRAIN_BROAD_NOISE_PERSISTENCE[field];
        var sum = vec4<f32>(0.0);
        var jump = 0.0;
        var amplitude = 1.0;
        var wavelength_m = TERRAIN_BROAD_NOISE_WAVELENGTHS_M[field] * tuning.terrain_tuning.x;
        for (var octave = 0u; octave < octaves; octave++) {
            let seed = scene.match_seed
                ^ TERRAIN_BROAD_NOISE_SALTS[field]
                ^ OCTAVE_SALTS[octave];
            sum += amplitude * terrain_cache_simplex_4d(seed, corners.lower_left, wavelength_m);
            jump = max(jump, amplitude * terrain_cache_simplex_jump_4d(seed, corners, wavelength_m));
            amplitude *= persistence;
            wavelength_m *= 0.5;
        }
        let amplitude_sum = fractal_amplitude_sum(TERRAIN_BROAD_NOISE_OCTAVES[field], persistence);
        jumps[field] = select(0.0, 1.0, jump > TERRAIN_CACHE_JUMP_TOLERANCE * amplitude_sum);
        textureStore(
            terrain_cache_output,
            slot,
            rect.level * TERRAIN_BROAD_NOISE_COUNT + field,
            sum,
        );
    }
    textureStore(terrain_cache_jump_output, slot, rect.level, jumps);
}

struct TerrainCacheCell {
    lower_left: vec4<f32>,
    lower_right: vec4<f32>,
    upper_left: vec4<f32>,
    upper_right: vec4<f32>,
};

fn terrain_cache_circle_position(lattice_position_m: vec2<f32>) -> vec4<f32> {
    return periodic_noise_circle_position_from_canonical(
        canonical_world_position(lattice_position_m),
    );
}

fn terrain_cache_cylinder_point(circle_position: vec4<f32>, wavelength_m: f32) -> vec4<f32> {
    let circle_radius = scene.world_size_m.x / (TAU * wavelength_m);
    let linear_y = circle_position.z / (sqrt(2.0) * wavelength_m);
    return vec4<f32>(
        circle_radius * circle_position.x,
        circle_radius * circle_position.y,
        linear_y,
        linear_y + 0.5,
    );
}

// One circle octave's value, x and y slopes, and cross derivative, matching
// `terrain_simplex_noise_4d_from_circle`. The cross derivative is analytic
// within the simplex, so jumps at simplex boundaries cannot leak into it.
fn terrain_cache_simplex_4d(
    seed: vec2<u32>,
    circle_position: vec4<f32>,
    wavelength_m: f32,
) -> vec4<f32> {
    let unskew_factor = 0.1381966;
    let domain = simplex_noise_domain_4d(
        terrain_cache_cylinder_point(circle_position, wavelength_m),
    );
    var value = 0.0;
    var gradient = vec4<f32>(0.0);
    var cross = vec2<f32>(0.0);
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
            let lattice_gradient = terrain_simplex_lattice_gradient_4d(
                seed,
                domain,
                lattice_offset,
            );
            let ramp = dot(lattice_gradient, offset);
            let attenuation_squared = attenuation * attenuation;
            let attenuation_cubed = attenuation_squared * attenuation;
            value += attenuation_squared * attenuation_squared * ramp;
            gradient += attenuation_squared * attenuation_squared * lattice_gradient
                - 8.0 * attenuation_cubed * ramp * offset;
            // Second derivatives between the circle axes and the two
            // latitude axes, which move together.
            let latitude_offset = offset.z + offset.w;
            let latitude_gradient = lattice_gradient.z + lattice_gradient.w;
            cross += 48.0 * attenuation_squared * latitude_offset * ramp * offset.xy
                - 8.0 * attenuation_cubed
                    * (latitude_gradient * offset.xy + latitude_offset * lattice_gradient.xy);
        }
    }
    let scale = 35.7;
    return scale * vec4<f32>(
        value,
        (-circle_position.y * gradient.x + circle_position.x * gradient.y) / wavelength_m,
        (gradient.z + gradient.w) / (sqrt(2.0) * wavelength_m),
        (-circle_position.y * cross.x + circle_position.x * cross.y)
            / (sqrt(2.0) * wavelength_m * wavelength_m),
    );
}

// The largest change from evaluating a cell corner with another corner's
// simplex, comparing each corner with the lower-left one both ways: the value
// jump where a cell crosses a simplex boundary.
fn terrain_cache_simplex_jump_4d(
    seed: vec2<u32>,
    corners: TerrainCacheCell,
    wavelength_m: f32,
) -> f32 {
    let lower_left = terrain_cache_cylinder_point(corners.lower_left, wavelength_m);
    let lower_right = terrain_cache_cylinder_point(corners.lower_right, wavelength_m);
    let upper_left = terrain_cache_cylinder_point(corners.upper_left, wavelength_m);
    let upper_right = terrain_cache_cylinder_point(corners.upper_right, wavelength_m);
    let reference = simplex_noise_domain_4d(lower_left);
    let jump = max(
        max(
            terrain_cache_simplex_pair_jump(seed, lower_left, reference, lower_right),
            terrain_cache_simplex_pair_jump(seed, lower_left, reference, upper_left),
        ),
        terrain_cache_simplex_pair_jump(seed, lower_left, reference, upper_right),
    );
    return (35.7 / 33.4) * jump;
}

fn terrain_cache_simplex_pair_jump(
    seed: vec2<u32>,
    reference_point: vec4<f32>,
    reference: Simplex4dDomain,
    point: vec4<f32>,
) -> f32 {
    let domain = simplex_noise_domain_4d(point);
    if all(domain.lattice == reference.lattice) && all(domain.rank == reference.rank) {
        return 0.0;
    }
    let at_reference = abs(
        simplex_noise_value_4d_from_domain(
            seed,
            simplex_noise_domain_4d_in(reference_point, domain.lattice, domain.rank),
        ) - simplex_noise_value_4d_from_domain(seed, reference),
    );
    let at_point = abs(
        simplex_noise_value_4d_from_domain(
            seed,
            simplex_noise_domain_4d_in(point, reference.lattice, reference.rank),
        ) - simplex_noise_value_4d_from_domain(seed, domain),
    );
    return max(at_reference, at_point);
}
