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
        let layers = environment_field_layers(procedural_environment_field_sample(position_m));
        for (var layer = 0u; layer < ENVIRONMENT_FIELD_LAYERS; layer++) {
            textureStore(environment_field_output, invocation_id.xy, layer, layers[layer]);
        }
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
