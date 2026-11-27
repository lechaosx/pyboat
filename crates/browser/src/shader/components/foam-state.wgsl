// Keep synchronized with crates/browser/src/foam.rs.
const FOAM_CLASSES: u32 = 5u;
const FOAM_SHAPE_POINTS: u32 = 9u;
const FOAM_SHAPE_ROWS: u32 = 14u;
const FOAM_CANDIDATES: u32 = 128u;
const FOAM_NOISE_FIELD: u32 = 35u;
const FOAM_FRESH_OPTICAL_DEPTH: f32 = 0.798;
const FOAM_OPTICAL_DEPTH_EFOLD_S: f32 = 4.07;
const FOAM_BUBBLE_ASYMMETRY: f32 = 0.85;
// Texels of the foam grid's tiles, which the scene skips where they hold no
// foam.
const FOAM_TILE_TEXELS: u32 = 32u;

struct FoamClass {
    // The cell below the view centre's place in the water, wrapped around
    // the world, the cells around the world, then the slot.
    lattice: vec4<i32>,
    // The view centre's place in its cell, the cell's width, then the share
    // drawn as discrete events, the rest joining the unresolved mean.
    place: vec4<f32>,
    // The first cell the frame looks up, from `lattice.xy`, then the cells
    // it looks up across and down.
    window: vec4<i32>,
    // Seconds into the slot, the slot's length, how far an event may drift
    // beyond the view centre's water, then the class's slowest breaking
    // speed.
    timing: vec4<f32>,
    // The class's table, `BinShape::cumulative` in foam.rs, point by point:
    // six integrals over the base-2 logarithm of speed up to each point.
    table: array<vec4<f32>, FOAM_SHAPE_ROWS>,
};

struct FoamUniforms {
    classes: array<FoamClass, FOAM_CLASSES>,
    // The foam grid's first texel's corner from the view centre, its texel
    // width, then the age at which foam has faded.
    grid: vec4<f32>,
    // Texels across and down, then tiles across and down.
    grid_texels: vec4<u32>,
    // The forcing grid's first texel's corner from the view centre, then its
    // spacing and texels across.
    forcing: vec4<f32>,
    // The view centre's orbital displacement, then its water's surface drift.
    water: vec4<f32>,
    // Each light's direction for the short shadows, then their reach in
    // texels; the shadows run only where it reaches half a texel.
    sun: vec4<f32>,
    moon: vec4<f32>,
    // 1 while foam is drawn, 1 while every class and the shadows are forced,
    // the pixels across a texel, then padding.
    flags: vec4<u32>,
    // The hole noise's longest octave in metres, the slope its unresolved
    // octaves give per unit of the filaments' optical depth, the frame's
    // capacity of live events, then the share of a row its segments cover.
    lace: vec4<f32>,
};

// Foam over the water a pixel shows: the share of it the discrete events'
// filaments cover and their optical depth, their relief's gradient and
// unresolved slope, the short shadows they cast on the Sun's and Moon's
// glint, then the sub-pixel foam's covered share, its filaments' mean
// reflectance and squared diffuse transmittance times that share, and
// their unscattered depth.
struct FoamCover {
    covered: f32,
    depth: f32,
    relief_gradient: vec2<f32>,
    unresolved_slope: f32,
    glint_shadow: vec2<f32>,
    mean_cover: f32,
    mean_reflectance: f32,
    mean_diffuse_transmittance_squared: f32,
    mean_depth: f32,
};

fn no_foam_cover() -> FoamCover {
    return FoamCover(0.0, 0.0, vec2<f32>(0.0), 0.0, vec2<f32>(1.0), 0.0, 0.0, 0.0, 0.0);
}

// How foam covering water changes its light: the reflectance of the foam
// itself, then the factors on the water's body, its mirrored sky, and the
// Sun's and Moon's glint. Foam is a grey, non-absorbing layer: two-stream
// reflectance τ*/(1 + τ*) and diffuse transmittance 1/(1 + τ*), which the
// body's light crosses twice, and unscattered transmittance
// exp(-τ (1/μ_in + 1/μ_out)) for the specular light, of extinction depth
// τ = τ*/(1 - g) (Coakley and Chýlek 1975); the holes between its
// filaments pass the water's light untouched. Compromises: the layer lies
// level, and direct light takes the diffuse reflectance rather than its own
// incidence's.
struct FoamLight {
    reflectance: f32,
    body: f32,
    sky: f32,
    glint: vec2<f32>,
};

fn foam_light(cover: FoamCover, view_z: f32, sun_z: f32, moon_z: f32) -> FoamLight {
    let air_masses = vec3<f32>(
        2.0 / max(view_z, 1e-3),
        1.0 / max(sun_z, 1e-3) + 1.0 / max(view_z, 1e-3),
        1.0 / max(moon_z, 1e-3) + 1.0 / max(view_z, 1e-3),
    ) / (1.0 - FOAM_BUBBLE_ASYMMETRY);
    let transmitted = 1.0 / (1.0 + cover.depth);
    let bare = 1.0 - cover.covered;
    let reflectance = cover.covered * cover.depth * transmitted;
    let body = bare + cover.covered * transmitted * transmitted;
    let specular = bare + cover.covered * exp(-cover.depth * air_masses);
    // Sub-pixel foam covers its share of the pixel beside the discrete
    // events, without overlapping them, and casts no resolved shadow.
    let cover_share = cover.mean_cover;
    let mean_specular = exp(-cover.mean_depth * air_masses);
    return FoamLight(
        reflectance * (1.0 - cover_share) + cover.mean_reflectance,
        body * (1.0 - cover_share) + cover.mean_diffuse_transmittance_squared,
        mix(specular.x, mean_specular.x, cover_share),
        mix(specular.yz * cover.glint_shadow, mean_specular.yz, cover_share),
    );
}

fn no_foam_light() -> FoamLight {
    return FoamLight(0.0, 1.0, 1.0, vec2<f32>(1.0));
}

// The share of a light's glint the foam passes, as `foam_light` gives it.
fn foam_glint_transmittance(cover: FoamCover, view_z: f32, light_z: f32) -> f32 {
    let air_masses = (1.0 / max(light_z, 1e-3) + 1.0 / max(view_z, 1e-3))
        / (1.0 - FOAM_BUBBLE_ASYMMETRY);
    let specular = 1.0 - cover.covered + cover.covered * exp(-cover.depth * air_masses);
    return mix(specular, exp(-cover.mean_depth * air_masses), cover.mean_cover);
}
