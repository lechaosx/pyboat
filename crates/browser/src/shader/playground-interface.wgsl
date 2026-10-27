struct PlaygroundUniforms {
    water_tuning: vec4<f32>,
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
        playground.water_tuning,
        playground.landform_lighting,
        playground.cloud_shadows,
    );
}

struct InitializedPlaygroundScene {
    input: VertexOutput,
    atmosphere: SurfaceAtmosphereSample,
    surface_circle_position: vec4<f32>,
}

fn initialize_playground_base(input: VertexOutput) -> VertexOutput {
    visual_pixel_footprint_m = scene.world_phases_and_pixel_footprint_m.w;
    tuning = playground_scene_tuning();
    landscape_visibility = playground.landscape_visibility;
    var periodic_input = input;
    periodic_input.world_position_m = canonical_world_position(input.world_position_m);
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

fn water_material_color_visibility() -> vec2<f32> {
    return vec2<f32>(playground.water_material_color_visibility.xy);
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
