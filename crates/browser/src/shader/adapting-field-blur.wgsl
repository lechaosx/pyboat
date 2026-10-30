@group(1) @binding(0)
var illumination: texture_2d<f32>;
// The field as the previous frame left it.
@group(1) @binding(1)
var previous_field: texture_2d<f32>;

// Time constants of the eye's adaptation, which eases in stops so equal
// brightness ratios take equal time. Darkness takes longer to adapt to than
// bright light. Gameplay time constants make adaptation perceptible within
// seconds.
const DARKENING_ADAPTATION_SECONDS: f32 = 3.0;
const BRIGHTENING_ADAPTATION_SECONDS: f32 = 1.0;
// Time constant of chromatic adaptation, with which the adapting white eases
// toward the field's colour: 26 s completes 90% of it in about 60 s, as
// Fairchild and Reniff (1995, J. Opt. Soc. Am. A 12) measured at constant
// luminance. A compromise: one exponential stands in for their sum of two,
// whose fast phase of about a second completes much of the change at once.
const CHROMATIC_ADAPTATION_SECONDS: f32 = 26.0;

@vertex
fn fullscreen_vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

// Stops of the view's mean illumination over the adapting field around each
// texel, and that mean's colour, the adapting white: an arithmetic mean of
// light, as CIE 159:2004 defines the adapting luminance as the field's mean
// luminance, and as light scattered within the eye adds. CIE gives the
// field's edge only approximately, so a Gaussian with the variance of its
// disc pools the light, renormalized over the part of the field the view
// covers. Adaptation is retinal, so each texel eases toward that target where
// it lies on the screen, wherever the view has moved.
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
    var weighted_light = vec3<f32>(0.0);
    var total_weight = 0.0;
    for (var row = 0u; row < ILLUMINATION_TEXELS; row++) {
        let offset = uv.y - (f32(row) + 0.5) / f32(ILLUMINATION_TEXELS);
        let row_weight = exp(exponent_scale * offset * offset);
        var row_light = vec3<f32>(0.0);
        var row_total = 0.0;
        for (var column = 0u; column < ILLUMINATION_TEXELS; column++) {
            let light = textureLoad(illumination, vec2<u32>(column, row), 0);
            row_light += column_weights[column] * exp2(light.r) * light.gba;
            row_total += column_weights[column];
        }
        weighted_light += row_weight * row_light;
        total_weight += row_weight * row_total;
    }
    let target_light = adapting_light(weighted_light / total_weight);
    let previous = textureLoad(previous_field, vec2<u32>(position.xy), 0);
    let time_constant = select(
        BRIGHTENING_ADAPTATION_SECONDS,
        DARKENING_ADAPTATION_SECONDS,
        target_light.r < previous.r,
    );
    let seconds = scene.exposure_and_output.x;
    let weight = vec4<f32>(
        1.0 - exp(-seconds / time_constant),
        vec3<f32>(1.0 - exp(-seconds / CHROMATIC_ADAPTATION_SECONDS)),
    );
    return previous + (target_light - previous) * weight;
}
