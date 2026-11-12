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

// Babin et al.'s (2003, J. Geophys. Res. 108, 3211) mineral sediment's
// absorption per g/m³, 0.041 e^(-0.0123 (λ - 443 nm)) m²/g, for which the
// water-body tests integrate their references.
const BABIN_SEDIMENT_ABSORPTION = WaterBands(
    vec4<f32>(0.043697, 0.026667, 0.019883, 0.013861),
    vec4<f32>(0.0090395, 0.0067426, 0.0048973, 0.0026619),
);

// Seawater holding `sediment_g_m3` of Babin et al.'s mineral sediment.
fn babin_mineral_sediment_optics(sediment_g_m3: f32) -> InherentOptics {
    return InherentOptics(
        WaterBands(
            WATER_ABSORPTION.low + sediment_g_m3 * BABIN_SEDIMENT_ABSORPTION.low,
            WATER_ABSORPTION.high + sediment_g_m3 * BABIN_SEDIMENT_ABSORPTION.high,
        ),
        WaterBands(
            WATER_BACKSCATTERING.low + sediment_g_m3 * SEDIMENT_BACKSCATTERING.low,
            WATER_BACKSCATTERING.high + sediment_g_m3 * SEDIMENT_BACKSCATTERING.high,
        ),
    );
}

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
            SEAWATER_OPTICS,
            TerrainSurface(vec3<f32>(0.0), vec2<f32>(0.0), 0.0, vec2<f32>(0.0)),
            vec3<f32>(0.0, 0.0, 1.0),
            receiver,
            calculate_atmospheric_ambient_light(RegionalCanopyConditions(0.0, 0.0), receiver),
            vec3<f32>(0.0, 0.0, 1.0),
            vec3<f32>(1.0),
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
            babin_mineral_sediment_optics(LIGHTING_MODEL_SEDIMENT),
            1.0 - water_fresnel_reflectance(cos(view)),
            cos(view),
            vec2<f32>(sin(radians(LIGHTING_MODEL_SUN_DEGREES)), -1.0),
            vec2<f32>(1.0),
            vec3<f32>(1.0),
        ).sun;
        return vec4<f32>(body / (body + 0.05), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 40u || LIGHTING_MODEL_REGRESSION_OUTPUT == 41u {
        // Wet level ground under a Sun 50° and a Moon 30° high, seen from
        // overhead. Columns hold bright sand, rock and grey material colours.
        // In 40, wet ground's light per unit of the body's of water 0.1 mm
        // deep over the same ground, as 0.5 + 50 (w / b - 1) in each channel:
        // its diffuse light in the top row, the Sun's in the middle and the
        // Moon's at the bottom. In 41, the top row is its sunlight per unit of
        // the beam's on level ground, as w / (w + 0.25), for linear greys of
        // 0.182 then 0.05, 0.3, 0.5 and 0.8, and the bottom row the same for
        // dry ground.
        var colors = array<vec3<f32>, 3>(
            linear_material_color(vec3<f32>(0.84, 0.76, 0.50)),
            linear_material_color(vec3<f32>(0.36, 0.34, 0.30)),
            vec3<f32>(0.18),
        );
        var greys = array<f32, 5>(0.182, 0.05, 0.3, 0.5, 0.8);
        let column = min(u32(input.position.x), 4u);
        let row = u32(input.position.y);
        let light_z = vec2<f32>(sin(radians(50.0)), sin(radians(30.0)));
        let relief = vec3<f32>(1.0, light_z);
        if LIGHTING_MODEL_REGRESSION_OUTPUT == 41u {
            let response = land_light_response(
                vec3<f32>(greys[column]),
                select(1.0, 0.0, row >= 1u),
                relief,
                1.0,
            );
            let sun = response.sun / light_z.x;
            return vec4<f32>(sun / (sun + 0.25), 1.0);
        }
        let albedo = colors[min(column, 2u)];
        let wet = land_light_response(albedo, 1.0, relief, 1.0);
        let body = water_body_response(
            albedo,
            1e-4,
            SEAWATER_OPTICS,
            1.0 - water_fresnel_reflectance(1.0),
            1.0,
            light_z,
            vec2<f32>(1.0),
            vec3<f32>(1.0),
        );
        var ratio = wet.clear_sky / body.diffuse;
        if row == 1u {
            ratio = wet.sun / (light_z.x * body.sun);
        } else if row >= 2u {
            ratio = wet.moon / (light_z.y * body.moon);
        }
        return vec4<f32>(0.5 + 50.0 * (ratio - 1.0), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 42u {
        // Clear sky's sunlit seconds from `clear_sky_sunlit_seconds` less a
        // numerical integral of max(0, μ₀) over 8192 steps, per day of zenith
        // sunlight, as 0.5 + 200 e in red. Columns hold latitude phases,
        // rows declinations of -23.44°, 0° and 23.44° for each of four spans
        // of hour angle.
        var latitudes = array<f32, 12>(
            -120.0, -89.0, -66.0, -45.0, -20.0, 0.0, 20.0, 45.0, 66.0, 80.0, 89.0, 120.0,
        );
        var spans = array<vec2<f32>, 4>(
            vec2<f32>(-1.0, 0.5),
            vec2<f32>(1.0, TAU + 0.3),
            vec2<f32>(-4.0, 3.5),
            vec2<f32>(2.5, 4.0),
        );
        let row = min(u32(input.position.y), 11u);
        let latitude = radians(latitudes[min(u32(input.position.x), 11u)]);
        let declination = radians(23.44 * (f32(row / 4u) - 1.0));
        let span = spans[row % 4u];
        var integral = 0.0;
        let steps = 8192u;
        let step = (span.y - span.x) / f32(steps);
        for (var index = 0u; index < steps; index++) {
            let hour_angle = span.x + (f32(index) + 0.5) * step;
            integral += max(
                sin(latitude) * sin(declination) + cos(latitude) * cos(declination) * cos(hour_angle),
                0.0,
            );
        }
        let closed = clear_sky_sunlit_seconds(latitude, declination, span.x, span.y);
        let error = (closed - integral * step * WORLD_DAY_SECONDS / TAU) / WORLD_DAY_SECONDS;
        return vec4<f32>(0.5 + 200.0 * error, 0.0, 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 43u {
        // The tide's wetness of ground at the view centre: columns step its
        // elevation by 3 cm from 2 m below mean sea level. The top row is sand,
        // the next rock, both of luminance 0.3, and the third that sand under
        // waves of 4 m and 11 s on a slope of 0.1. Red is its wet share, green
        // one where water covers it.
        let initialized = initialize_playground_scene(input);
        let elevation_m = -2.0 + 0.03 * floor(input.position.x);
        let covered = elevation_m < tide_height_m();
        let row = u32(input.position.y);
        let wet = tidal_wet_share(
            initialized.input.world_position_m,
            elevation_m,
            select(1.0, 0.0, row == 1u),
            select(0.0, wave_run_up_m(4.0, 9.81 * 11.0 * 11.0 / TAU, 0.1), row == 2u),
            0.3,
            ground_weather(initialized.input.world_position_m, 0.0, initialized.atmosphere)
                .air_temperature_c,
        );
        return vec4<f32>(select(wet, 1.0, covered), select(0.0, 1.0, covered), 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 44u {
        // Stockdon's run-up in whole millimetres, its high byte in red and
        // its low in green, eight bits each: under waves of 4 m and 11 s on
        // slopes of 0.1 and of 0.01 in the two columns.
        let slope = select(0.1, 0.01, input.position.x >= 1.0);
        let millimetres = u32(round(1000.0 * wave_run_up_m(4.0, 9.81 * 11.0 * 11.0 / TAU, slope)));
        return vec4<f32>(linear_material_color(vec3<f32>(
            f32(millimetres / 256u) / 255.0,
            f32(millimetres % 256u) / 255.0,
            0.0,
        )), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 45u {
        // The weather at each pixel that calibrates the rain-capable canopy:
        // the regional canopy's cover in red, its optical depth τ as
        // (ln τ + 4) / 9 in green, and one in blue where it rains.
        let initialized = initialize_playground_scene(input);
        let canopy = regional_canopy_conditions(surface_cloud_weather);
        return vec4<f32>(
            canopy.sky_fraction,
            clamp((log(max(canopy.mean_optical_thickness, 1e-6)) + 4.0) / 9.0, 0.0, 1.0),
            select(0.0, 1.0, surface_cloud_weather.rain > 0.0),
            1.0,
        );
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 46u {
        // Weather's wet share of ground of luminance 0.3 at the view centre
        // and clock, in red, by column: clear dry air at 20 °C and half its
        // saturation; fog, of total water 5% beyond saturation; a thick deck
        // of optical depth 40 over the dry air; that deck over fog at -5 °C;
        // clear air at nine tenths of saturation; and that air under a full
        // veil of optical depth 0.5.
        let initialized = initialize_playground_scene(input);
        var temperatures = array<f32, 6>(20.0, 20.0, 20.0, -5.0, 20.0, 20.0);
        var humidities = array<f32, 6>(0.5, 1.05, 0.5, 1.05, 0.9, 0.9);
        var canopies = array<vec2<f32>, 6>(
            vec2<f32>(0.0),
            vec2<f32>(0.0),
            vec2<f32>(1.0, 40.0),
            vec2<f32>(1.0, 40.0),
            vec2<f32>(0.0),
            vec2<f32>(1.0, 0.5),
        );
        let case_index = min(u32(input.position.x), 5u);
        let temperature = temperatures[case_index];
        let wet = weather_wet_share(
            initialized.input.world_position_m,
            humidities[case_index]
                * water_vapor_saturation_capacity_g_kg(temperature, SEA_LEVEL_PRESSURE_HPA),
            temperature,
            RegionalCanopyConditions(canopies[case_index].x, canopies[case_index].y),
            0.3,
        );
        return vec4<f32>(wet, 0.0, 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 47u {
        // The weather's wet share of ground of luminance 0.3 at each pixel in
        // red, and the rain-capable share of its canopy in green.
        let initialized = initialize_playground_scene(input);
        let atmosphere = initialized.atmosphere;
        let canopy = regional_canopy_conditions(surface_cloud_weather);
        let air_temperature_c = surface_current_air_temperature_c_from_atmosphere(
            atmosphere.temperature_anomaly.value,
            atmosphere.climate,
        );
        let wet = weather_wet_share(
            initialized.input.world_position_m,
            atmosphere.water_vapor_g_kg,
            air_temperature_c,
            canopy,
            0.3,
        );
        let rain_capable = weather_wet_share(
            initialized.input.world_position_m,
            0.0,
            20.0,
            canopy,
            0.3,
        );
        return vec4<f32>(wet, rain_capable, 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 48u {
        // The seabed's mobility, as 65535 times the share in 16 bits, its
        // high byte in red and its low in green: sand by `seabed_mobility`,
        // then by integrating its wind sea's motion over 4096 quantiles of
        // the wind climate, then fine sediment alike, in the first four rows.
        // Columns step the depth by sixteenths of an octave from 0.25 m, so
        // even columns fall on the table's depths and odd ones between them.
        // The fifth row holds Komar and Miller's sand threshold at 6 s in
        // 0.1 mm/s; the orbital speed at beds 20 and 30 m down under a 4 m sea
        // peaking at 2π U² / (0.769 g) for U = 15 m/s, in 0.1 mm/s; and the
        // wind climate's medians, 90th and 99th percentiles in mm/s.
        let column = floor(input.position.x);
        let row = u32(input.position.y);
        var value = 0.0;
        if row < 4u {
            let depth_m = exp2(column / 16.0 - 2.0);
            let fine = row >= 2u;
            if row % 2u == 0u {
                value = select(seabed_mobility(depth_m).x, seabed_mobility(depth_m).y, fine);
            } else {
                var moved = 0.0;
                for (var index = 0u; index < 4096u; index++) {
                    let quantile = (f32(index) + 0.5) / 4096.0;
                    let wind_m_s = WIND_CLIMATE_WEIBULL.y
                        * pow(-log(1.0 - quantile), 1.0 / WIND_CLIMATE_WEIBULL.x);
                    let wavelength_m = wave_peak_wavelength_m(wind_sea(wind_m_s));
                    let speed_m_s = wave_orbital_speed_at_bed_m_s(
                        significant_wave_height_m(wind_sea(wind_m_s), 0.0, depth_m),
                        wavelength_m,
                        depth_m,
                    );
                    let threshold_m_s = select(
                        sand_threshold_orbital_speed_m_s(wave_period_s(wavelength_m)),
                        FINE_SEDIMENT_THRESHOLD_ORBITAL_SPEED_M_S,
                        fine,
                    );
                    moved += select(0.0, 1.0, speed_m_s > threshold_m_s);
                }
                value = moved / 4096.0;
            }
            value *= 65535.0;
        } else {
            var quantiles = array<f32, 3>(0.5, 0.9, 0.99);
            let storm_wavelength_m = TAU * 15.0 * 15.0 / (0.769 * 9.81);
            if column < 1.0 {
                value = 1e4 * sand_threshold_orbital_speed_m_s(6.0);
            } else if column < 3.0 {
                value = 1e4 * wave_orbital_speed_at_bed_m_s(
                    4.0,
                    storm_wavelength_m,
                    select(20.0, 30.0, column >= 2.0),
                );
            } else {
                value = 1e3 * WIND_CLIMATE_WEIBULL.y
                    * pow(-log(1.0 - quantiles[min(u32(column) - 3u, 2u)]), 1.0 / WIND_CLIMATE_WEIBULL.x);
            }
        }
        let units = u32(round(clamp(value, 0.0, 65535.0)));
        return vec4<f32>(linear_material_color(vec3<f32>(
            f32(units / 256u) / 255.0,
            f32(units % 256u) / 255.0,
            0.0,
        )), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 49u {
        // The sand cover of ground's dry composition at the view centre,
        // without coastal rock, in red: columns step its elevation by 10 cm
        // down from 0.1 m, and rows take coastal sediment amounts of 0.05,
        // 0.2, 0.3 and 0.5 on level ground, then 0.3 on slopes of 0.3 and
        // 0.75.
        let initialized = initialize_playground_scene(input);
        var amounts = array<f32, 6>(0.05, 0.2, 0.3, 0.5, 0.3, 0.3);
        var slopes = array<f32, 6>(0.0, 0.0, 0.0, 0.0, 0.3, 0.75);
        let row = min(u32(input.position.y), 5u);
        surface_baking = true;
        let elevation_m = 0.1 - 0.1 * floor(input.position.x);
        let atmosphere = initialized.atmosphere;
        _ = terrain_surface(
            initialized.input.world_position_m,
            elevation_m,
            vec2<f32>(slopes[row], 0.0),
            elevation_m,
            vec2<f32>(slopes[row], 0.0),
            0.0,
            0.0,
            amounts[row],
            -1.0e6,
            0.0,
            atmosphere,
        );
        surface_baking = false;
        return vec4<f32>(surface_bake_factors.sand_cover, 0.0, 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 50u {
        // Fine sediment the wind sea stirs, alone, without swell. In the top
        // row, in 0.01 g/m³, its high byte in red and its low in green: over
        // beds all sand 2, 3, 5, 7 and 10 m deep, under the wind climate's
        // median wind, then under 2.9 m/s; over a bed of fine sediment alone
        // 3 m deep under waves stressing it by 0.3 Pa; that bed 20 m deep
        // under the climate's 99th percentile wind, then its median; a sand
        // bed 10 km deep under the 99th percentile; and a bare rock bed 5 m
        // deep under it. Below, in 0.1 g/m², what a bed of fine sediment alone
        // gives the column above it, by columns 2^(i/2 - 1) m deep, under
        // winds of 0, 3, 6, 9, 12, 15, 20 and 25 m/s by rows.
        let column = u32(input.position.x);
        let row = u32(input.position.y);
        let median_m_s = WIND_CLIMATE_WEIBULL.y * pow(log(2.0), 1.0 / WIND_CLIMATE_WEIBULL.x);
        let storm_m_s = WIND_CLIMATE_WEIBULL.y * pow(log(100.0), 1.0 / WIND_CLIMATE_WEIBULL.x);
        var value = 0.0;
        if row == 0u {
            var depths_m = array<f32, 15>(
                2.0, 3.0, 5.0, 7.0, 10.0, 2.0, 3.0, 5.0, 7.0, 10.0, 3.0, 20.0, 20.0, 10000.0, 5.0,
            );
            var winds_m_s = array<f32, 15>(
                median_m_s, median_m_s, median_m_s, median_m_s, median_m_s,
                2.9, 2.9, 2.9, 2.9, 2.9, 0.0, storm_m_s, median_m_s, storm_m_s, storm_m_s,
            );
            let case_index = min(column, 14u);
            let depth_m = depths_m[case_index];
            let wind_m_s = winds_m_s[case_index];
            var speed_m_s = wave_orbital_speed_at_bed_m_s(
                significant_wave_height_m(wind_sea(wind_m_s), 0.0, depth_m),
                wave_peak_wavelength_m(wind_sea(wind_m_s)),
                depth_m,
            );
            var availability = fine_sediment_availability(select(1.0, 0.0, case_index == 14u), depth_m);
            if case_index >= 10u && case_index <= 12u {
                availability = 1.0;
            }
            if case_index == 10u {
                speed_m_s = sqrt(0.3 / (0.5 * SEAWATER_DENSITY_KG_M3 * WAVE_FRICTION_FACTOR));
            }
            value = 100.0 * stirred_fine_sediment_g_m3(availability, speed_m_s, depth_m);
        } else {
            var winds_m_s = array<f32, 8>(0.0, 3.0, 6.0, 9.0, 12.0, 15.0, 20.0, 25.0);
            let wind_m_s = winds_m_s[min(row - 1u, 7u)];
            let depth_m = exp2(f32(column) / 2.0 - 1.0);
            let speed_m_s = wave_orbital_speed_at_bed_m_s(
                significant_wave_height_m(wind_sea(wind_m_s), 0.0, depth_m),
                wave_peak_wavelength_m(wind_sea(wind_m_s)),
                depth_m,
            );
            value = 10.0 * depth_m * stirred_fine_sediment_g_m3(1.0, speed_m_s, depth_m);
        }
        let units = u32(round(clamp(value, 0.0, 65535.0)));
        return vec4<f32>(linear_material_color(vec3<f32>(
            f32(units / 256u) / 255.0,
            f32(units % 256u) / 255.0,
            0.0,
        )), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 51u {
        // The light a bed of bright beach sand, all sand, adds to the body of
        // water 2, 3 and 5 m deep by column, under the Sun and view overhead,
        // holding the sediment the wind climate's median wind stirs from it,
        // per unit of what it adds through clear seawater, in red. The bed
        // adds its body's luminance over it less that over a black bed.
        var depths_m = array<f32, 3>(2.0, 3.0, 5.0);
        let depth_m = depths_m[min(u32(input.position.x), 2u)];
        let median_m_s = WIND_CLIMATE_WEIBULL.y * pow(log(2.0), 1.0 / WIND_CLIMATE_WEIBULL.x);
        let sand = linear_material_color(vec3<f32>(0.84, 0.76, 0.50));
        let sediment_g_m3 = stirred_fine_sediment_g_m3(
            fine_sediment_availability(1.0, depth_m),
            wave_orbital_speed_at_bed_m_s(
                significant_wave_height_m(wind_sea(median_m_s), 0.0, depth_m),
                wave_peak_wavelength_m(wind_sea(median_m_s)),
                depth_m,
            ),
            depth_m,
        );
        var added = vec2<f32>(0.0);
        for (var index = 0u; index < 2u; index++) {
            var inherent = SEAWATER_OPTICS;
            if index == 0u {
                inherent = stirred_sediment_optics(sand, sediment_g_m3);
            }
            var luminance = vec2<f32>(0.0);
            for (var bed_index = 0u; bed_index < 2u; bed_index++) {
                let body = water_body_response(
                    select(vec3<f32>(0.0), sand, bed_index == 0u),
                    depth_m,
                    inherent,
                    1.0 - water_fresnel_reflectance(1.0),
                    1.0,
                    vec2<f32>(1.0, -1.0),
                    vec2<f32>(1.0),
                    vec3<f32>(1.0),
                ).sun;
                luminance[bed_index] = dot(body, LUMINANCE_WEIGHTS);
            }
            added[index] = luminance.x - luminance.y;
        }
        return vec4<f32>(added.x / added.y, 0.0, 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 52u {
        // Stirred sediment's absorption per unit of its backscattering, r, as
        // r / (r + 1) in red, in each of the eight bands by column: over a
        // bed of Babin et al.'s mineral colour, then of 30% grey.
        let color = select(vec3<f32>(0.48, 0.25, 0.11), vec3<f32>(0.3), input.position.y >= 1.0);
        let stirred = stirred_sediment_optics(color, 1.0);
        let band = min(u32(input.position.x), 7u);
        var absorption = stirred.absorption.low - WATER_ABSORPTION.low;
        var backscattering = stirred.backscattering.low - WATER_BACKSCATTERING.low;
        if band >= 4u {
            absorption = stirred.absorption.high - WATER_ABSORPTION.high;
            backscattering = stirred.backscattering.high - WATER_BACKSCATTERING.high;
        }
        let ratio = absorption[band % 4u] / backscattering[band % 4u];
        return vec4<f32>(ratio / (ratio + 1.0), 0.0, 0.0, 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 53u {
        // The fine sediment the waves hold at each pixel's rendered position
        // beneath water, in 0.01 g/m³, its high byte in red and its low in
        // green, with alpha 1 over water and 0 elsewhere.
        let initialized = initialize_playground_scene(input);
        let water_height_m = tide_height_m();
        let surface = scene_surface_at(
            initialized.input.world_position_m,
            initialized.surface_circle_position,
            water_height_m,
            initialized.atmosphere,
        );
        if surface.is_land {
            return vec4<f32>(0.0);
        }
        let sediment_g_m3 = scene_stirred_sediment_g_m3(surface, water_height_m);
        let units = u32(round(clamp(100.0 * sediment_g_m3, 0.0, 65535.0)));
        return vec4<f32>(linear_material_color(vec3<f32>(
            f32(units / 256u) / 255.0,
            f32(units % 256u) / 255.0,
            0.0,
        )), 1.0);
    }
    if LIGHTING_MODEL_REGRESSION_OUTPUT == 54u {
        // The water's surface at points (53, 31) m apart from the view
        // centre, three columns to a point: its height in mm, then its slope
        // along x and y in units of 10⁻⁴, each offset by 32768, its high byte
        // in red and its low in green.
        _ = initialize_playground_scene(input);
        let column = u32(input.position.x);
        let point = f32(column / 3u);
        let waves = physical_water_wave_sample(
            scene.camera_and_view_m.xy + point * vec2<f32>(53.0, 31.0),
        );
        var values = array<f32, 3>(
            1000.0 * waves.displacement_m,
            1.0e4 * waves.gradient.x,
            1.0e4 * waves.gradient.y,
        );
        let value = values[column % 3u];
        let units = u32(round(clamp(value + 32768.0, 0.0, 65535.0)));
        return vec4<f32>(linear_material_color(vec3<f32>(
            f32(units / 256u) / 255.0,
            f32(units % 256u) / 255.0,
            0.0,
        )), 1.0);
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
