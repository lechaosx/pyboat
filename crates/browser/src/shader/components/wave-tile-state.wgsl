// Keep synchronized with crates/browser/src/wave_tiles.rs.
const WAVE_TILE_LAYERS: u32 = 5u;
const WAVE_TILE_TEXELS: u32 = 512u;
// Tile wavenumbers from -WAVE_TILE_REACH to WAVE_TILE_REACH along each axis,
// in units of 2π over the tile, hold every band a layer carries.
const WAVE_TILE_REACH: i32 = 46;

struct WaveTileLayer {
    // The view centre's place in the tile, in tile lengths, then in the
    // random tiling's skewed lattice, past `lattice.xy`.
    view: vec4<f32>,
    // The lattice vertex below the view centre, the lattice's vertices
    // around the world, then the texels the tile spans.
    lattice: vec4<i32>,
    // The tile's length, the octaves of its two bands' slope variance the
    // footprint leaves along the view centre's wind, the texture's
    // coordinates per tile length, then the octaves the footprint leaves
    // across the wind.
    tile: vec4<f32>,
    // The share of the slope the random tiling blends, then 1 on the layer
    // whose single and blended reads the forced evaluation both takes.
    blend: vec4<f32>,
};

struct WaveTileUniforms {
    layers: array<WaveTileLayer, WAVE_TILE_LAYERS>,
    // Seconds into the clock's loop, the loop's period, the sea-level pixel
    // footprint, then padding.
    clock: vec4<f32>,
    // The wind's direction at the view centre, then the angular frequency of
    // the spectrum's peak, then padding.
    sea: vec4<f32>,
    // The octaves of slope variance no drawn layer resolves: the shares of
    // the layers' octaves the footprint filters away and those of the waves
    // shorter than the layers down to the shortest gravity wave. Then the
    // octaves the drawn layers resolve, the number of drawn layers, and the
    // most layers the graphics tier draws.
    octaves: vec4<f32>,
};
