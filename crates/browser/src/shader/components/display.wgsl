fn linear_material_color(display_color: vec3<f32>) -> vec3<f32> {
    // Material palettes are authored in sRGB; light multiplies their linear reflectance.
    return select(
        pow((max(display_color, vec3<f32>(0.0)) + 0.055) / 1.055, vec3<f32>(2.4)),
        display_color / 12.92,
        display_color <= vec3<f32>(0.04045),
    );
}

// Clear-sky daylight from a zenith Sun on level ground, the unit of scene
// light, in lux; game::ZENITH_DAYLIGHT_LUX gives its provenance.
const ZENITH_DAYLIGHT_LUX: f32 = 1.173e5;
// Light of a clear moonless sky, in scene light; game::NIGHT_SKY_LIGHT gives
// its provenance.
const NIGHT_SKY_LIGHT: f32 = 7.3e-4 / ZENITH_DAYLIGHT_LUX;
// Mirrors ADAPTED_BRIGHTNESS_EXPONENT in exposure.rs, which gives its
// provenance and compromise.
const ADAPTED_BRIGHTNESS_EXPONENT: f32 = 1.0 / 12.0;

// Gain that shows a part of the view adapted to `light`, in scene light:
// exposure.rs's `exposure_target`.
fn adapted_gain(light: f32) -> f32 {
    return pow(clamp(light, NIGHT_SKY_LIGHT, 1.0), ADAPTED_BRIGHTNESS_EXPONENT - 1.0);
}

// Light, in scene light, that a view shown at `adapted_gain` is adapted to:
// the inverse of exposure.rs's `exposure_target`.
fn adapted_view_light(adapted_gain: f32) -> f32 {
    return pow(max(adapted_gain, 1.0), 1.0 / (ADAPTED_BRIGHTNESS_EXPONENT - 1.0));
}

// The eye's state where it is adapted to a part of the view: the gain that
// shows the light it is adapted to, and that light's colour at unit
// luminance, the adapting white.
struct EyeAdaptation {
    gain: f32,
    white: vec3<f32>,
};

// Adapting luminance, in cd/m², of a view shown at `adapted_gain`: its adapted
// light on a 20% grey, CIECAM02's background, taken as photopic luminance.
fn adapting_luminance(adapted_gain: f32) -> f32 {
    return 0.2 * adapted_view_light(adapted_gain) * ZENITH_DAYLIGHT_LUX / PI;
}

// Share of vision that rods carry: one minus the photopic weight of CIE
// 191:2010's mesopic model, 0.7670 + 0.3334 log10 L between 0.005 and 5
// cd/m², for the adapting luminance L without the model's iteration.
fn rod_vision_fraction(adapted_gain: f32) -> f32 {
    return 1.0 - clamp(
        0.7670 + 0.3334 * log2(adapting_luminance(adapted_gain)) / log2(10.0),
        0.0,
        1.0,
    );
}

// Rods' achromatic signal from `stimulus` mixed into `cone_color` by the
// rods' share of vision. Jacobs et al. (2015),
// https://www.emilyacooper.org/pubs/2015Jacobs_TOG.pdf: normalized scotopic
// RGB weights preserve neutral reflectance while rods favor blue-green over
// red. Rods have one pigment, so the cones' chromatic adaptation does not
// reach their signal.
fn night_vision_color(cone_color: vec3<f32>, stimulus: vec3<f32>, rod_fraction: f32) -> vec3<f32> {
    let rod_signal = dot(stimulus, vec3<f32>(0.0, 0.8451, 0.1459) / 0.991);
    return mix(cone_color, vec3<f32>(rod_signal), rod_fraction);
}

// Linear sRGB to CAT16's cone space (Li et al. 2017, Color Res. Appl. 42):
// its M16 after the sRGB primaries' XYZ (IEC 61966-2-1), and back. CAT16 is
// the chromatic adaptation of CIECAM16 (CIE 248:2022), the CIE's successor to
// CIECAM02, which keeps CIECAM02's luminance adaptation and so the brightness
// law of ADAPTED_BRIGHTNESS_EXPONENT.
const SRGB_TO_CAT16: mat3x3<f32> = mat3x3<f32>(
    vec3<f32>(0.302725, 0.153733, 0.027945),
    vec3<f32>(0.602370, 0.777367, 0.147880),
    vec3<f32>(0.070461, 0.085370, 0.909106),
);
const CAT16_TO_SRGB: mat3x3<f32> = mat3x3<f32>(
    vec3<f32>(5.446455, -1.077967, 0.007928),
    vec3<f32>(-4.215377, 2.144103, -0.219193),
    vec3<f32>(-0.026289, -0.117793, 1.119950),
);

// CIECAM16's degree of adaptation, F [1 - e^((-L_A - 42)/92) / 3.6], for an
// average surround's F of 1 and the adapting luminance L_A. A compromise:
// the formula is fitted to photopic vision and is kept below it, where it
// settles at 0.82, while the rods' achromatic signal takes over the view's
// colour.
fn chromatic_adaptation_degree(adapted_gain: f32) -> f32 {
    return 1.0 - exp((-adapting_luminance(adapted_gain) - 42.0) / 92.0) / 3.6;
}

