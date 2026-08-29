struct PlaygroundUniforms {
    cloud_detail: vec4<f32>,
    cloud_wisps: vec4<f32>,
    cloud_rendering: vec4<f32>,
    terrain_tuning: vec4<f32>,
    water_tuning: vec4<f32>,
    landform_lighting: vec4<f32>,
    layer_visibility: vec4<f32>,
    landscape_visibility: vec4<f32>,
    wind_model: vec4<f32>,
    climate_visibility: vec4<f32>,
    celestial_visibility_and_padding: vec4<u32>,
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
    );
}

fn initialize_playground_base(input: VertexOutput) -> VertexOutput {
    visual_pixel_footprint_m = max(
        0.0001,
        max(
            length(dpdx(input.world_position_m)),
            length(dpdy(input.world_position_m)),
        ),
    );
    tuning = playground_scene_tuning();
    layer_visibility = playground.layer_visibility;
    landscape_visibility = playground.landscape_visibility;
    atmosphere_model = playground.wind_model;
    climate_visibility = playground.climate_visibility;
    celestial_visibility = f32(playground.celestial_visibility_and_padding.x);
    var periodic_input = input;
    periodic_input.world_position_m = canonical_world_position(input.world_position_m);
    surface_position_m = periodic_input.world_position_m;
    return periodic_input;
}

fn initialize_playground_weather() {
    surface_cloud_weather = low_cloud_weather_at(surface_position_m);
}

fn initialize_playground_tide() {
    initialize_surface_tide();
}

fn initialize_playground_celestial() {
    initialize_celestial_lighting();
}

fn initialize_playground_atmospheric_lighting() {
    initialize_playground_celestial();
    horizon_scattering = calculate_horizon_scattering();
    moon_light_color = calculate_moon_light_color();
    ambient_light_color = calculate_atmospheric_ambient_color();
}

fn initialize_playground_scene(input: VertexOutput) -> VertexOutput {
    let periodic_input = initialize_playground_base(input);
    initialize_playground_weather();
    initialize_playground_tide();
    initialize_playground_atmospheric_lighting();
    return periodic_input;
}
