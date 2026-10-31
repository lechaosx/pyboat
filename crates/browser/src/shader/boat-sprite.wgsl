@group(1) @binding(0) var boat_texture: texture_2d<f32>;
@group(1) @binding(1) var boat_sampler: sampler;

struct BoatVertexInput {
    @location(0) position_m: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) coverage: f32,
    @location(3) depth_span_m: f32,
};

@vertex
fn vertex_boat_sprite(input: BoatVertexInput) -> VertexOutput {
    var output: VertexOutput;
    let clip_xy = (input.position_m.xy / scene.camera_and_view_m.zw) * 2.0;
    output.position = vec4<f32>(clip_xy,
        1.0 - input.position_m.z / input.depth_span_m, 1.0);
    output.color = input.color;
    output.world_position_m = scene.camera_and_view_m.xy + input.position_m.xy;
    output.is_terrain = input.coverage;
    output.coverage = clamp(1.0 + input.coverage, 0.0, 1.0);
    let css_view_height_px = scene.precipitation_phase_and_css_view_height_px.y;
    output.css_position_px = (vec2<f32>(0.5, -0.5) * clip_xy + 0.5) * vec2<f32>(
        css_view_height_px * scene.camera_and_view_m.z / scene.camera_and_view_m.w,
        css_view_height_px,
    );
    return output;
}

@fragment
fn fragment_sail_depth(input: VertexOutput) -> @location(0) vec4<f32> {
    if textureSample(boat_texture, boat_sampler, input.color.xy).a * input.coverage <= 0.0 {
        discard;
    }
    return vec4<f32>(0.0);
}

@fragment
fn fragment_boat_sprite(
    input: VertexOutput,
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4<f32> {
    // Negative state carries cloth shading; nonnegative state carries hull damage.
    let texel = textureSample(boat_texture, boat_sampler, input.color.xy);
    var color = texel.rgb / max(texel.a, 1e-6);
    if input.color.z < 0.0 {
        color *= -input.color.z;
        // Rig-atlas sail bounds correspond to boat_asset::CLOTH.
        let sail_uv = input.color.xy * vec2<f32>(1024.0, 1536.0);
        let is_sail = all(sail_uv >= vec2<f32>(48.0, 985.0))
            && all(sail_uv <= vec2<f32>(473.0, 1404.0));
        if is_sail {
            let height = clamp((1404.0 - sail_uv.y) / (1404.0 - 985.0), 0.0, 1.0);
            color *= mix(0.3, 1.0, height);
            if !front_facing {
                color *= vec3<f32>(0.8, 0.12, 0.08);
            }
        }
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
