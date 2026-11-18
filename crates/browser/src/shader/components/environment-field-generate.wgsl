@group(1) @binding(0)
var environment_field_output: texture_storage_2d_array<rgba32float, write>;

@compute @workgroup_size(8, 8)
fn generate_environment_fields(
    @builtin(global_invocation_id) invocation_id: vec3<u32>,
) {
    if all(invocation_id.xy < scene.environment_field_texels.xy) {
        let position_m = canonical_world_position(
            scene.camera_and_view_m.xy
                + scene.environment_field_grid_m.xy
                + vec2<f32>(invocation_id.xy) * scene.environment_field_grid_m.zw,
        );
        let sample = procedural_environment_field_sample(position_m);
        let layers = environment_field_layers(sample);
        for (var layer = 0u; layer < ENVIRONMENT_FIELD_LAYERS; layer++) {
            textureStore(environment_field_output, invocation_id.xy, layer, layers[layer]);
        }
        let field = cloud_weather_field_from_environment_fields(position_m, sample);
        textureStore(
            environment_field_output,
            invocation_id.xy,
            SURFACE_GUST_FIELD,
            vec4<f32>(
                surface_gust_velocity_m_s_from_weather(
                    surface_gust_signal_in_cell(
                        position_m,
                        max(scene.environment_field_grid_m.z, scene.environment_field_grid_m.w),
                    ),
                    field.weather,
                    surface_current_air_temperature_c_from_atmosphere(
                        field.atmosphere.temperature_anomaly.value,
                        field.atmosphere.climate,
                    ),
                    resolved_surface_wind_m_s_from_pressure(position_m, sample.atmosphere.pressure),
                    upper_wind_m_s_from_atmosphere(position_m, field.atmosphere),
                ),
                0.0,
                0.0,
            ),
        );
    }
    if all(invocation_id.xy < scene.cloud_weather_texels.xy) {
        let position_m = canonical_world_position(
            scene.camera_and_view_m.xy
                + scene.cloud_weather_grid_m.xy
                + vec2<f32>(invocation_id.xy) * scene.cloud_weather_grid_m.zw,
        );
        let field = cloud_weather_field_at(position_m);
        var weather = field.weather;
        weather.land_fraction = land_fraction_from_broad_elevation_m(
            broad_landform_elevation_m(position_m),
        );
        textureStore(
            environment_field_output,
            invocation_id.xy,
            GROUND_BOUNCE_FIELD,
            ground_bounce_field(position_m, field.atmosphere, weather),
        );
        textureStore(
            environment_field_output,
            invocation_id.xy,
            CUMULUS_DRIVER_FIELD,
            cumulus_drivers(weather),
        );
        textureStore(
            environment_field_output,
            invocation_id.xy,
            DECK_DRIVER_FIELD,
            deck_drivers(weather),
        );
    }
}
