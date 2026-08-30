struct PlaygroundUniforms {
    cloud_detail: vec4<f32>,
    cloud_wisps: vec4<f32>,
    cloud_rendering: vec4<f32>,
    terrain_tuning: vec4<f32>,
    water_tuning: vec4<f32>,
    landform_lighting: vec4<f32>,
    cloud_lighting: vec4<f32>,
    cloud_shadows: vec4<f32>,
    water_surface: vec4<f32>,
    ice_surface: vec4<f32>,
    ice_structure: vec4<f32>,
    land_material_normals: vec4<f32>,
    material_coverage: vec4<f32>,
    light_balance: vec4<f32>,
    layer_visibility: vec4<f32>,
    landscape_visibility: vec4<f32>,
    wind_model: vec4<f32>,
    climate_visibility: vec4<f32>,
    light_contribution_visibility: vec4<u32>,
    land_material_color_visibility: vec4<u32>,
    solid_material_color_visibility: vec4<u32>,
    water_material_color_visibility: vec4<u32>,
    layer_material_color_visibility: vec4<u32>,
    environment_features: vec4<f32>,
    material_features: vec4<f32>,
    effect_features: vec4<f32>,
};

@group(1) @binding(0)
var<uniform> playground: PlaygroundUniforms;

fn playground_scene_tuning() -> SceneTuning {
    return SceneTuning(
        playground.cloud_detail,
        playground.cloud_wisps,
        playground.cloud_rendering,
        playground.terrain_tuning,
        playground.water_tuning,
        playground.landform_lighting,
        playground.cloud_lighting,
        playground.cloud_shadows,
        playground.water_surface,
        playground.ice_surface,
        playground.ice_structure,
        playground.land_material_normals,
        playground.material_coverage,
        playground.light_balance,
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
    layer_visibility = vec4<f32>(1.0);
    landscape_visibility = playground.landscape_visibility;
    atmosphere_model = playground.wind_model;
    climate_visibility = playground.climate_visibility;
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
    return playground.light_balance
        * vec4<f32>(playground.light_contribution_visibility);
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

fn low_cloud_material_color_visibility() -> f32 {
    return f32(playground.layer_material_color_visibility.x);
}

fn initialize_playground_weather() {
    surface_cloud_weather = low_cloud_weather_at(surface_position_m);
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
    horizon_scattering = calculate_horizon_scattering();
    moon_light_color = calculate_moon_light_color();
    initialize_light_contributions();
}

fn initialize_playground_scene(input: VertexOutput) -> InitializedPlaygroundScene {
    let periodic_input = initialize_playground_base(input);
    let surface_circle_position = periodic_noise_circle_position_from_canonical(
        surface_position_m,
    );
    let surface_weather = low_cloud_weather_field_at(surface_position_m);
    surface_cloud_weather = surface_weather.weather;
    initialize_surface_tide_from_circle(surface_circle_position);
    initialize_playground_atmospheric_lighting();
    return InitializedPlaygroundScene(
        periodic_input,
        surface_weather.atmosphere,
        surface_circle_position,
    );
}
