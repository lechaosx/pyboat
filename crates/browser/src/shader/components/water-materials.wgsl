fn visual_hash_unit(value: u32) -> f32 {
    return f32(value >> 8u) / 16777216.0;
}

fn precipitation_water_contact_sample_from_atmosphere(
    position_m: vec2<f32>,
    atmosphere: SurfaceAtmosphereSample,
) -> TerrainNoiseSample {
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    if effect_feature_visibility().x <= 0.0 {
        return sample;
    }
    let precipitation = surface_cloud_weather.rain;
    if precipitation <= 0.001 || visual_pixel_footprint_m >= 0.30 {
        return sample;
    }
    let cell_cycles = max(
        vec2<i32>(1),
        vec2<i32>(round(scene.world_size_m / 1.50)),
    );
    let cell_size_m = scene.world_size_m / vec2<f32>(cell_cycles);
    let canonical_position = canonical_world_position(position_m);
    let positive_position = select(
        canonical_position,
        canonical_position + scene.world_size_m,
        canonical_position < vec2<f32>(0.0),
    );
    let cell_position = positive_position
        * scene.inverse_world_size_m
        * vec2<f32>(cell_cycles);
    let base_cell = vec2<i32>(floor(cell_position));
    let cell_local = fract(cell_position);
    let cycles_per_anchor = 2400u;
    let anchor_cycles = multiply_u64(
        scene.precipitation_epoch,
        vec2<u32>(cycles_per_anchor, 0u),
    );
    let snow_fraction = 1.0 - smoothstep(
        -1.0,
        2.0,
        surface_current_air_temperature_c_from_atmosphere(
            atmosphere.temperature_anomaly.value,
            atmosphere.climate,
        ),
    );
    for (var offset_y = -1; offset_y <= 1; offset_y++) {
        for (var offset_x = -1; offset_x <= 1; offset_x++) {
            let unwrapped_cell = base_cell + vec2<i32>(offset_x, offset_y);
            let wrapped_cell = vec2<i32>(
                positive_mod(unwrapped_cell.x, cell_cycles.x),
                positive_mod(unwrapped_cell.y, cell_cycles.y),
            );
            let cell_hash = terrain_hash(
                scene.match_seed
                    ^ multiply_u64(
                        signed_u64(wrapped_cell.x),
                        vec2<u32>(0x243f6a88u, 0x85a308d3u),
                    )
                    ^ multiply_u64(
                        signed_u64(wrapped_cell.y),
                        vec2<u32>(0x13198a2eu, 0x03707344u),
                    ),
            );
            let local_cycle_time = scene.precipitation_phase_and_padding.x
                * f32(cycles_per_anchor)
                + visual_hash_unit(cell_hash.x);
            let local_cycle = u32(floor(local_cycle_time));
            let age = fract(local_cycle_time);
            let impact_low = anchor_cycles.x + local_cycle;
            let impact_epoch = vec2<u32>(
                impact_low,
                anchor_cycles.y + select(0u, 1u, impact_low < anchor_cycles.x),
            );
            let impact_hash = terrain_hash(
                cell_hash
                    ^ multiply_u64(
                        impact_epoch,
                        vec2<u32>(0x94d049bbu, 0x133111ebu),
                    ),
            );
            let presence = smoothstep(
                visual_hash_unit(impact_hash.x) - 0.10,
                visual_hash_unit(impact_hash.x) + 0.10,
                precipitation,
            );
            if presence <= 0.0 {
                continue;
            }
            let impact_position = 0.05 + 0.90 * vec2<f32>(
                visual_hash_unit(impact_hash.x ^ 0x9e3779b9u),
                visual_hash_unit(impact_hash.y ^ 0x7f4a7c15u),
            );
            let delta_m = (
                cell_local
                    - vec2<f32>(f32(offset_x), f32(offset_y))
                    - impact_position
            ) * cell_size_m;
            let distance_m = length(delta_m);
            let radial_direction = select(
                vec2<f32>(0.0),
                delta_m / distance_m,
                distance_m > 0.0001,
            );
            let life_envelope = sin(PI * age);
            let ring_radius_m = mix(0.018, 0.22, age);
            let ring_width_m = max(0.012, 0.55 * visual_pixel_footprint_m);
            let ring_offset = (distance_m - ring_radius_m) / ring_width_m;
            let rain_profile = exp(-ring_offset * ring_offset);
            let rain_contact = precipitation
                * presence
                * life_envelope
                * rain_profile;
            let rain_height_m = 0.010 * rain_contact;
            let rain_gradient = rain_height_m
                * (-2.0 * ring_offset / ring_width_m)
                * radial_direction;
            let snow_radius_m = max(0.035, 0.55 * visual_pixel_footprint_m);
            let snow_offset = distance_m / snow_radius_m;
            let snow_profile = exp(-snow_offset * snow_offset);
            let snow_contact = precipitation
                * presence
                * life_envelope
                * snow_profile;
            let snow_height_m = 0.0012 * snow_contact;
            let snow_gradient = snow_height_m
                * (-2.0 * snow_offset / snow_radius_m)
                * radial_direction;
            sample.value += mix(rain_contact, snow_contact, snow_fraction);
            sample.gradient += mix(rain_gradient, snow_gradient, snow_fraction);
        }
    }
    return sample;
}

