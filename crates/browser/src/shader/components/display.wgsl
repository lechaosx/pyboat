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
const SCENE_LUMINANCE_CD_M2: f32 = ZENITH_DAYLIGHT_LUX / PI;
const DAYLIGHT_BACKGROUND_CD_M2: f32 = 0.2 * SCENE_LUMINANCE_CD_M2;
const EYE_RESPONSE_EXPONENT: f32 = 0.73;
// A photopic-equivalent RGB estimate, not a recovery of scotopic spectra:
// Jacobs et al. (2015), Appendix A, normalized to the neutral reference.
const ROD_RGB_WEIGHTS: vec3<f32> = vec3<f32>(0.0, 0.8451, 0.1459) / 0.991;
override VISION_INTERMEDIATE: bool = false;

struct EyeAdaptation {
    white: vec3<f32>,
    // Neural rod/cone backgrounds (cd/m²), available rod/cone pigment.
    state: vec4<f32>,
};

fn settled_eye_state(light: f32, white: vec3<f32>) -> vec4<f32> {
    let cone = max(light, NIGHT_SKY_LIGHT) * DAYLIGHT_BACKGROUND_CD_M2;
    let rod = cone * dot(white, ROD_RGB_WEIGHTS);
    return vec4<f32>(rod, cone, 0.04 / (0.04 + rod), 2e6 / (2e6 + cone));
}

fn steady_adaptation(light: f32, white: vec3<f32>) -> EyeAdaptation {
    return EyeAdaptation(white, settled_eye_state(light, white));
}

// Pattanaik et al. (2000), Eq.2–6, https://www.cs.ucf.edu/~sumant/publications/sig00.pdf.
fn eye_sigmas(background: vec2<f32>) -> vec2<f32> {
    let rod = background.x;
    let cone = background.y;
    let j = 1.0 / (5e5 * rod + 1.0);
    let k = 1.0 / (5.0 * cone + 1.0);
    var rod_sigma = 2.5874 / 19000.0;
    if rod > 0.0 {
        rod_sigma = 2.5874 * rod / (19000.0 * j*j * rod
            + 0.2615 * pow(1.0 - j*j, 4.0) * pow(rod, 1.0/6.0));
    }
    let cone_sigma = 12.9223 * cone / (pow(k, 4.0) * cone
        + 0.171 * pow(1.0 - pow(k, 4.0), 2.0) * pow(cone, 1.0/3.0));
    return vec2<f32>(rod_sigma, cone_sigma);
}

fn eye_response(luminance: vec2<f32>, sigma: vec2<f32>, pigment: vec2<f32>) -> vec2<f32> {
    let signal = pow(max(luminance, vec2<f32>(0.0)), vec2<f32>(EYE_RESPONSE_EXPONENT));
    return pigment * signal / (signal + pow(sigma, vec2<f32>(EYE_RESPONSE_EXPONENT)));
}

fn daylight_response(luminance: f32) -> f32 {
    let state = settled_eye_state(1.0, vec3<f32>(1.0));
    return dot(eye_response(vec2<f32>(luminance), eye_sigmas(state.xy), state.zw), vec2<f32>(1.0));
}

fn daylight_inverse(response: f32) -> f32 {
    let state = settled_eye_state(1.0, vec3<f32>(1.0));
    let sigma = pow(eye_sigmas(state.xy), vec2<f32>(EYE_RESPONSE_EXPONENT));
    let a = state.z + state.w - response;
    let b = state.z * sigma.y + state.w * sigma.x - response * (sigma.x + sigma.y);
    let c = response * sigma.x * sigma.y;
    let root = sqrt(b*b + 4.0*a*c);
    var x = (root - b) / (2.0*a);
    if b >= 0.0 {
        x = 2.0*c / (b + root);
    }
    return pow(max(x, 0.0), 1.0/EYE_RESPONSE_EXPONENT);
}

fn adapted_gain(light: f32) -> f32 {
    let state = settled_eye_state(clamp(light, NIGHT_SKY_LIGHT, 1.0), vec3<f32>(1.0));
    let response = dot(eye_response(state.xy, eye_sigmas(state.xy), state.zw), vec2<f32>(1.0));
    return daylight_inverse(response) / state.y;
}

