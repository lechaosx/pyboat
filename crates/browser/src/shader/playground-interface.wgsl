struct PlaygroundUniforms {
    landform_lighting: vec4<f32>,
    cloud_shadows: vec4<f32>,
    layer_visibility: vec4<f32>,
    landscape_visibility: vec4<f32>,
    light_contribution_visibility: vec4<u32>,
    land_material_color_visibility: vec4<u32>,
    solid_material_color_visibility: vec4<u32>,
    water_material_color_visibility: vec4<u32>,
    environment_features: vec4<f32>,
    material_features: vec4<f32>,
    effect_features: vec4<f32>,
    occluder_field_inspection: vec4<u32>,
};

@group(1) @binding(0)
var<uniform> playground: PlaygroundUniforms;

fn playground_scene_tuning() -> SceneTuning {
    return SceneTuning(
        playground.landform_lighting,
        playground.cloud_shadows,
    );
}

struct InitializedPlaygroundScene {
    input: VertexOutput,
    atmosphere: SurfaceAtmosphereSample,
    surface_circle_position: vec4<f32>,
}

// The terrain the drone camera sees through `input`'s sea-level point.
fn initialize_playground_base(input: VertexOutput) -> VertexOutput {
    // Compromise: the terrain depth holds one copy of the world, so its lookup
    // wraps offsets to the nearest copy, and a view wider than the world
    // repeats and cuts leaning land at half a lap from its centre.
    let view_offset_m = camera_relative_position_m(input.world_position_m);
    let hit_scale = terrain_hit_scale(view_offset_m);
    surface_view_offset_m = (input.world_position_m - scene.camera_and_view_m.xy) * hit_scale;
    // Compromise: the footprint narrows with the hit's height but ignores its
    // slope, which spreads a pixel over more ground, so steep land is
    // filtered as if level.
    visual_pixel_footprint_m = scene.world_phases_and_pixel_footprint_m.w * hit_scale;
    tuning = playground_scene_tuning();
    landscape_visibility = playground.landscape_visibility;
    var periodic_input = input;
    periodic_input.world_position_m = canonical_world_position(
        scene.camera_and_view_m.xy + view_offset_m * hit_scale,
    );
    surface_position_m = periodic_input.world_position_m;
    return periodic_input;
}

fn environment_feature_visibility() -> vec4<f32> {
    return playground.environment_features;
}

fn material_feature_visibility() -> vec4<f32> {
    return playground.material_features;
}

fn effect_feature_visibility() -> vec4<f32> {
    return playground.effect_features;
}

fn lighting_feature_visibility() -> vec4<f32> {
    return vec4<f32>(1.0);
}

fn direct_light_visibility() -> vec2<f32> {
    return vec2<f32>(1.0);
}

fn fragment_layer_visibility() -> vec4<f32> {
    return playground.layer_visibility;
}

fn fragment_light_visibility() -> vec4<f32> {
    return vec4<f32>(playground.light_contribution_visibility);
}

fn land_material_color_visibility() -> vec4<f32> {
    return vec4<f32>(playground.land_material_color_visibility);
}

fn solid_material_color_visibility() -> vec4<f32> {
    return vec4<f32>(playground.solid_material_color_visibility);
}

fn water_material_color_visibility() -> vec3<f32> {
    return vec3<f32>(playground.water_material_color_visibility.xyz);
}

fn initialize_playground_weather() {
    surface_cloud_weather = cloud_weather_at(surface_position_m);
}

fn initialize_playground_tide() {
    initialize_surface_tide();
}

fn initialize_playground_tide_value() {
    initialize_surface_tide_value();
}

fn initialize_playground_celestial() {
    initialize_celestial_lighting();
}

fn initialize_playground_atmospheric_lighting() {
    initialize_playground_celestial();
    local_aerosol_optical_depth = local_aerosol_optical_depth_at(surface_cloud_weather);
    regional_aerosol_optical_depth = regional_aerosol_optical_depth_at(surface_cloud_weather.regional);
}

fn initialize_playground_scene_from_environment_fields(
    input: VertexOutput,
) -> InitializedPlaygroundScene {
    let periodic_input = initialize_playground_base(input);
    let surface_circle_position = periodic_noise_circle_position_from_canonical(
        surface_position_m,
    );
    let fields = environment_field_sample(surface_position_m);
    let surface_weather = cloud_weather_field_from_environment_fields(
        surface_position_m,
        fields,
    );
    surface_cloud_weather = surface_weather.weather;
    initialize_surface_tide_from_sample(fields.tide);
    initialize_playground_atmospheric_lighting();
    return InitializedPlaygroundScene(
        periodic_input,
        surface_weather.atmosphere,
        surface_circle_position,
    );
}

fn initialize_playground_scene(input: VertexOutput) -> InitializedPlaygroundScene {
    let periodic_input = initialize_playground_base(input);
    let surface_circle_position = periodic_noise_circle_position_from_canonical(
        surface_position_m,
    );
    let surface_weather = cloud_weather_field_at(surface_position_m);
    surface_cloud_weather = surface_weather.weather;
    initialize_surface_tide_from_circle(surface_circle_position);
    initialize_playground_atmospheric_lighting();
    return InitializedPlaygroundScene(
        periodic_input,
        surface_weather.atmosphere,
        surface_circle_position,
    );
}
