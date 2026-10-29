// The eye adapts to the light falling on the scene, pooled over its adapting
// field, and sees each surface's reflectance against it, so albedo edges such
// as coasts and snow beside water leave no halos. A compromise of the eye
// layer: real adaptation also responds to bright surfaces, such as the glare
// of snow, which this ignores.

// Texels along each axis of the illumination sampled over the view and of the
// adapting field blurred from it; adapting_field.rs mirrors them. The field's
// Gaussian spans 9 illumination texels vertically and 5 across a 16:9 view,
// so a sample crossing a shadow edge moves the field by under 1% of a stop.
const ILLUMINATION_TEXELS: u32 = 32u;
const ADAPTING_FIELD_TEXELS: u32 = 32u;

// Radius of the adapting field in visual angle: CIE 159:2004's background,
// which reaches about 10° beyond the proximal field, itself about 2° beyond
// the 2° stimulus.
const ADAPTING_FIELD_RADIUS_DEGREES: f32 = 13.0;
// A compromise: the view's height is taken to subtend 23°, 1080 CSS pixels at
// the CSS reference pixel's 0.0213°, which is a full-HD desktop monitor at
// arm's length. The real angle depends on the display and the viewer.
const VIEW_HEIGHT_DEGREES: f32 = 23.0;

@group(0) @binding(7)
var adapting_field: texture_2d<f32>;

// Light in stops of scene light, as the adapting field holds it. The eye
// adapts no further than full dark adaptation, so darker light counts as
// that bound.
fn adapting_light_stops(light: f32) -> f32 {
    return log2(max(light, NIGHT_SKY_LIGHT));
}

// Gain of the part of the view at `css_position_px`, adapted to the field there.
fn field_adapted_gain(css_position_px: vec2<f32>) -> f32 {
    let css_view_height_px = scene.precipitation_phase_and_css_view_height_px.y;
    let css_view_px = vec2<f32>(
        css_view_height_px * scene.camera_and_view_m.z / scene.camera_and_view_m.w,
        css_view_height_px,
    );
    let last = ADAPTING_FIELD_TEXELS - 1u;
    let texel = clamp(
        css_position_px / css_view_px * f32(ADAPTING_FIELD_TEXELS) - 0.5,
        vec2<f32>(0.0),
        vec2<f32>(f32(last)),
    );
    let origin = vec2<u32>(texel);
    let next = min(origin + 1u, vec2<u32>(last));
    let blend = texel - vec2<f32>(origin);
    let stops = mix(
        mix(
            textureLoad(adapting_field, origin, 0).r,
            textureLoad(adapting_field, vec2<u32>(next.x, origin.y), 0).r,
            blend.x,
        ),
        mix(
            textureLoad(adapting_field, vec2<u32>(origin.x, next.y), 0).r,
            textureLoad(adapting_field, next, 0).r,
            blend.x,
        ),
        blend.y,
    );
    return adapted_gain(exp2(stops));
}
