@group(0) @binding(14)
var foam_grid: texture_2d<f32>;
@group(0) @binding(15)
var foam_tiles: texture_2d<u32>;
@group(0) @binding(16)
var foam_shadows: texture_2d<f32>;
@group(0) @binding(17)
var foam_forcing: texture_2d_array<f32>;
@group(0) @binding(18)
var foam_sampler: sampler;
@group(0) @binding(19)
var<uniform> foam: FoamUniforms;

// The foam over water `offset_m` from the view centre, its optical depths
// and cover scaled by `thinning`. A pixel reads the grid's texel under it
// and, for the sub-pixel foam, the forcing grid between its texels.
// Compromise: the low tier's half-resolution grid is read unfiltered, so its
// foam shows two-pixel texels.
fn foam_cover(offset_m: vec2<f32>, thinning: f32) -> FoamCover {
    var cover = no_foam_cover();
    // The forced evaluation takes every foam branch through the frame's
    // flag rather than `EVALUATE_ALL_BRANCHES`, so it times the shipped
    // shader's branches as compiled.
    let forced = foam.flags.y != 0u;
    if foam.flags.x == 0u {
        return cover;
    }
    let forcing_uv = (offset_m - foam.forcing.xy) / (foam.forcing.z * foam.forcing.w);
    let mean = textureSampleLevel(foam_forcing, foam_sampler, forcing_uv, 1, 0.0);
    cover.mean_cover = mean.x * thinning;
    cover.mean_reflectance = mean.y * thinning;
    cover.mean_diffuse_transmittance_squared = mean.z * thinning;
    cover.mean_depth = mean.w;
    let texel = vec2<i32>(floor((offset_m - foam.grid.xy) / foam.grid.z));
    let inside = all(texel >= vec2<i32>(0)) && all(texel < vec2<i32>(foam.grid_texels.xy));
    var tile = 0u;
    if inside {
        tile = textureLoad(foam_tiles, texel / i32(FOAM_TILE_TEXELS), 0).x;
    }
    let foamy = (tile & 1u) != 0u;
    if foamy || forced {
        let grid = select(vec4<f32>(0.0), textureLoad(foam_grid, texel, 0), foamy);
        cover.covered = grid.x * thinning;
        cover.depth = grid.y;
        cover.relief_gradient = grid.zw * thinning;
        // Holes too fine for the grid tilt the filaments' edges.
        cover.unresolved_slope = foam.lace.y * cover.depth * sqrt(grid.x * (1.0 - grid.x));
    }
    let shadowed = (tile & 2u) != 0u;
    if shadowed || forced {
        cover.glint_shadow = select(
            vec2<f32>(1.0),
            textureLoad(foam_shadows, texel, 0).xy,
            shadowed,
        );
    }
    cover.covered = select(cover.covered, max(cover.covered, 1e-6), forced);
    return cover;
}
