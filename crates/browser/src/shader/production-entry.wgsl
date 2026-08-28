@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    visual_pixel_footprint_m = max(
        0.0001,
        max(
            length(dpdx(input.world_position_m)),
            length(dpdy(input.world_position_m)),
        ),
    );
    tuning = production_tuning();
    layer_visibility = vec4<f32>(1.0);
    landscape_visibility = vec4<f32>(1.0);
    atmosphere_model = vec4<f32>(1.0);
    climate_visibility = vec4<f32>(1.0);
    celestial_visibility = 1.0;
    var periodic_input = input;
    periodic_input.world_position_m = canonical_world_position(input.world_position_m);
    surface_position_m = periodic_input.world_position_m;
    surface_cloud_weather = cloud_weather_at(surface_position_m);
    initialize_surface_tide();
    initialize_celestial_lighting();
    horizon_scattering = calculate_horizon_scattering();
    moon_light_color = calculate_moon_light_color();
    ambient_light_color = calculate_atmospheric_ambient_color();
    return final_scene_color(periodic_input);
}
