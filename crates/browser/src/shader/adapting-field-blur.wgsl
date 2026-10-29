@group(1) @binding(0)
var illumination: texture_2d<f32>;

@vertex
fn fullscreen_vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

// Mean stops of the view's illumination over the adapting field around each
// texel: a geometric mean, as the eye responds to ratios of light. CIE gives
// the field's edge only approximately, so a Gaussian with the variance of its
// disc pools the light, renormalized over the part of the field the view
// covers.
@fragment
fn blur_adapting_field(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let aspect = scene.camera_and_view_m.z / scene.camera_and_view_m.w;
    let uv = position.xy / f32(ADAPTING_FIELD_TEXELS);
    // A uniform disc of radius R has variance R²/4 along each axis.
    let sigma_view_heights = 0.5 * ADAPTING_FIELD_RADIUS_DEGREES / VIEW_HEIGHT_DEGREES;
    let exponent_scale = -0.5 / (sigma_view_heights * sigma_view_heights);
    var column_weights: array<f32, ILLUMINATION_TEXELS>;
    for (var column = 0u; column < ILLUMINATION_TEXELS; column++) {
        let offset = aspect * (uv.x - (f32(column) + 0.5) / f32(ILLUMINATION_TEXELS));
        column_weights[column] = exp(exponent_scale * offset * offset);
    }
    var weighted_stops = 0.0;
    var total_weight = 0.0;
    for (var row = 0u; row < ILLUMINATION_TEXELS; row++) {
        let offset = uv.y - (f32(row) + 0.5) / f32(ILLUMINATION_TEXELS);
        let row_weight = exp(exponent_scale * offset * offset);
        var row_stops = 0.0;
        var row_total = 0.0;
        for (var column = 0u; column < ILLUMINATION_TEXELS; column++) {
            row_stops += column_weights[column] * textureLoad(illumination, vec2<u32>(column, row), 0).r;
            row_total += column_weights[column];
        }
        weighted_stops += row_weight * row_stops;
        total_weight += row_weight * row_total;
    }
    return vec4<f32>(weighted_stops / total_weight, 0.0, 0.0, 1.0);
}
