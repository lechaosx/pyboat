override LIGHTING_MODEL_REGRESSION_OUTPUT: u32 = 0u;
override LIGHTING_MODEL_SURFACE_HEIGHT_M: f32 = 0.0;
override LIGHTING_MODEL_EXPOSURE_GAIN: f32 = 1.0;
override LIGHTING_MODEL_VISION_RED: f32 = 0.6;
override LIGHTING_MODEL_VISION_GREEN: f32 = 0.25;
override LIGHTING_MODEL_VISION_BLUE: f32 = 0.1;
override LIGHTING_MODEL_SIGNAL: f32 = 0.0001;
override LIGHTING_MODEL_NOISE_PHASE: f32 = 0.0;
override LIGHTING_MODEL_LIGHT_HEIGHT: f32 = 0.5;
override LIGHTING_MODEL_CLOUD_DEPTH: f32 = 1.0;

@fragment
fn fragment_lighting_model_regressions(input: VertexOutput) -> @location(0) vec4<f32> {
    initialize_playground_scene(input);
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 21u {
        return vec4<f32>(night_vision_color(
            vec3<f32>(LIGHTING_MODEL_VISION_RED, LIGHTING_MODEL_VISION_GREEN, LIGHTING_MODEL_VISION_BLUE),
            rod_vision_fraction(LIGHTING_MODEL_EXPOSURE_GAIN),
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
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 26u {
        // The displayed light of the vision colour along a horizontal ramp
        // from 1/64 to 64 diffuse whites, in a view adapted to daylight, at a
        // quarter scale so an HDR peak of 4 fits the 8-bit image.
        let css_view_width_px = scene.precipitation_phase_and_css_view_height_px.y
            * scene.camera_and_view_m.z / scene.camera_and_view_m.w;
        let level = exp2(mix(-6.0, 6.0, input.css_position_px.x / css_view_width_px));
        let color = vec3<f32>(LIGHTING_MODEL_VISION_RED, LIGHTING_MODEL_VISION_GREEN, LIGHTING_MODEL_VISION_BLUE);
        return vec4<f32>(0.25 * tone_map_scene_color(level * color, 1.0, input.css_position_px), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 24u {
        return diagnostic_shadow_darkness(LIGHTING_MODEL_LIGHT_HEIGHT, LIGHTING_MODEL_CLOUD_DEPTH);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 27u || LIGHTING_MODEL_REGRESSION_OUTPUT == 28u {
        // Sweeps the Sun, with the Moon set, from 0.5 degrees above a
        // receiver's horizontal on the left to 2.5 degrees below it on the
        // right. Under a clear sky (27), red is its direct sunlight on level
        // ground, in fiftieths of a zenith Sun's clear-sky light, and green
        // that light's red share. Under a full storm deck (28), red is its
        // diffuse light, in hundredths of a zenith Sun's clear-sky light, and
        // green the Sun's beam share of its clear-sky light.
        let css_view_width_px = scene.precipitation_phase_and_css_view_height_px.y
            * scene.camera_and_view_m.z / scene.camera_and_view_m.w;
        let depression = radians(mix(-0.5, 2.5, input.css_position_px.x / css_view_width_px));
        sun_light_direction = vec3<f32>(cos(depression), 0.0, -sin(depression));
        geocentric_moon_light_direction = vec3<f32>(0.0, 0.0, -1.0);
        let receiver = celestial_receiver_light_at_height(LIGHTING_MODEL_SURFACE_HEIGHT_M);
        if LIGHTING_MODEL_REGRESSION_OUTPUT == 28u {
            let ambient = calculate_atmospheric_ambient_light(
                RegionalCanopyConditions(1.0, 30.0),
                receiver,
            );
            let diffuse = ambient.split.diffuse_share * ambient.clear_sky
                + ambient.canopy
                + ambient.canopy_scattered;
            return vec4<f32>(
                100.0 * dot(diffuse, LUMINANCE_WEIGHTS),
                ambient.split.beam_shares.x,
                0.0,
                1.0,
            );
        }
        let direct = calculate_atmospheric_ambient_light(RegionalCanopyConditions(0.0, 0.0), receiver)
            .split.direct.x * sun_direct_light_color(receiver);
        return vec4<f32>(
            50.0 * dot(direct, LUMINANCE_WEIGHTS),
            direct.r / max(direct.r + direct.g + direct.b, 1e-30),
            0.0,
            1.0,
        );
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
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 25u {
        let receiver = celestial_receiver_light_at_height(LIGHTING_MODEL_SURFACE_HEIGHT_M);
        let share = moon_clear_sky_share(
            receiver,
            calculate_atmospheric_ambient_light(RegionalCanopyConditions(0.0, 0.0), receiver),
        );
        // Tenfold in green resolves the small shares near the grid's threshold.
        return vec4<f32>(share, min(10.0 * share, 1.0), 0.0, 1.0);
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
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 16u {
        return diagnostic_below_datum_cloud_projection(input);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 11u {
        return diagnostic_landform_shadow_penumbra_center();
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 12u {
        return diagnostic_landform_shadow_partial_disc_ray();
    }
    return diagnostic_ambient_with_surface_horizon(LIGHTING_MODEL_SURFACE_HEIGHT_M);
}
