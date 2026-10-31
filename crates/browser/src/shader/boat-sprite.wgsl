@group(1) @binding(0) var boat_texture: texture_2d<f32>;
@group(1) @binding(1) var boat_sampler: sampler;

@fragment
fn fragment_boat_sprite(input: VertexOutput) -> @location(0) vec4<f32> {
    // Negative state carries cloth shading; nonnegative state carries hull damage.
    let texel = textureSample(boat_texture, boat_sampler, input.color.xy);
    var color = texel.rgb / max(texel.a, 1e-6);
    if input.color.z < 0.0 {
        color *= -input.color.z;
    } else if input.color.z > 1.5 {
        color = mix(color, vec3<f32>(0.8, 0.12, 0.05), 0.65);
    } else if input.color.z > 0.5 {
        let gray = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
        color = mix(color, vec3<f32>(gray), 0.7) * 0.65;
    }
    let adaptation = field_adaptation(input.css_position_px);
    return vec4<f32>(encode_display_color(tone_map_scene_color(
        adapted_unlit_color(color, adaptation), adaptation, input.css_position_px,
    )), texel.a * input.coverage);
}