fn precipitation_water_contact_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    return precipitation_water_contact_sample_from_atmosphere(
        position_m,
        surface_atmosphere_sample_at(position_m),
    );
}

fn ocean_tick_phase_radians(phase_per_tick_radians: f32) -> f32 {
    let tick = scene.world_tick_and_padding.xy;
    let chunks = array<u32, 4>(
        tick.y >> 16u,
        tick.y & 0xffffu,
        tick.x >> 16u,
        tick.x & 0xffffu,
    );
    var phase = 0.0;
    for (var chunk_index = 0u; chunk_index < 4u; chunk_index++) {
        phase = (phase * 65536.0 + f32(chunks[chunk_index]) * phase_per_tick_radians) % TAU;
    }
    return phase;
}

fn ocean_hash_unit(key: vec2<u32>) -> f32 {
    return (f32(terrain_hash(key).y >> 8u) + 0.5) / 16777216.0;
}

struct OceanForcingSample {
    current_m_s: vec2<f32>,
    significant_height_m: f32,
    surface_wind_m_s: vec2<f32>,
    upper_wind_m_s: vec2<f32>,
}

fn ocean_forcing_from_surface(
    position_m: vec2<f32>,
    terrain_cluster: TerrainNoiseSample,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
    surface_tide: SurfaceTideSample,
) -> OceanForcingSample {
    let latitude_rad = radians(atmosphere.climate.latitude_degrees);
    let cluster = terrain_cluster;
    let transition = clamp((cluster.value - 0.02) / 0.46, 0.0, 1.0);
    let open_water = 1.0 - transition * transition * (3.0 - 2.0 * transition);
    let open_water_derivative = select(
        -6.0 * transition * (1.0 - transition) / 0.46,
        0.0,
        transition == 0.0 || transition == 1.0,
    );
    let polar_capacity = pow(cos(latitude_rad), 2.0);
    let basin = open_water * polar_capacity;
    let basin_gradient = open_water_derivative * cluster.gradient * polar_capacity
        + vec2<f32>(
            0.0,
            -open_water * sin(2.0 * latitude_rad) * PI
                * scene.inverse_world_size_m.y,
        );
    let basin_squared = basin * basin;
    let basin_squared_gradient = 2.0 * basin * basin_gradient;

    let gyre = spatiotemporal_fractal_sample(
        vec2<u32>(0xc1059ed8u, 0xcbbb9d5du),
        position_m,
        0.25 * scene.world_size_m.x,
        scene.ocean_current_time_coordinate_and_padding.x,
        3u,
        0.5,
    );
    let hemisphere = sin(latitude_rad);
    let hemisphere_gradient_y_per_m = select(
        cos(latitude_rad) * PI * scene.inverse_world_size_m.y,
        0.0,
        abs(atmosphere.climate.latitude_degrees) >= 90.0,
    );
    let base_gradient = 5500.0
        * (hemisphere * (basin_squared * gyre.gradient + gyre.value * basin_squared_gradient)
            + vec2<f32>(
                0.0,
                basin_squared * gyre.value * hemisphere_gradient_y_per_m,
            ));

    let pressure = atmosphere.pressure;
    let pressure_envelope = basin_squared * hemisphere;
    let pressure_envelope_gradient = hemisphere * basin_squared_gradient
        + vec2<f32>(0.0, basin_squared * hemisphere_gradient_y_per_m);
    let weather_gradient = 0.006
        * (pressure_envelope * pressure.gradient_m_s
            + pressure.potential_m2_s * pressure_envelope_gradient);

    var tide = surface_tide;
    if tuning.terrain_tuning.x != 1.0 {
        tide = SurfaceTideSample(
            terrain_simplex_fractal_sample(
                vec2<u32>(0xf3bcc909u, 0x6a09e667u),
                position_m,
                131072.0,
                3u,
                0.5,
                3u,
            ),
            terrain_simplex_fractal_sample(
                vec2<u32>(0x84caa73bu, 0xbb67ae85u),
                position_m,
                131072.0,
                3u,
                0.5,
                3u,
            ),
        );
    }
    let tidal_range_m = 1.25 + 0.75 * tide.response.value;
    let tidal_range_gradient = 0.75 * tide.response.gradient;
    let tidal_phase = 0.08 * tide.lag.value;
    let tidal_phase_gradient = 0.08 * tide.lag.gradient;
    let solar_hour_angle = TAU * (atmosphere.climate.local_solar_phase - 0.5);
    let solar_longitude_phase = scene.world_phases_and_pixel_footprint_m.y - 0.25;
    let lunar_phase = scene.world_phases_and_pixel_footprint_m.z - solar_longitude_phase;
    let lunar_hour_angle = solar_hour_angle + TAU * lunar_phase;
    let lunar_angle = 2.0 * (lunar_hour_angle + TAU * tidal_phase);
    let solar_angle = 2.0 * (solar_hour_angle + TAU * tidal_phase);
    let tide_sum = cos(lunar_angle) + 0.46 * cos(solar_angle);
    let angle_gradient = vec2<f32>(
        TAU * scene.inverse_world_size_m.x,
        0.0,
    )
        + TAU * tidal_phase_gradient;
    let tide_sum_gradient = (-2.0 * sin(lunar_angle) - 0.92 * sin(solar_angle))
        * angle_gradient;
    let tide_height_m = 0.5 * tidal_range_m * tide_sum / 1.46;
    let tide_gradient = 0.5 / 1.46
        * (tide_sum * tidal_range_gradient + tidal_range_m * tide_sum_gradient);
    let tidal_potential_gradient = 1200.0
        * (basin * tide_gradient + tide_height_m * basin_gradient);
    let current_m_s = rotate_left(base_gradient + weather_gradient) - tidal_potential_gradient;

    let water_depth_m = max(tide_height_m - terrain_elevation_m, 0.0);
    let surface_wind_m_s = resolved_surface_wind_m_s_from_pressure(
        position_m,
        atmosphere.pressure,
    );
    let upper_wind_m_s = upper_wind_m_s_from_atmosphere(position_m, atmosphere);
    if water_depth_m == 0.0 {
        return OceanForcingSample(
            vec2<f32>(0.0),
            0.0,
            surface_wind_m_s,
            upper_wind_m_s,
        );
    }
    // Wave energy responds to sustained resolved flow, not an instantaneous
    // 256 m surface gust.
    let surface_speed_m_s = length(surface_wind_m_s);
    let upper_speed_m_s = length(upper_wind_m_s);
    let wind_sea_height_m = 0.05 + 0.016 * surface_speed_m_s * surface_speed_m_s;
    let swell_height_m = 0.006 * upper_speed_m_s * upper_speed_m_s;
    let unrestricted_height_m = length(vec2<f32>(wind_sea_height_m, swell_height_m));
    return OceanForcingSample(
        current_m_s,
        min(unrestricted_height_m, min(0.78 * water_depth_m, 14.0)),
        surface_wind_m_s,
        upper_wind_m_s,
    );
}