// Colour that shows `linear_color` on the display as the cones of an eye in
// `adaptation` see it: CAT16's von Kries gains carry the adapting white to
// the display's D65 white by the degree of adaptation and leave the rest of
// the light's colour. A neutral surface lit by the adapting white keeps its
// luminance and shows neutral when adaptation is complete.
fn chromatically_adapted_color(linear_color: vec3<f32>, adaptation: EyeAdaptation) -> vec3<f32> {
    let degree = chromatic_adaptation_degree(adaptation.gain);
    let gains = degree * (SRGB_TO_CAT16 * vec3<f32>(1.0)) / (SRGB_TO_CAT16 * adaptation.white)
        + 1.0 - degree;
    return CAT16_TO_SRGB * (gains * (SRGB_TO_CAT16 * linear_color));
}

// Unlit colours, such as boats and the windsock, as level ground lit by the
// light the view is adapted to there.
fn adapted_unlit_color(linear_color: vec3<f32>, adaptation: EyeAdaptation) -> vec3<f32> {
    return linear_color * adaptation.white * adapted_view_light(adaptation.gain);
}

fn vision_noise_sample(cell: vec2<u32>, phase: u32) -> f32 {
    var key = cell.x * 0x9e3779b9u ^ cell.y * 0x85ebca6bu ^ phase * 0xc2b2ae35u;
    key = (key ^ (key >> 16u)) * 0x7feb352du;
    key = (key ^ (key >> 15u)) * 0x846ca68bu;
    key = key ^ (key >> 16u);
    return f32(key >> 8u) / 16777215.0 * 2.0 - 1.0;
}

fn vision_noise_field(cell: vec2<u32>, blend: vec2<f32>, phase: u32) -> f32 {
    return mix(
        mix(vision_noise_sample(cell, phase), vision_noise_sample(cell + vec2<u32>(1u, 0u), phase), blend.x),
        mix(vision_noise_sample(cell + vec2<u32>(0u, 1u), phase), vision_noise_sample(cell + vec2<u32>(1u), phase), blend.x),
        blend.y,
    );
}

fn adapted_view_color(linear_color: vec3<f32>, adaptation: EyeAdaptation, css_position_px: vec2<f32>, noise_phase: f32) -> vec3<f32> {
    let cone_color = chromatically_adapted_color(linear_color, adaptation);
    let rod_fraction = rod_vision_fraction(adaptation.gain);
    if rod_fraction <= 0.0 {
        return cone_color;
    }
    let vision_color = night_vision_color(cone_color, linear_color, rod_fraction);
    let signal = dot(vision_color, vec3<f32>(0.2126, 0.7152, 0.0722));
    // Relative uncertainty grows as photon signal falls. Multiplicative noise
    // preserves black and has zero mean; the cap keeps faint hazards readable.
    // 1.43e-6 sets the noise at unit scene light in place of a photon count:
    // 1.07% on white under a clear moonless sky's light.
    let amplitude = rod_fraction * min(0.12, 1.43e-6 / sqrt(max(signal, 1e-20)));
    // CSS-pixel cells keep the grain's on-screen size the same at every graphics setting.
    let grid = css_position_px / 4.0;
    let cell = vec2<u32>(floor(grid));
    let blend = smoothstep(vec2<f32>(0.0), vec2<f32>(1.0), fract(grid));
    let phase = noise_phase % 4096.0;
    let first_phase = u32(floor(phase));
    let temporal_blend = smoothstep(0.0, 1.0, fract(phase));
    let noise = mix(
        vision_noise_field(cell, blend, first_phase),
        vision_noise_field(cell, blend, (first_phase + 1u) % 4096u),
        temporal_blend,
    );
    return vision_color * (1.0 + amplitude * noise);
}

fn encode_display_color(linear_color: vec3<f32>) -> vec3<f32> {
    if scene.exposure_and_output.z <= 0.0 {
        return linear_color;
    }
    // Floating-point WebGPU canvases expect extended sRGB encoding; SDR
    // attachments perform this conversion through their -srgb texture view.
    return select(
        1.055 * pow(max(linear_color, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055,
        12.92 * linear_color,
        linear_color <= vec3<f32>(0.0031308),
    );
}

// Exposed light up to this share of diffuse white shows unchanged, so the eye
// model's brightness holds for all but the brightest surfaces; above it an
// exponential shoulder of matching slope approaches the output's peak. A
// presentation choice of the display layer.
const TONE_MAP_SHOULDER: f32 = 0.8;

fn tone_map_scene_color(scene_color: vec3<f32>, adaptation: EyeAdaptation, css_position_px: vec2<f32>) -> vec3<f32> {
    let exposed = max(
        adapted_view_color(scene_color, adaptation, css_position_px, scene.exposure_and_output.w)
            * adaptation.gain,
        vec3<f32>(0.0),
    );
    let maximum = max(exposed.r, max(exposed.g, exposed.b));
    if maximum <= TONE_MAP_SHOULDER {
        return exposed;
    }
    let range = scene.exposure_and_output.y - TONE_MAP_SHOULDER;
    let compressed = TONE_MAP_SHOULDER
        + range * (1.0 - exp(-(maximum - TONE_MAP_SHOULDER) / range));
    // One scale for all channels preserves the hue of bright material colors.
    return exposed * (compressed / maximum);
}