// Linear sRGB to CAT16's cone space (Li et al. 2017, Color Res. Appl. 42):
// its M16 after the sRGB primaries' XYZ (IEC 61966-2-1), and back. CAT16 is
// the chromatic adaptation of CIECAM16 (CIE 248:2022).
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
fn chromatic_adaptation_degree(adaptation: EyeAdaptation) -> f32 {
    return 1.0 - exp((-adaptation.state.y - 42.0) / 92.0) / 3.6;
}

// Colour that shows `linear_color` on the display as the cones of an eye in
// `adaptation` see it: CAT16's von Kries gains carry the adapting white to
// the display's D65 white by the degree of adaptation and leave the rest of
// the light's colour. A neutral surface lit by the adapting white keeps its
// luminance and shows neutral when adaptation is complete.
fn chromatically_adapted_color(linear_color: vec3<f32>, adaptation: EyeAdaptation) -> vec3<f32> {
    let degree = chromatic_adaptation_degree(adaptation);
    let gains = degree * (SRGB_TO_CAT16 * vec3<f32>(1.0)) / (SRGB_TO_CAT16 * adaptation.white)
        + 1.0 - degree;
    return CAT16_TO_SRGB * (gains * (SRGB_TO_CAT16 * linear_color));
}

fn adapted_unlit_color(linear_color: vec3<f32>, adaptation: EyeAdaptation) -> vec3<f32> {
    return linear_color * adaptation.white * (adaptation.state.y / DAYLIGHT_BACKGROUND_CD_M2);
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

fn eye_color_response(linear_color: vec3<f32>, adaptation: EyeAdaptation) -> vec4<f32> {
    let luminance = dot(linear_color, vec3<f32>(0.2126, 0.7152, 0.0722));
    if luminance <= 0.0 {
        return vec4<f32>(0.0);
    }
    let stimulus = SCENE_LUMINANCE_CD_M2 * vec2<f32>(dot(linear_color, ROD_RGB_WEIGHTS), luminance);
    let sigma = eye_sigmas(adaptation.state.xy);
    let response = eye_response(stimulus, sigma, adaptation.state.zw);
    // Eq.8 and §4.3: source black/white responses retain the current neural
    // and pigment states. The virtual display uses zero black and the
    // calibrated daylight observer's white, rather than an office CRT.
    let reference_white = dot(eye_response(5.0 * adaptation.state.xy, sigma, adaptation.state.zw), vec2<f32>(1.0));
    let reference_black = dot(eye_response(5.0 / 32.0 * adaptation.state.xy, sigma, adaptation.state.zw), vec2<f32>(1.0));
    let display_white = daylight_response(SCENE_LUMINANCE_CD_M2);
    var mapped_response = response.x + response.y;
    if reference_white > display_white {
        let span = reference_white - reference_black;
        if span > display_white {
            mapped_response = (mapped_response - reference_black) * display_white / span;
        } else {
            mapped_response += display_white - reference_white;
        }
    }
    if mapped_response <= 0.0 {
        return vec4<f32>(0.0);
    }
    // Bound the inverse before its asymptote: the shared intermediate uses
    // half-float range independently of output peak; direct inspection stops
    // where the shoulder is within one float mantissa bit of its peak.
    let raw_cap = select(TONE_MAP_SHOULDER
        + (scene.exposure_and_output.y - TONE_MAP_SHOULDER) * 24.0 * log(2.0),
        65504.0, VISION_INTERMEDIATE);
    let display_luminance = daylight_inverse(min(mapped_response,
        daylight_response(raw_cap * SCENE_LUMINANCE_CD_M2)));
    let reference_state = settled_eye_state(1.0, vec3<f32>(1.0));
    // Above the virtual observer's diffuse white, hold its color derivative
    // at that reference endpoint. The finite display gamut cannot reproduce
    // the inverse model's unbounded chroma near response saturation.
    let display_cone = eye_response(vec2<f32>(min(display_luminance, SCENE_LUMINANCE_CD_M2)), eye_sigmas(reference_state.xy), vec2<f32>(1.0)).y;
    let scene_cone = eye_response(stimulus, sigma, vec2<f32>(1.0)).y;
    let scene_slope = EYE_RESPONSE_EXPONENT * adaptation.state.w * scene_cone * (1.0 - scene_cone);
    let display_slope = EYE_RESPONSE_EXPONENT * reference_state.w * display_cone * (1.0 - display_cone);
    let cone_color = chromatically_adapted_color(linear_color, adaptation);
    // Eq.3 color compression in positive CAT16 receptor coordinates keeps
    // zero-valued RGB primaries continuous as the cone response vanishes.
    // The exact local display derivative preserves the daylight palette.
    let receptor_white = SRGB_TO_CAT16 * vec3<f32>(1.0);
    let receptors = (SRGB_TO_CAT16 * cone_color) / (luminance * receptor_white);
    var vision_color = CAT16_TO_SRGB * (receptor_white
        * pow(receptors, vec3<f32>(scene_slope / display_slope)))
        * (display_luminance / SCENE_LUMINANCE_CD_M2);
    if VISION_INTERMEDIATE {
        // The shared linear intermediate has half-float range. Its gamut
        // bound is independent of SDR/HDR output so pooled highlights agree.
        let maximum = max(vision_color.r, max(vision_color.g, vision_color.b));
        vision_color *= min(1.0, 65504.0 / maximum);
    }
    let rod_fraction = response.x / (response.x + response.y);
    return vec4<f32>(vision_color, rod_fraction);
}

fn adapted_view_color(linear_color: vec3<f32>, adaptation: EyeAdaptation, css_position_px: vec2<f32>, noise_phase: f32) -> vec3<f32> {
    let response = eye_color_response(linear_color, adaptation);
    let amplitude = vision_noise_amplitude(response.a, dot(linear_color, ROD_RGB_WEIGHTS));
    return response.rgb * (1.0 + amplitude * vision_noise(css_position_px, noise_phase));
}

fn vision_noise_amplitude(rod_fraction: f32, signal: f32) -> f32 {
    // An effective photon budget: 7 mm pupil, 10 arcmin circular patch,
    // 100 ms integration and 10% absorption, with 555 nm light at 683 lm/W.
    // Hecht et al. (1942, doi:10.1085/jgp.25.6.819) motivate the patch and
    // absorption; pupil and integration are representative dark-view assumptions.
    // These give 10480 photons/(cd/m²), or 3.91e8 per scene-light unit.
    // The filtered uniform field only approximates retinal uncertainty, not
    // Poisson statistics. Its finite ceiling is a display approximation.
    return rod_fraction / sqrt(1.0 / (0.12 * 0.12) + max(signal, 0.0) * 3.91e8);
}

fn vision_noise(css_position_px: vec2<f32>, noise_phase: f32) -> f32 {
    // CSS-pixel cells keep the grain's on-screen size the same at every graphics setting.
    let grid = css_position_px / 4.0;
    let cell = vec2<u32>(floor(grid));
    let blend = smoothstep(vec2<f32>(0.0), vec2<f32>(1.0), fract(grid));
    let phase = noise_phase % 4096.0;
    let first_phase = u32(floor(phase));
    let temporal_blend = smoothstep(0.0, 1.0, fract(phase));
    return mix(
        vision_noise_field(cell, blend, first_phase),
        vision_noise_field(cell, blend, (first_phase + 1u) % 4096u),
        temporal_blend,
    );
}

fn encode_display_color(linear_color: vec3<f32>) -> vec3<f32> {
    if VISION_INTERMEDIATE || scene.exposure_and_output.z <= 0.0 {
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
    if VISION_INTERMEDIATE {
        return max(eye_color_response(scene_color, adaptation).rgb, vec3<f32>(0.0));
    }
    let exposed = max(
        adapted_view_color(scene_color, adaptation, css_position_px, scene.exposure_and_output.w),
        vec3<f32>(0.0),
    );
    return tone_map_exposed_color(exposed);
}

fn vision_auxiliary(scene_color: vec3<f32>, alpha: f32) -> vec4<f32> {
    let luminance = SCENE_LUMINANCE_CD_M2 * dot(scene_color, vec3<f32>(0.2126, 0.7152, 0.0722));
    let low = min(luminance, 32768.0);
    let high = (luminance - low) / 32768.0;
    return vec4<f32>(low, high, 0.0, alpha);
}

fn tone_map_exposed_color(exposed: vec3<f32>) -> vec3<f32> {
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
