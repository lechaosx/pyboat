override LIGHTING_MODEL_REGRESSION_OUTPUT: u32 = 0u;
override LIGHTING_MODEL_SURFACE_HEIGHT_M: f32 = 0.0;
override LIGHTING_MODEL_EXPOSURE_GAIN: f32 = 1.0;
override LIGHTING_MODEL_VISION_RED: f32 = 0.6;
override LIGHTING_MODEL_VISION_GREEN: f32 = 0.25;
override LIGHTING_MODEL_VISION_BLUE: f32 = 0.1;
override LIGHTING_MODEL_SIGNAL: f32 = 0.0001;
override LIGHTING_MODEL_NOISE_PHASE: f32 = 0.0;

@fragment
fn fragment_lighting_model_regressions(input: VertexOutput) -> @location(0) vec4<f32> {
    initialize_playground_scene(input);
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 21u {
        return vec4<f32>(night_vision_color(
            vec3<f32>(LIGHTING_MODEL_VISION_RED, LIGHTING_MODEL_VISION_GREEN, LIGHTING_MODEL_VISION_BLUE),
            LIGHTING_MODEL_EXPOSURE_GAIN,
        ), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 22u {
        return vec4<f32>(adapted_view_color(
            vec3<f32>(LIGHTING_MODEL_SIGNAL), LIGHTING_MODEL_EXPOSURE_GAIN,
            input.position.xy, LIGHTING_MODEL_NOISE_PHASE,
        ) * (0.5 / LIGHTING_MODEL_SIGNAL), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 23u {
        return vec4<f32>(adapted_view_color(
            vec3<f32>(0.0), LIGHTING_MODEL_EXPOSURE_GAIN,
            input.position.xy, LIGHTING_MODEL_NOISE_PHASE,
        ), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 0u {
        return diagnostic_low_cloud_sun_lighting();
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 1u {
        return diagnostic_landform_sun_grade_moon_coupling();
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 2u {
        return diagnostic_receiver_ambient(LIGHTING_MODEL_SURFACE_HEIGHT_M);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 3u {
        return diagnostic_receiver_ambient_share(LIGHTING_MODEL_SURFACE_HEIGHT_M);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 19u {
        let receiver = celestial_receiver_light_at_height(LIGHTING_MODEL_SURFACE_HEIGHT_M);
        return vec4<f32>(
            celestial_atmospheric_transmittance(receiver, receiver.sun_direction),
            celestial_atmospheric_transmittance(receiver, receiver.moon_direction),
            0.0,
            1.0,
        );
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 5u {
        return diagnostic_landform_shadow_disc_radii(input);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 6u {
        return diagnostic_cloud_projection(input);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 7u {
        return diagnostic_cloud_shadow_receiver_light(LIGHTING_MODEL_SURFACE_HEIGHT_M);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 8u {
        return diagnostic_world_light_direction();
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 9u {
        return diagnostic_cloud_projection_isotropy(input);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 10u {
        return diagnostic_landform_shadow_tangent_continuity(input);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 15u {
        return diagnostic_projected_low_cloud_receiver(input);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 16u {
        return diagnostic_below_datum_cloud_projection(input);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 17u {
        return diagnostic_cloud_disc_transverse_spread(input);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 18u {
        return diagnostic_projected_low_cloud_direct_color(input);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 11u {
        return diagnostic_landform_shadow_penumbra_center();
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 12u {
        return diagnostic_landform_shadow_partial_disc_ray();
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 13u {
        return diagnostic_cloud_shadow_disc_radius(input);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 14u {
        return diagnostic_low_cloud_canopy_shadow_disc_radius(input);
    }
    return diagnostic_ambient_with_surface_horizon(LIGHTING_MODEL_SURFACE_HEIGHT_M);
}
