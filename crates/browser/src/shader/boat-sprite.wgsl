@group(1) @binding(0) var boat_texture: texture_2d<f32>;
@group(1) @binding(1) var boat_sampler: sampler;
// The height above mean sea level of the surface each scene pixel shows,
// which the scene pass writes beside its colour.
@group(2) @binding(0) var surface_height: texture_2d<f32>;
@group(2) @binding(1) var scene_light: texture_2d<f32>;
struct BoatTarget {
    clip_scale_offset: vec4<f32>,
    origin_and_size_px: vec4<f32>,
};
@group(2) @binding(2) var<uniform> boat_target: BoatTarget;
@group(2) @binding(3) var sail_depth: texture_depth_multisampled_2d;
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

// Projection and lighting retain the full view's coordinates when the boat
// attachments cover only the occupied rectangle.
@vertex
fn vertex_boat_sprite(input: BoatVertexInput) -> BoatSpriteOutput {
    var output: BoatSpriteOutput;
    let scale = 1.0 - input.position_m.z / scene.drone_camera.x;
    let clip_xy = (input.position_m.xy / scene.camera_and_view_m.zw) * 2.0;
    output.position = vec4<f32>(
        clip_xy * boat_target.clip_scale_offset.xy + scale * boat_target.clip_scale_offset.zw,
        scale - scene.drone_camera.y, scale,
    );
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

// Reuse the unblurred illumination grid to avoid evaluating the full scene per
// boat fragment. Small shadow edges inherit its resolution and ground height.
fn boat_illumination(input: BoatSpriteOutput) -> vec3<f32> {
    let uv = vec2<f32>(0.5) + vec2<f32>(1.0, -1.0) * input.view_offset_m / scene.camera_and_view_m.zw;
    let size = textureDimensions(scene_light);
    let texel = clamp(uv * vec2<f32>(size) - 0.5, vec2<f32>(0.0), vec2<f32>(size - 1u));
    let origin = vec2<u32>(texel);
    let next = min(origin + 1u, size - 1u);
    let blend = fract(texel);
    return mix(
        mix(textureLoad(scene_light, origin, 0).rgb, textureLoad(scene_light, vec2<u32>(next.x, origin.y), 0).rgb, blend.x),
        mix(textureLoad(scene_light, vec2<u32>(origin.x, next.y), 0).rgb, textureLoad(scene_light, next, 0).rgb, blend.x),
        blend.y,
    );
}

struct SailDepth {
    @builtin(frag_depth) depth: f32,
    @builtin(sample_mask) coverage: u32,
};

@fragment
fn fragment_sail_depth(input: BoatSpriteOutput, @builtin(sample_index) sample_index: u32) -> SailDepth {
    if textureSample(boat_texture, boat_sampler, input.color.xy).a * input.coverage <= 0.0 {
        discard;
    }
    // Store the shader's per-sample depth so the colour pass compares the
    // same arithmetic rather than the rasterizer's rounded depth interpolation.
    return SailDepth(input.position.z, 1u << sample_index);
}

struct BoatFragment {
    @location(0) color: vec4<f32>,
    @builtin(sample_mask) coverage: u32,
};

fn shade_boat_sprite(
    input: BoatSpriteOutput,
    front_facing: bool,
    sample_index: u32,
) -> BoatFragment {
    let texel = textureSample(boat_texture, boat_sampler, input.color.xy);
    if hidden_by_surface(input) {
        discard;
    }
    if IS_SAIL && input.position.z != textureLoad(sail_depth, vec2<i32>(input.position.xy), i32(sample_index)) {
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
    var output: BoatFragment;
    output.color = vec4<f32>(encode_display_color(tone_map_scene_color(
        color * boat_illumination(input), adaptation, input.css_position_px,
    )), 1.0);
    // Alpha describes filtered silhouette coverage. Retained samples are opaque;
    // applying alpha again when resolving would attenuate their edges twice.
    let samples = u32(round(clamp(texel.a * input.coverage, 0.0, 1.0) * 4.0));
    output.coverage = (1u << samples) - 1u;
    return output;
}

@fragment
fn fragment_boat_sprite(input: BoatSpriteOutput, @builtin(front_facing) front_facing: bool) -> BoatFragment {
    return shade_boat_sprite(input, front_facing, 0u);
}

@fragment
fn fragment_sail(input: BoatSpriteOutput, @builtin(front_facing) front_facing: bool, @builtin(sample_index) sample_index: u32) -> BoatFragment {
    return shade_boat_sprite(input, front_facing, sample_index);
}
