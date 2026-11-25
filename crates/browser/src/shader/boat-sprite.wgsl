@group(1) @binding(0) var boat_texture: texture_2d<f32>;
@group(1) @binding(1) var boat_sampler: sampler;
// The height above mean sea level of the surface each scene pixel shows,
// which the scene pass writes beside its colour.
@group(2) @binding(0) var surface_height: texture_2d<f32>;
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
    @location(2) coverage_or_projected_sheer_m: f32,
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
    @location(4) @interpolate(linear) projected_sheer_m: f32,
    @location(5) world_position_m: vec3<f32>,
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
    let hull = input.color.z >= 0.0;
    output.coverage = select(input.coverage_or_projected_sheer_m, 1.0, hull);
    output.projected_sheer_m = select(0.0, input.coverage_or_projected_sheer_m, hull);
    output.view_offset_m = input.position_m.xy / scale;
    let css_view_height_px = scene.css_view_height_px_and_padding.x;
    output.css_position_px = (vec2<f32>(0.5, -0.5) * clip_xy / scale + 0.5) * vec2<f32>(
        css_view_height_px * scene.camera_and_view_m.z / scene.camera_and_view_m.w,
        css_view_height_px,
    );
    output.world_position_m = input.position_m;
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
    var height_m = input.world_position_m.z;
    if input.color.z >= 0.0 {
        height_m = input.projected_sheer_m / (1.0 + input.projected_sheer_m / scene.drone_camera.x);
    }
    return textureLoad(surface_height, texel, 0).r > height_m;
}

fn boat_illumination(input: BoatSpriteOutput, normal: vec3<f32>) -> vec3<f32> {
    boat_shadow_receiver_normal = normal;
    surface_position_m = canonical_world_position(scene.camera_and_view_m.xy + input.world_position_m.xy);
    surface_view_offset_m = input.world_position_m.xy;
    visual_pixel_footprint_m = scene.world_phases_and_pixel_footprint_m.w;
    tuning = production_tuning();
    landscape_visibility = vec4<f32>(1.0);
    let fields = environment_field_sample(surface_position_m);
    surface_cloud_weather = cloud_weather_field_from_environment_fields(surface_position_m, fields).weather;
    initialize_surface_tide_from_sample(fields.tide);
    initialize_celestial_lighting();
    local_aerosol_optical_depth = local_aerosol_optical_depth_at(surface_cloud_weather);
    regional_aerosol_optical_depth = regional_aerosol_optical_depth_at(surface_cloud_weather.regional);
    let receiver = celestial_receiver_light_at_height(input.world_position_m.z);
    let ambient = calculate_atmospheric_ambient_light(regional_canopy_conditions(surface_cloud_weather), receiver);
    let sky = vec3<f32>(0.5 + 0.5 * normal.z);
    let response = SurfaceLightResponse(sky, sky, sky,
        vec3<f32>(ambient.split.normal.x * max(dot(normal, receiver.sun_direction), 0.0)),
        vec3<f32>(ambient.split.normal.y * max(dot(normal, receiver.moon_direction), 0.0)));
    return fragment_surface_light(receiver, landform_shadow_transmittance(receiver), response,
        ambient, sampled_ground_bounce_conditions(surface_position_m), vec4<f32>(1.0), 1.0)
        + lightning_matte_light(receiver, normal);
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

var<private> boat_vision_auxiliary: vec4<f32>;
struct VisionBoatFragment {
    @location(0) color: vec4<f32>,
    @location(1) auxiliary: vec4<f32>,
    @builtin(sample_mask) coverage: u32,
};

@fragment
fn fragment_boat_vision(input: BoatSpriteOutput, @builtin(front_facing) front_facing: bool) -> VisionBoatFragment {
    let output = shade_boat_sprite(input, front_facing, 0u);
    return VisionBoatFragment(output.color, boat_vision_auxiliary, output.coverage);
}

@fragment
fn fragment_sail_vision(input: BoatSpriteOutput, @builtin(front_facing) front_facing: bool, @builtin(sample_index) sample_index: u32) -> VisionBoatFragment {
    let output = shade_boat_sprite(input, front_facing, sample_index);
    return VisionBoatFragment(output.color, boat_vision_auxiliary, output.coverage);
}

fn shade_boat_sprite(
    input: BoatSpriteOutput,
    front_facing: bool,
    sample_index: u32,
) -> BoatFragment {
    let cross_normal = normalize(cross(dpdy(input.world_position_m), dpdx(input.world_position_m)));
    let view = vec3<f32>(-input.world_position_m.xy, scene.drone_camera.x - input.world_position_m.z);
    let normal = select(cross_normal, -cross_normal, dot(cross_normal, view) < 0.0);
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
    let stimulus = color * boat_illumination(input, normal);
    if VISION_INTERMEDIATE {
        boat_vision_auxiliary = vision_auxiliary(stimulus, 1.0);
    }
    var output: BoatFragment;
    output.color = vec4<f32>(encode_display_color(tone_map_scene_color(
        stimulus, adaptation, input.css_position_px,
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
