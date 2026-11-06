@group(1) @binding(0) var boat_texture: texture_2d<f32>;
@group(1) @binding(1) var boat_sampler: sampler;
// The height above mean sea level of the surface each scene pixel shows,
// which the scene pass writes beside its colour.
@group(2) @binding(0) var surface_height: texture_2d<f32>;
override IS_SAIL: bool = false;

struct BoatVertexInput {
    // Offset from the view centre, then height above mean sea level.
    @location(0) position_m: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) coverage: f32,
};

struct BoatSpriteOutput {
    // Equal-depth passes require the same clip positions across pipelines.
    @invariant @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) coverage: f32,
    // Position in CSS pixels from the top left, independent of render resolution.
    @location(2) @interpolate(linear) css_position_px: vec2<f32>,
    // The sea-level offset from the view centre seen through the fragment.
    @location(3) @interpolate(linear) view_offset_m: vec2<f32>,
    @location(4) height_m: f32,
};

// Projects as `vertex_terrain_depth` does; depth orders sail folds.
@vertex
fn vertex_boat_sprite(input: BoatVertexInput) -> BoatSpriteOutput {
    var output: BoatSpriteOutput;
    let scale = 1.0 - input.position_m.z / scene.drone_camera.x;
    let clip_xy = (input.position_m.xy / scene.camera_and_view_m.zw) * 2.0;
    output.position = vec4<f32>(clip_xy, scale - scene.drone_camera.y, scale);
    output.color = input.color;
    output.coverage = input.coverage;
    output.view_offset_m = input.position_m.xy / scale;
    let css_view_height_px = scene.precipitation_phase_and_css_view_height_px.y;
    output.css_position_px = (vec2<f32>(0.5, -0.5) * clip_xy / scale + 0.5) * vec2<f32>(
        css_view_height_px * scene.camera_and_view_m.z / scene.camera_and_view_m.w,
        css_view_height_px,
    );
    output.height_m = input.position_m.z;
    return output;
}

// Looking straight down, the camera sees a higher point on any eye ray
// before a lower one, so a fragment is hidden where the surface the scene
// shows through it stands higher. The occlusion edge has the scene's
// resolution.
fn hidden_by_surface(input: BoatSpriteOutput) -> bool {
    let view = vec2<f32>(0.5) + vec2<f32>(1.0, -1.0) * input.view_offset_m / scene.camera_and_view_m.zw;
    let size = vec2<i32>(textureDimensions(surface_height));
    let texel = clamp(vec2<i32>(floor(view * vec2<f32>(size))), vec2<i32>(0), size - 1);
    return textureLoad(surface_height, texel, 0).r > input.height_m;
}

@fragment
fn fragment_sail_depth(input: BoatSpriteOutput) -> @location(0) vec4<f32> {
    if textureSample(boat_texture, boat_sampler, input.color.xy).a * input.coverage <= 0.0 {
        discard;
    }
    return vec4<f32>(0.0);
}

@fragment
fn fragment_boat_sprite(
    input: BoatSpriteOutput,
    @builtin(front_facing) front_facing: bool,
) -> @location(0) vec4<f32> {
    let texel = textureSample(boat_texture, boat_sampler, input.color.xy);
    if hidden_by_surface(input) {
        discard;
    }
    // Negative state carries cloth shading; nonnegative state carries hull damage.
    var color = texel.rgb / max(texel.a, 1e-6);
    if input.color.z < 0.0 {
        color *= -input.color.z;
        if IS_SAIL {
            let height = clamp((1404.0 - input.color.y * 1536.0) / (1404.0 - 985.0), 0.0, 1.0);
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