fn ocean_forcing_at(position_m: vec2<f32>) -> vec3<f32> {
    let terrain = procedural_terrain_sample(position_m);
    let forcing = ocean_forcing_from_surface(
        position_m,
        terrain.cluster,
        terrain.elevation_m,
        surface_atmosphere_sample_at(position_m),
        surface_tide_sample_at(position_m),
    );
    return vec3<f32>(forcing.current_m_s, forcing.significant_height_m);
}

fn physical_water_wave_sample_from_surface(
    position_m: vec2<f32>,
    terrain_cluster: TerrainNoiseSample,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
    tide: SurfaceTideSample,
) -> TerrainNoiseSample {
    if material_feature_visibility().y <= 0.0 {
        return TerrainNoiseSample(0.0, vec2<f32>(0.0));
    }
    let forcing = ocean_forcing_from_surface(
        position_m,
        terrain_cluster,
        terrain_elevation_m,
        atmosphere,
        tide,
    );
    let significant_height_m = forcing.significant_height_m;
    var sample: TerrainNoiseSample;
    sample.value = significant_height_m;
    sample.gradient = vec2<f32>(0.0);
    if significant_height_m <= 0.0 {
        return sample;
    }
    if material_feature_visibility().z <= 0.0 {
        return sample;
    }

    let surface_wind = forcing.surface_wind_m_s;
    let upper_wind = forcing.upper_wind_m_s;
    let surface_speed = length(surface_wind);
    let upper_speed = length(upper_wind);
    let surface_direction = surface_wind / max(surface_speed, 0.0001);
    let upper_direction = upper_wind / max(upper_speed, 0.0001);
    let wind_sea_height_m = 0.05 + 0.016 * surface_speed * surface_speed;
    let swell_height_m = 0.006 * upper_speed * upper_speed;
    let energy_direction = wind_sea_height_m * wind_sea_height_m * surface_direction
        + swell_height_m * swell_height_m * upper_direction;
    let energy_direction_length = length(energy_direction);
    let dominant_direction = select(
        vec2<f32>(1.0, 0.0),
        energy_direction / max(energy_direction_length, 0.0001),
        energy_direction_length > 0.0,
    );
    let rotation = TAU * ocean_hash_unit(
        scene.match_seed ^ vec2<u32>(0xfe94f82bu, 0x3c6ef372u),
    );
    let amplitude_scale = significant_height_m / 5.676266;
    var displacement_m = 0.0;

    for (var mode_index = 0u; mode_index < 16u; mode_index++) {
        let mode_key = scene.match_seed
            ^ vec2<u32>(0x299f31d0u, 0xa4093822u)
            ^ multiply_u64(
                vec2<u32>(mode_index, 0u),
                vec2<u32>(0x7f4a7c15u, 0x9e3779b9u),
            );
        let angle = rotation + TAU * f32(mode_index) / 16.0;
        let original_direction = vec2<f32>(cos(angle), sin(angle));
        let is_wind_sea = mode_index < 12u;
        let wavelength_fraction = ocean_hash_unit(
            mode_key ^ vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        );
        let minimum_wavelength_m = select(120.0, 8.0, is_wind_sea);
        let maximum_wavelength_m = select(600.0, 180.0, is_wind_sea);
        let wavelength_m = minimum_wavelength_m * pow(
            maximum_wavelength_m / minimum_wavelength_m,
            wavelength_fraction,
        );
        var mode_x = round(
            original_direction.x * scene.world_size_m.x / wavelength_m,
        );
        if mode_x == 0.0 && abs(original_direction.y) < 0.000001 {
            mode_x = select(1.0, -1.0, original_direction.x < 0.0);
        }
        let wave_number_x = TAU * mode_x
            * scene.inverse_world_size_m.x;
        let wave_number_y = TAU * original_direction.y / wavelength_m;
        let wave_number = length(vec2<f32>(wave_number_x, wave_number_y));
        let direction = vec2<f32>(wave_number_x, wave_number_y) / wave_number;
        let alignment = max(dot(direction, dominant_direction), 0.0);
        let weight = 0.20 + pow(alignment, 4.0);
        let amplitude_m = weight * amplitude_scale;
        let angular_frequency = sqrt(9.81 * wave_number);
        let phase = wave_number_x
            * fract(position_m.x * scene.inverse_world_size_m.x)
            * scene.world_size_m.x
            + wave_number_y * position_m.y
            - ocean_tick_phase_radians(angular_frequency / 60.0)
            + TAU * ocean_hash_unit(
                mode_key ^ vec2<u32>(0x1ce4e5b9u, 0xbf58476du),
            );
        let footprint_attenuation = exp(
            -0.5 * pow(wave_number * visual_pixel_footprint_m, 2.0),
        );
        displacement_m += footprint_attenuation * amplitude_m * cos(phase);
        let first_slope = -footprint_attenuation * amplitude_m * sin(phase) * wave_number;
        sample.gradient += first_slope * direction;
    }
    let nonlinear_strength = 0.5 * smoothstep(1.5, 3.0, significant_height_m);
    sample.gradient *= 1.0
        + 2.0 * nonlinear_strength * displacement_m / significant_height_m;
    return sample;
}

