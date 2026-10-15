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

// One invocation per scheduled texel; rects take consecutive invocations.
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
    let column = (u32(rect.first_texel.x) + offset.x) % rect.lap_columns;
    let row = rect.first_texel.y + i32(offset.y);
    let circle_position = periodic_noise_circle_position_from_canonical(
        canonical_world_position(vec2<f32>(f32(column), f32(row)) * rect.spacing_m),
    );
    let size = textureDimensions(terrain_cache_output).x;
    let slot = (rect.first_slot + offset) % size;
    for (var field = 0u; field < TERRAIN_BROAD_NOISE_COUNT; field++) {
        let octaves = terrain_cache_octaves(
            field,
            max(rect.spacing_m.x, rect.spacing_m.y),
            tuning.terrain_tuning.x,
        );
        let persistence = TERRAIN_BROAD_NOISE_PERSISTENCE[field];
        var sum = vec4<f32>(0.0);
        var amplitude = 1.0;
        var wavelength_m = TERRAIN_BROAD_NOISE_WAVELENGTHS_M[field] * tuning.terrain_tuning.x;
        for (var octave = 0u; octave < octaves; octave++) {
            let key = noise_key(TERRAIN_BROAD_NOISE_FIELDS[field], octave);
            sum += amplitude * terrain_cache_simplex_4d(key, circle_position, wavelength_m);
            amplitude *= persistence;
            wavelength_m *= 0.5;
        }
        textureStore(
            terrain_cache_output,
            slot,
            rect.level * TERRAIN_BROAD_NOISE_COUNT + field,
            sum,
        );
    }
}

// One circle octave's value, x and y slopes, and cross derivative, matching
// `terrain_simplex_noise_4d_from_circle`.
fn terrain_cache_simplex_4d(
    key: u32,
    circle_position: vec4<f32>,
    wavelength_m: f32,
) -> vec4<f32> {
    let unskew_factor = 0.1381966;
    let circle_radius = scene.world_size_m.x / (TAU * wavelength_m);
    let linear_y = circle_position.z / (sqrt(2.0) * wavelength_m);
    let domain = simplex_noise_domain_4d(vec4<f32>(
        circle_radius * circle_position.x,
        circle_radius * circle_position.y,
        linear_y,
        linear_y + 0.5,
    ));
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
        let attenuation = 0.5 - dot(offset, offset);
        if attenuation > 0.0 {
            let lattice_gradient = terrain_simplex_lattice_gradient_4d(
                key,
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
    // Keep synchronized with `terrain_simplex_noise_4d_from_circle`.
    let scale = 114.3;
    return scale * vec4<f32>(
        value,
        (-circle_position.y * gradient.x + circle_position.x * gradient.y) / wavelength_m,
        (gradient.z + gradient.w) / (sqrt(2.0) * wavelength_m),
        (-circle_position.y * cross.x + circle_position.x * cross.y)
            / (sqrt(2.0) * wavelength_m * wavelength_m),
    );
}
