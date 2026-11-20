@group(0) @binding(11)
var wave_tile_texture: texture_2d_array<f32>;
@group(0) @binding(12)
var wave_tile_sampler: sampler;
@group(0) @binding(13)
var<uniform> wave_tiles: WaveTileUniforms;

// A tile at the lattice vertex `vertex`, shifted by the vertex's seeded
// offset, at `position` in tile lengths.
fn wave_tile_texel(
    layer: u32,
    position: vec2<f32>,
    vertex: vec2<i32>,
    period: i32,
    key: u32,
    texture_scale: f32,
) -> vec2<f32> {
    let hashed = hash32(
        key ^ (u32(positive_mod(vertex.x, period)) * LATTICE_KEY_MULTIPLIERS[0])
            ^ (u32(vertex.y) * LATTICE_KEY_MULTIPLIERS[1]),
    );
    let offset = vec2<f32>(f32(hashed & 0xffffu), f32(hashed >> 16u)) / 65536.0;
    return textureSampleLevel(
        wave_tile_texture,
        wave_tile_sampler,
        (position + offset) * texture_scale,
        layer,
        0.0,
    ).xy;
}

// The slope of a layer's two bands at `offset_m` from the view centre, each
// band of unit slope variance less what the footprint filters. Heitz and
// Neyret's (2018, Proc. ACM Comput. Graph. Interact. Tech. 1) variance-
// preserving blend of three copies of the tile at seeded offsets, on a
// triangle lattice of half the tile's spacing, hides the tile's period; a
// blend of copies of a Gaussian field is a Gaussian field of its spectrum.
// A tile longer than twice the view's diagonal never shows a wave twice, so
// it is read once, and the blend takes over, by `blend.x`, as the tile
// shortens to one diagonal.
fn wave_tile_slope(layer: u32, offset_m: vec2<f32>) -> vec2<f32> {
    let tiles = wave_tiles.layers[layer];
    let lattice = 2.0 * offset_m / tiles.tile.x;
    let skewed = vec2<f32>(lattice.x - 0.57735027 * lattice.y, 1.1547005 * lattice.y)
        + tiles.view.zw;
    let cell = floor(skewed);
    let corner = skewed - cell;
    let lower = corner.x + corner.y < 1.0;
    let weights = select(
        vec3<f32>(corner.x + corner.y - 1.0, 1.0 - corner.y, 1.0 - corner.x),
        vec3<f32>(1.0 - corner.x - corner.y, corner.y, corner.x),
        lower,
    );
    let base = vec2<i32>(cell) + tiles.lattice.xy;
    let position = offset_m / tiles.tile.x + tiles.view.xy;
    let key = noise_key(DECORATIVE_WATER_NOISE_FIELD, WAVE_TILE_LAYERS + layer);
    let period = tiles.lattice.z;
    let texture_scale = tiles.tile.z;
    let tiling = tiles.blend.x;
    let forced_blend = EVALUATE_ALL_BRANCHES && tiles.blend.y > 0.0;
    var blended = vec2<f32>(0.0);
    if tiling < 1.0 || forced_blend {
        blended = (1.0 - tiling) * textureSampleLevel(
            wave_tile_texture,
            wave_tile_sampler,
            position * texture_scale,
            layer,
            0.0,
        ).xy;
    }
    if tiling <= 0.0 && !forced_blend {
        return blended;
    }
    let slope = weights.x * wave_tile_texel(
            layer,
            position,
            base + select(vec2<i32>(1), vec2<i32>(0), lower),
            period,
            key,
            texture_scale,
        )
        + weights.y * wave_tile_texel(
            layer,
            position,
            base + select(vec2<i32>(1, 0), vec2<i32>(0, 1), lower),
            period,
            key,
            texture_scale,
        )
        + weights.z * wave_tile_texel(
            layer,
            position,
            base + select(vec2<i32>(0, 1), vec2<i32>(1, 0), lower),
            period,
            key,
            texture_scale,
        );
    blended += tiling * slope * inverseSqrt(dot(weights, weights));
    return blended * inverseSqrt((1.0 - tiling) * (1.0 - tiling) + tiling * tiling);
}

fn wave_tile_octaves() -> vec4<f32> {
    return wave_tiles.octaves;
}

fn wave_tile_resolved_octaves(layer: u32) -> vec2<f32> {
    return wave_tiles.layers[layer].tile.yw;
}

fn wave_tile_wind_direction() -> vec2<f32> {
    return wave_tiles.sea.xy;
}