fn physical_water_wave_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    let terrain = procedural_terrain_sample(position_m);
    return physical_water_wave_sample_from_surface(
        position_m,
        terrain.cluster,
        terrain.elevation_m,
        surface_atmosphere_sample_at(position_m),
        surface_tide_sample_at(position_m),
    );
}

fn decorative_water_normal_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    var sample = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    if material_feature_visibility().w <= 0.0 {
        return sample;
    }
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = 18.0;
    var time_coordinate = scene.scalar_field_time_coordinates.w;
    for (var octave = 0u; octave < 4u; octave++) {
        let octave_visibility = smoothstep(
            visual_pixel_footprint_m,
            2.0 * visual_pixel_footprint_m,
            wavelength_m,
        );
        if octave_visibility > 0.0 {
            let octave_seed = scene.match_seed
                ^ vec2<u32>(0x6a09e667u, 0xbb67ae85u)
                ^ OCTAVE_SALTS[octave];
            let octave_sample = spatiotemporal_noise_sample(
                octave_seed,
                position_m,
                wavelength_m,
                time_coordinate,
            );
            sample.value += amplitude * octave_visibility * octave_sample.value;
            sample.gradient += amplitude * octave_visibility * octave_sample.gradient;
        }
        amplitude_sum += amplitude;
        amplitude *= 0.58;
        wavelength_m *= 0.5;
        time_coordinate *= 2.0;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
    sample.gradient *= 1.6;
    return sample;
}

