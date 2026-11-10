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
override LIGHTING_MODEL_SLOPE_DEGREES: f32 = 30.0;
override LIGHTING_MODEL_SUN_DEGREES: f32 = 30.0;
override LIGHTING_MODEL_VIEW_DEGREES: f32 = 0.0;
override LIGHTING_MODEL_SEDIMENT: f32 = 0.0;

@fragment
fn fragment_lighting_model_regressions(input: VertexOutput) -> @location(0) vec4<f32> {
    initialize_playground_scene(input);
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 21u {
        let color = vec3<f32>(LIGHTING_MODEL_VISION_RED, LIGHTING_MODEL_VISION_GREEN, LIGHTING_MODEL_VISION_BLUE);
        return vec4<f32>(night_vision_color(
            color,
            color,
            rod_vision_fraction(LIGHTING_MODEL_EXPOSURE_GAIN),
        ), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 22u {
        return vec4<f32>(adapted_view_color(
            vec3<f32>(LIGHTING_MODEL_SIGNAL), EyeAdaptation(LIGHTING_MODEL_EXPOSURE_GAIN, vec3<f32>(1.0)),
            input.position.xy, LIGHTING_MODEL_NOISE_PHASE,
        ) * (0.5 / LIGHTING_MODEL_SIGNAL), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 23u {
        return vec4<f32>(adapted_view_color(
            vec3<f32>(0.0), EyeAdaptation(LIGHTING_MODEL_EXPOSURE_GAIN, vec3<f32>(1.0)),
            input.position.xy, LIGHTING_MODEL_NOISE_PHASE,
        ), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 31u {
        // A 50% grey lit by the vision colour as an illuminant at unit
        // luminance, as the cones of an eye adapted to that illuminant at
        // LIGHTING_MODEL_EXPOSURE_GAIN see it.
        let illuminant = unit_luminance(vec3<f32>(
            LIGHTING_MODEL_VISION_RED,
            LIGHTING_MODEL_VISION_GREEN,
            LIGHTING_MODEL_VISION_BLUE,
        ));
        return vec4<f32>(0.5 * chromatically_adapted_color(
            illuminant,
            EyeAdaptation(LIGHTING_MODEL_EXPOSURE_GAIN, illuminant),
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
        return vec4<f32>(0.25 * tone_map_scene_color(level * color, EyeAdaptation(1.0, vec3<f32>(1.0)), input.css_position_px), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 24u {
        return diagnostic_shadow_darkness(LIGHTING_MODEL_LIGHT_HEIGHT, LIGHTING_MODEL_CLOUD_DEPTH);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 27u || LIGHTING_MODEL_REGRESSION_OUTPUT == 28u {
        // Sweeps the Sun, with the Moon set, from 0.5 degrees above a
        // receiver's horizontal on the left to 2.5 degrees below it on the
        // right. Under a clear sky (27), red is its direct sunlight on a surface
        // facing it, in hundredths of a zenith Sun's clear-sky light, green that
        // light's red share, and blue its direct sunlight on level ground, in
        // two-thousandths. Under a full storm deck (28), red is its diffuse
        // light, in hundredths of a zenith Sun's clear-sky light, and green the
        // Sun's beam share of its clear-sky light on level ground.
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
                ambient.split.direct.x / max(
                    twilight_clear_sky_light(receiver.sun_direction.z, receiver.height_m),
                    1e-30,
                ),
                0.0,
                1.0,
            );
        }
        let split = calculate_atmospheric_ambient_light(RegionalCanopyConditions(0.0, 0.0), receiver)
            .split;
        let color = sun_direct_light_color(receiver);
        return vec4<f32>(
            100.0 * split.normal.x,
            color.r / (color.r + color.g + color.b),
            2000.0 * split.direct.x,
            1.0,
        );
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 29u {
        // Land relief under a Sun LIGHTING_MODEL_SUN_DEGREES high: red is the
        // direct light on a slope LIGHTING_MODEL_SLOPE_DEGREES steep facing the
        // Sun and green on one facing away, in quarters of level ground's, and
        // blue the sky's light on the slope facing it, in halves of level
        // ground's.
        let sun = radians(LIGHTING_MODEL_SUN_DEGREES);
        var receiver = celestial_receiver_light_at_height(0.0);
        receiver.sun_direction = vec3<f32>(cos(sun), 0.0, sin(sun));
        receiver.moon_direction = vec3<f32>(0.0, 0.0, -1.0);
        let gradient = vec2<f32>(tan(radians(LIGHTING_MODEL_SLOPE_DEGREES)), 0.0);
        let beams = vec2<f32>(1.0, 0.0);
        let level = landform_light_grade(vec2<f32>(0.0), 0.0, receiver, beams);
        let toward = landform_light_grade(-gradient, 0.0, receiver, beams);
        let away = landform_light_grade(gradient, 0.0, receiver, beams);
        return vec4<f32>(
            0.25 * toward.y / level.y,
            0.25 * away.y / level.y,
            0.5 * toward.x / level.x,
            1.0,
        );
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 30u {
        // The Sun's direct light on land relief under a Sun
        // LIGHTING_MODEL_SUN_DEGREES high, per unit of level ground's, in
        // eighths, and green the land.
        let initialized = initialize_playground_scene(input);
        let surface = scene_surface_at(
            initialized.input.world_position_m,
            initialized.surface_circle_position,
            tide_height_m(),
            initialized.atmosphere,
        );
        let sun = radians(LIGHTING_MODEL_SUN_DEGREES);
        var receiver = celestial_receiver_light_at_height(max(surface.elevation_m, 0.0));
        receiver.sun_direction = vec3<f32>(cos(sun), 0.0, sin(sun));
        receiver.moon_direction = vec3<f32>(0.0, 0.0, -1.0);
        let light = land_relief_light(surface, receiver, vec2<f32>(1.0, 0.0));
        return vec4<f32>(
            select(0.0, 0.125 * light.y / sin(sun), surface.is_land),
            select(0.0, 1.0, surface.is_land),
            0.0,
            1.0,
        );
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 32u {
        // Water's Sun glint at its rendered position under a Sun
        // LIGHTING_MODEL_SUN_DEGREES high toward +x, per unit of the beam's
        // light on level ground, as g / (g + 0.1) in red; the resolved surface
        // wind's speed U as U / (U + 10) in green and its axis from +x in half
        // turns in blue. Alpha is 1 over water and 0 elsewhere.
        let initialized = initialize_playground_scene(input);
        let position_m = initialized.input.world_position_m;
        let water_height_m = tide_height_m();
        let surface = scene_surface_at(
            position_m,
            initialized.surface_circle_position,
            water_height_m,
            initialized.atmosphere,
        );
        if surface.is_land {
            return vec4<f32>(0.0);
        }
        let sun = radians(LIGHTING_MODEL_SUN_DEGREES);
        let glint = water_glint(
            camera_view_direction(water_height_m),
            vec3<f32>(cos(sun), 0.0, sin(sun)),
            surface.surface.detail_gradient,
            unresolved_slope_covariance(surface.surface),
            CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
        ) / sin(sun);
        let wind = ocean_forcing_from_surface(
            position_m,
            surface.elevation_m,
            initialized.atmosphere,
            surface_tide_sample,
        ).surface_wind_m_s;
        let speed = length(wind);
        return vec4<f32>(
            glint / (glint + 0.1),
            speed / (speed + 10.0),
            fract(atan2(wind.y, wind.x) / PI + 1.0),
            1.0,
        );
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 34u {
        // The clear sky level water mirrors toward a camera overhead, per
        // unit of an isotropic sky's, in halves, under a clear sky with the
        // Moon set: in the left column with the Sun's disc fully up, two
        // radii above the horizon, and in the right fully set, two below.
        let height = select(2.0, -2.0, input.position.x >= 1.0) * CELESTIAL_DISC_ANGULAR_RADIUS_RAD;
        sun_light_direction = vec3<f32>(cos(height), 0.0, sin(height));
        geocentric_moon_light_direction = vec3<f32>(0.0, 0.0, -1.0);
        let receiver = celestial_receiver_light_at_height(0.0);
        let response = water_light_response(
            vec3<f32>(0.0),
            0.0,
            0.0,
            TerrainSurface(vec3<f32>(0.0), vec2<f32>(0.0), 0.0, vec2<f32>(0.0)),
            vec3<f32>(0.0, 0.0, 1.0),
            receiver,
            calculate_atmospheric_ambient_light(RegionalCanopyConditions(0.0, 0.0), receiver),
            vec3<f32>(0.0, 0.0, 1.0),
        );
        return vec4<f32>(0.5 * response.clear_sky.x / water_fresnel_reflectance(1.0), 0.0, 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 35u {
        // The squared slope water's surface resolves at its rendered position,
        // s², as s² / (s² + 0.001) in red. Alpha is 1 over water and 0
        // elsewhere.
        let initialized = initialize_playground_scene(input);
        let surface = scene_surface_at(
            initialized.input.world_position_m,
            initialized.surface_circle_position,
            tide_height_m(),
            initialized.atmosphere,
        );
        if surface.is_land {
            return vec4<f32>(0.0);
        }
        let slope_squared = dot(surface.surface.detail_gradient, surface.surface.detail_gradient);
        return vec4<f32>(slope_squared / (slope_squared + 0.001), 0.0, 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 38u {
        // Water's body per unit of the Sun's light on level ground, under a
        // Sun LIGHTING_MODEL_SUN_DEGREES high with the Moon set, seen
        // LIGHTING_MODEL_VIEW_DEGREES off the vertical through level water,
        // as b / (b + 0.05) in each channel. The columns lie 0, 1, 2, 5, 20
        // and 10 000 m deep, the top row over a black bed and the bottom over
        // a grey one of albedo 0.12. They hold LIGHTING_MODEL_SEDIMENT g/m³ of
        // sediment.
        var depths_m = array<f32, 6>(0.0, 1.0, 2.0, 5.0, 20.0, 10000.0);
        let depth_m = depths_m[min(u32(input.position.x), 5u)];
        let view = radians(LIGHTING_MODEL_VIEW_DEGREES);
        let body = water_body_response(
            vec3<f32>(select(0.0, 0.12, input.position.y >= 1.0)),
            depth_m,
            LIGHTING_MODEL_SEDIMENT,
            1.0 - water_fresnel_reflectance(cos(view)),
            cos(view),
            vec2<f32>(sin(radians(LIGHTING_MODEL_SUN_DEGREES)), -1.0),
            vec2<f32>(1.0),
        ).sun;
        return vec4<f32>(body / (body + 0.05), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 39u {
        // The Sun's light reaching the surface at the rendered position,
        // through the scene's cloud and landform shadows, on a white surface,
        // in scene light, as l / (l + 0.01) of its luminance in red.
        let initialized = initialize_playground_scene(input);
        let light = lit_scene_surface(
            initialized.input.world_position_m,
            initialized.atmosphere,
            initialized.surface_circle_position,
            true,
            vec4<f32>(1.0, 0.0, 0.0, 0.0),
            fragment_layer_visibility().y,
        );
        let luminance = dot(light, LUMINANCE_WEIGHTS);
        return vec4<f32>(luminance / (luminance + 0.01), 0.0, 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 37u {
        // The x component of `camera_view_direction` from the point the
        // pixel's eye ray meets, taken at sea level, as 0.5 + 0.5 x in red.
        _ = initialize_playground_scene(input);
        let view = camera_view_direction(0.0);
        return vec4<f32>(0.5 + 0.5 * view.x, 0.0, 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 33u {
        // The clear sky's radiance factor integrated over the sky, cosine
        // weighted, per π, in halves: 1 where its radiance carries exactly its
        // diffuse light. The light stands 5 degrees higher in each column,
        // from the horizon.
        let height = radians(5.0 * floor(input.position.x));
        let light = vec3<f32>(cos(height), 0.0, sin(height));
        var total = 0.0;
        for (var zenith_step = 0u; zenith_step < 64u; zenith_step++) {
            let zenith = (f32(zenith_step) + 0.5) * 0.5 * PI / 64.0;
            for (var azimuth_step = 0u; azimuth_step < 256u; azimuth_step++) {
                let azimuth = (f32(azimuth_step) + 0.5) * TAU / 256.0;
                let direction = vec3<f32>(
                    sin(zenith) * cos(azimuth),
                    sin(zenith) * sin(azimuth),
                    cos(zenith),
                );
                total += clear_sky_radiance_factor(direction, light) * cos(zenith) * sin(zenith);
            }
        }
        return vec4<f32>(0.5 * total * (0.5 * PI / 64.0) * (TAU / 256.0) / PI, 0.0, 0.0, 1.0);
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