fn visual_water_normal_sample_from_surface(
    position_m: vec2<f32>,
    terrain_cluster: TerrainNoiseSample,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
    tide: SurfaceTideSample,
) -> TerrainNoiseSample {
    let physical = physical_water_wave_sample_from_surface(
        position_m,
        terrain_cluster,
        terrain_elevation_m,
        atmosphere,
        tide,
    );
    let decorative = decorative_water_normal_sample(position_m);
    return TerrainNoiseSample(
        physical.value,
        tuning.water_surface.x * physical.gradient
            + tuning.water_surface.y * decorative.gradient,
    );
}

fn visual_water_normal_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    let terrain = procedural_terrain_sample(position_m);
    return visual_water_normal_sample_from_surface(
        position_m,
        terrain.cluster,
        terrain.elevation_m,
        surface_atmosphere_sample_at(position_m),
        surface_tide_sample_at(position_m),
    );
}

fn visual_water_surface(
    position_m: vec2<f32>,
    base_color: vec3<f32>,
    terrain_cluster: TerrainNoiseSample,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> TerrainSurface {
    let normal_detail = visual_water_normal_sample_from_surface(
        position_m,
        terrain_cluster,
        terrain_elevation_m,
        atmosphere,
        surface_tide_sample,
    );
    let precipitation_contact = precipitation_water_contact_sample_from_atmosphere(
        position_m,
        atmosphere,
    );
    let undisturbed_gradient = mix(
        normal_detail.gradient,
        0.35 * normal_detail.gradient,
        clamp(2.0 * precipitation_contact.value, 0.0, 1.0),
    );
    return TerrainSurface(
        base_color,
        undisturbed_gradient
            + tuning.water_surface.z * precipitation_contact.gradient,
        clamp(
            tuning.water_surface.w * mix(
                mix(0.08, 0.20, smoothstep(0.10, 4.0, normal_detail.value)),
                0.26,
                surface_cloud_weather.rain,
            ),
            0.0,
            1.0,
        ),
    );
}

fn terrain_surface(
    position_m: vec2<f32>,
    terrain_cluster: TerrainNoiseSample,
    elevation_m: f32,
    terrain_gradient: vec2<f32>,
    material_elevation_m: f32,
    material_gradient: vec2<f32>,
    coastal_rock_amount: f32,
    coastal_outcrop_amount: f32,
    coastal_sediment_amount: f32,
    water_height_m: f32,
    surface: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> TerrainSurface {
    let land_color_gate = land_material_color_visibility();
    let solid_color_gate = solid_material_color_visibility();
    let water_color_gate = water_material_color_visibility();
    if material_feature_visibility().x <= 0.0 {
        return TerrainSurface(
            select(vec3<f32>(0.16, 0.24, 0.12), vec3<f32>(0.025, 0.11, 0.15), elevation_m < water_height_m),
            vec2<f32>(0.0),
            1.0,
        );
    }
    let slope = length(material_gradient);
    if elevation_m < water_height_m {
        let depth_m = water_height_m - elevation_m;
        let water_color = vec3<f32>(0.025, 0.11, 0.15);
        let bottom_transmittance = exp(
            -2.0 * depth_m / max(tuning.water_tuning.x, 0.01),
        );
        if effect_feature_visibility().y > 0.0 && bottom_transmittance > 0.02 {
            let rock_amount = smoothstep(
                0.25,
                0.75,
                max(coastal_rock_amount, coastal_outcrop_amount),
            );
            let sediment_amount = smoothstep(0.08, 0.60, coastal_sediment_amount)
                * (1.0 - rock_amount);
            let sediment_reflectance = 0.84;
            let rock_reflectance = mix(
                0.56,
                0.72,
                coastal_outcrop_amount,
            );
            var bottom_reflectance = mix(0.32, sediment_reflectance, sediment_amount);
            bottom_reflectance = mix(bottom_reflectance, rock_reflectance, rock_amount);
            let bottom_water_color = water_color
                * (1.0 + tuning.water_tuning.z * bottom_reflectance);
            let water_veiled_bottom = mix(
                bottom_water_color,
                water_color,
                tuning.water_tuning.w * (1.0 - bottom_transmittance),
            );
            let bottom_visibility = clamp(
                tuning.water_tuning.y * bottom_transmittance,
                0.0,
                1.0,
            );
            let visible_water_color = water_color_gate.x * water_color
                + water_color_gate.y * bottom_visibility
                    * (water_veiled_bottom - water_color);
            return visual_water_surface(
                position_m,
                visible_water_color,
                terrain_cluster,
                elevation_m,
                atmosphere,
            );
        }
        return visual_water_surface(
            position_m,
            water_color_gate.x * water_color,
            terrain_cluster,
            elevation_m,
            atmosphere,
        );
    }

    var micro_height = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    if landscape_visibility.z > 0.0 {
        micro_height = material_fractal_sample(
            vec2<u32>(0xcbbb9d5du, 0x629a292au),
            position_m,
            2.5,
            6u,
        );
    }
    let surface_grade = clamp(
        0.5 + landscape_visibility.z
            * (0.40 * surface + 0.22 * micro_height.value),
        0.0,
        1.0,
    );
    var snow_amount = 0.0;
    if climate_visibility.x > 0.0 {
        snow_amount = smoothstep(
            0.10,
            0.55,
            local_snow_cover_from_atmosphere(
            material_elevation_m,
            atmosphere,
            ),
        );
        snow_amount = clamp(tuning.material_coverage.w * snow_amount, 0.0, 1.0);
    }
    let snow_surface = TerrainSurface(
        solid_color_gate.y * mix(
            vec3<f32>(0.72, 0.76, 0.77),
            vec3<f32>(0.96, 0.97, 0.96),
            0.55 + 0.20 * surface_grade,
        ),
        0.22 * tuning.land_material_normals.w * micro_height.gradient,
        0.70,
    );
    if snow_amount >= 1.0 {
        return snow_surface;
    }
    var vegetation = 0.0;
    if landscape_visibility.y > 0.0 {
        vegetation = material_fractal_value(
            vec2<u32>(0x510e527fu, 0x9b05688cu),
            position_m,
            1536.0,
            4u,
        );
    }
    let altitude_exposure = smoothstep(50.0, 200.0, material_elevation_m);
    let rock_slope_threshold = tuning.terrain_tuning.z
        * mix(3.0, 0.8, altitude_exposure)
        * mix(0.72, 1.28, smoothstep(-0.45, 0.45, surface));
    let alpine_rock_line_m = 110.0 + 45.0 * vegetation;
    let slope_rock = smoothstep(
        0.65 * rock_slope_threshold,
        1.35 * rock_slope_threshold,
        slope,
    );
    let outcrop_rock = smoothstep(0.05, 0.65, coastal_outcrop_amount);
    let alpine_rock = smoothstep(
        alpine_rock_line_m - 35.0,
        alpine_rock_line_m + 55.0,
        material_elevation_m,
    );
    let coastal_rock = smoothstep(0.10, 0.82, coastal_rock_amount);
    let rock_cover = clamp(
        tuning.material_coverage.y * (
            1.0
                - (1.0 - slope_rock)
                    * (1.0 - outcrop_rock)
                    * (1.0 - alpine_rock)
                    * (1.0 - coastal_rock)
        ),
        0.0,
        1.0,
    );

    let beach_sediment = smoothstep(0.08, 0.42, coastal_sediment_amount);
    let beach_bottom_m = mix(-0.15, -4.0, beach_sediment);
    let beach_top_m = mix(-0.15, 1.4, beach_sediment);
    let beach_micro_height_m = mix(
        0.42,
        0.10,
        smoothstep(0.25, 0.85, beach_sediment),
    );
    let beach_color = mix(
        vec3<f32>(0.48, 0.43, 0.33),
        vec3<f32>(0.84, 0.76, 0.50),
        clamp(0.18 + 0.62 * beach_sediment + 0.16 * surface_grade, 0.0, 1.0),
    );
    let beach_elevation = smoothstep(
        beach_bottom_m - 0.8,
        beach_bottom_m + 0.3,
        material_elevation_m,
    ) * (1.0 - smoothstep(
        beach_top_m - 0.2,
        beach_top_m + 1.5,
        material_elevation_m,
    ));
    let sediment_capacity = clamp(
        tuning.material_coverage.z
            * beach_sediment
            * beach_elevation
            * (1.0 - smoothstep(0.10, 0.30, slope)),
        0.0,
        1.0,
    );
    let intertidal_ceiling_m = 0.5 * tide_reference_range_m();
    let intertidal_capacity = 1.0 - smoothstep(
        intertidal_ceiling_m - 0.2,
        intertidal_ceiling_m + 1.0,
        material_elevation_m,
    );
    let exposed_dryness = smoothstep(
        0.05,
        0.8,
        material_elevation_m - water_height_m,
    );
    let intertidal_grade = clamp(
        0.20 + 0.42 * surface_grade + 0.22 * coastal_rock_amount,
        0.0,
        1.0,
    );
    let intertidal_color = mix(
        vec3<f32>(0.20, 0.22, 0.18),
        vec3<f32>(0.43, 0.40, 0.31),
        intertidal_grade,
    );
    let intertidal_surface = TerrainSurface(
        land_color_gate.z * mix(
            0.70 * intertidal_color,
            intertidal_color,
            exposed_dryness,
        ),
        mix(0.12, 0.28, coastal_rock_amount) * micro_height.gradient,
        mix(0.96, 0.88, exposed_dryness),
    );
    let beach_surface = TerrainSurface(
        land_color_gate.w * mix(0.72 * beach_color, beach_color, exposed_dryness),
        beach_micro_height_m * micro_height.gradient,
        mix(0.90, 0.82, exposed_dryness),
    );

    let tree_line = 1.0 - smoothstep(150.0, 250.0, material_elevation_m);
    let forest_amount = smoothstep(-0.18, 0.35, vegetation)
        * tree_line
        * (1.0 - smoothstep(0.04, 0.08, slope));
    var canopy_detail = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    var canopy_cluster = 0.0;
    if forest_amount > 0.0 && landscape_visibility.y > 0.0 {
        canopy_detail = material_fractal_sample(
            vec2<u32>(0x428a2f98u, 0x71374491u),
            position_m,
            56.0,
            3u,
        );
        canopy_cluster = smoothstep(
            -0.22,
            0.38,
            canopy_detail.value + 0.32 * vegetation,
        );
    }
    var grass_color = mix(
        vec3<f32>(0.09, 0.24, 0.10),
        vec3<f32>(0.32, 0.38, 0.17),
        0.20 * surface_grade
            + 0.20 * (0.5 + 0.5 * vegetation)
            + 0.18 * (0.5 + 0.5 * micro_height.value)
            + 0.42 * smoothstep(8.0, 150.0, material_elevation_m),
    );
    var forest_color = mix(
        vec3<f32>(0.03, 0.12, 0.06),
        vec3<f32>(0.09, 0.27, 0.11),
        0.18 + 0.34 * canopy_cluster + 0.18 * (0.5 + 0.5 * micro_height.value),
    );
    var autumn = 0.0;
    var dormancy = 0.0;
    if climate_visibility.w > 0.0 {
        autumn = autumn_vegetation_amount_from_anomaly(
            material_elevation_m,
            atmosphere.temperature_anomaly.value,
            atmosphere.climate,
        );
        dormancy = vegetation_dormancy_from_anomaly(
            material_elevation_m,
            atmosphere.temperature_anomaly.value,
            atmosphere.climate,
        );
    }
    grass_color = mix(grass_color, vec3<f32>(0.52, 0.28, 0.04), 0.62 * autumn);
    forest_color = mix(
        forest_color,
        vec3<f32>(0.62, 0.18, 0.025),
        0.80 * autumn * (0.45 + 0.55 * canopy_cluster),
    );
    grass_color = mix(grass_color, vec3<f32>(0.23, 0.22, 0.15), 0.42 * dormancy);
    forest_color = mix(forest_color, vec3<f32>(0.12, 0.13, 0.10), 0.32 * dormancy);
    let forest_visibility = clamp(
        tuning.material_coverage.x
            * forest_amount
            * canopy_cluster
            * landscape_visibility.y,
        0.0,
        1.0,
    );
    let vegetation_surface = TerrainSurface(
        land_color_gate.x * (1.0 - forest_visibility) * grass_color
            + land_color_gate.y * forest_visibility * forest_color,
        mix(
            0.45 * tuning.land_material_normals.y * micro_height.gradient,
            3.5 * tuning.land_material_normals.z * canopy_detail.gradient
                + 0.24 * tuning.land_material_normals.y * micro_height.gradient,
            forest_visibility,
        ),
        mix(0.88, 0.96, forest_visibility),
    );
    let non_rock_cover = 1.0 - rock_cover;
    let sediment_cover = non_rock_cover * sediment_capacity;
    let non_rock_sediment_cover = non_rock_cover - sediment_cover;
    let intertidal_cover = non_rock_sediment_cover * intertidal_capacity;
    let vegetation_cover = non_rock_sediment_cover - intertidal_cover;
    var rock_surface = vegetation_surface;
    if rock_cover > 0.0 {
        rock_surface = terrain_rock_surface(
            position_m,
            terrain_gradient,
            material_gradient,
            elevation_m,
        );
    }
    let composed_surface = TerrainSurface(
        vegetation_cover * vegetation_surface.color
            + intertidal_cover * intertidal_surface.color
            + sediment_cover * beach_surface.color
            + rock_cover * rock_surface.color,
        vegetation_cover * vegetation_surface.detail_gradient
            + intertidal_cover * intertidal_surface.detail_gradient
            + sediment_cover * beach_surface.detail_gradient
            + rock_cover * rock_surface.detail_gradient,
        vegetation_cover * vegetation_surface.roughness
            + intertidal_cover * intertidal_surface.roughness
            + sediment_cover * beach_surface.roughness
            + rock_cover * rock_surface.roughness,
    );
    return blend_terrain_surface(composed_surface, snow_surface, snow_amount);
}
