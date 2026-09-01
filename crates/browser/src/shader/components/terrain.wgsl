fn terrain_smoothstep_sample(
    lower: f32,
    upper: f32,
    input: TerrainNoiseSample,
) -> TerrainNoiseSample {
    let normalized = (input.value - lower) / (upper - lower);
    let amount = clamp(normalized, 0.0, 1.0);
    var sample: TerrainNoiseSample;
    sample.value = amount * amount * (3.0 - 2.0 * amount);
    sample.gradient = vec2<f32>(0.0);
    if normalized > 0.0 && normalized < 1.0 {
        sample.gradient = input.gradient * 6.0 * amount * (1.0 - amount) / (upper - lower);
    }
    return sample;
}

fn solid_ice_sample(
    terrain: TerrainSample,
    maritime_temperature: TerrainNoiseSample,
    climate: SurfaceClimateSample,
    position_m: vec2<f32>,
) -> TerrainSample {
    var surface = terrain;
    let maritime_scale = 16.0 * (
        1.0 - smoothstep(75.0, 90.0, abs(climate.latitude_degrees))
    );
    surface.maritime_temperature_anomaly_c = maritime_scale * maritime_temperature.value;
    var bedrock: TerrainNoiseSample;
    bedrock.value = terrain.elevation_m;
    bedrock.gradient = terrain.gradient;
    if bedrock.value < 0.0 {
        var effective_water_temperature: TerrainNoiseSample;
        effective_water_temperature.value = water_temperature_c_from_climate(climate)
            + maritime_scale * maritime_temperature.value;
        effective_water_temperature.gradient = maritime_scale * maritime_temperature.gradient;
        let open_water = terrain_smoothstep_sample(-2.0, 0.0, effective_water_temperature);
        let cover = 1.0 - open_water.value;
        if cover <= 0.5 {
            return surface;
        }
        surface.elevation_m = 0.25 + 1.75 * cover;
        surface.gradient = -1.75 * open_water.gradient;
        surface.ice_amount = smoothstep(0.5, 0.65, cover);
        surface.water_ice_amount = cover;
        surface.bedrock_elevation_m = bedrock.value;
        return surface;
    }
    var positive_bedrock: TerrainNoiseSample;
    positive_bedrock.value = max(bedrock.value, 0.0);
    positive_bedrock.gradient = select(vec2<f32>(0.0), bedrock.gradient, bedrock.value > 0.0);
    var annual_temperature: TerrainNoiseSample;
    annual_temperature.value = climate.annual_mean_air_temperature_c
        - 0.0065 * positive_bedrock.value;
    annual_temperature.gradient = vec2<f32>(
        0.0,
        latitude_curve_gradient_y_per_m_at(CLIMATE_ANNUAL_MEAN_C, position_m),
    )
        - 0.0065 * positive_bedrock.gradient;
    if annual_temperature.value >= -8.0 {
        return surface;
    }
    let warm = terrain_smoothstep_sample(-12.0, -8.0, annual_temperature);
    var cold: TerrainNoiseSample;
    cold.value = 1.0 - warm.value;
    cold.gradient = -warm.gradient;
    let amount = cold.value;
    let amount_gradient = cold.gradient;
    let broad_surface = 20.0 * cold.value + 0.25 * positive_bedrock.value;
    let broad_gradient = 20.0 * cold.gradient + 0.25 * positive_bedrock.gradient;
    let local_surface = bedrock.value + 4.0 * cold.value;
    let local_gradient = bedrock.gradient + 4.0 * cold.gradient;
    let ice_surface = max(broad_surface, local_surface);
    let ice_gradient = select(
        local_gradient,
        broad_gradient,
        broad_surface >= local_surface,
    );
    let candidate_elevation = mix(bedrock.value, ice_surface, amount);
    let candidate_gradient = mix(bedrock.gradient, ice_gradient, amount)
        + amount_gradient * (ice_surface - bedrock.value);
    let thickness = max(candidate_elevation - bedrock.value, 0.0);
    surface.elevation_m = max(bedrock.value, candidate_elevation);
    surface.gradient = select(
        bedrock.gradient,
        candidate_gradient,
        candidate_elevation > bedrock.value,
    );
    surface.ice_amount = smoothstep(0.1, 2.0, thickness);
    return surface;
}

fn procedural_terrain_sample_with_climate_from_circle(
    position_m: vec2<f32>,
    terrain_circle_position: vec4<f32>,
    climate: SurfaceClimateSample,
) -> TerrainSample {
    // Keep the density composition synchronized with
    // `game::procedural_terrain_elevation_m`; Rust remains authoritative
    // for physical depth.
    let terrain_position_m = position_m;
    let cluster = terrain_simplex_fractal_sample_from_circle(
        vec2<u32>(0x78bd642fu, 0xa0761d64u),
        terrain_position_m,
        terrain_circle_position,
        8192.0,
        3u,
        0.5,
        TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
    );
    let island = terrain_simplex_fractal_sample_from_circle(
        vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        terrain_position_m,
        terrain_circle_position,
        1536.0,
        5u,
        0.35,
        TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT,
    );
    var unscaled: TerrainNoiseSample;
    unscaled.value = 18.0 * (0.78 * cluster.value + 0.22 * island.value - 0.30);
    unscaled.gradient = 18.0 * (0.78 * cluster.gradient + 0.22 * island.gradient);
    let island_ruggedness = terrain_smoothstep_sample(0.32, 0.58, cluster);
    var negative_cluster: TerrainNoiseSample;
    negative_cluster.value = -cluster.value;
    negative_cluster.gradient = -cluster.gradient;
    let trench_depth = terrain_smoothstep_sample(0.50, 0.74, negative_cluster);
    let elevation_scale = tuning.terrain_tuning.y
        * (1.0 + 19.0 * island_ruggedness.value + 29.0 * trench_depth.value);
    var base: TerrainNoiseSample;
    base.value = unscaled.value * elevation_scale;
    base.gradient = unscaled.gradient * elevation_scale
        + unscaled.value
            * tuning.terrain_tuning.y
            * (19.0 * island_ruggedness.gradient + 29.0 * trench_depth.gradient);

    var base_depth: TerrainNoiseSample;
    base_depth.value = -base.value;
    base_depth.gradient = -base.gradient;
    var reef_ruggedness: TerrainNoiseSample;
    reef_ruggedness.value = 0.0;
    reef_ruggedness.gradient = vec2<f32>(0.0);
    var coastal_rock: TerrainNoiseSample;
    coastal_rock.value = 0.0;
    coastal_rock.gradient = vec2<f32>(0.0);
    if landscape_visibility.x > 0.0 && base.value > -32.0 && base.value < 4.0 {
        let reef_ruggedness_noise = terrain_simplex_fractal_sample_from_circle(
            vec2<u32>(0x1ce4e5b9u, 0xbf58476du),
            terrain_position_m,
            terrain_circle_position,
            2048.0,
            3u,
            0.5,
            TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
        );
        reef_ruggedness = terrain_smoothstep_sample(
            -0.25,
            0.25,
            reef_ruggedness_noise,
        );
        let coastal_substrate = terrain_simplex_fractal_sample_from_circle(
            vec2<u32>(0x7f4a7c15u, 0x9e3779b9u),
            terrain_position_m,
            terrain_circle_position,
            1536.0,
            3u,
            0.5,
            TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
        );
        coastal_rock = terrain_smoothstep_sample(-0.25, 0.35, coastal_substrate);
    }
    var coastal_sediment: TerrainNoiseSample;
    coastal_sediment.value = (1.0 - coastal_rock.value)
        * (1.0 - reef_ruggedness.value);
    coastal_sediment.gradient = -coastal_rock.gradient
        * (1.0 - reef_ruggedness.value)
        - reef_ruggedness.gradient * (1.0 - coastal_rock.value);
    var ocean_floor_depth: TerrainNoiseSample;
    ocean_floor_depth.value = 3000.0 + 3000.0 * trench_depth.value;
    ocean_floor_depth.gradient = 3000.0 * trench_depth.gradient;
    let positive_base_depth_m = max(base_depth.value, 0.0);
    var shaped_depth: TerrainNoiseSample;
    shaped_depth.value = base_depth.value
        + 0.015 * positive_base_depth_m * positive_base_depth_m
            * positive_base_depth_m * positive_base_depth_m;
    shaped_depth.gradient = base_depth.gradient;
    if positive_base_depth_m > 0.0 {
        shaped_depth.gradient *= 1.0
            + 0.06 * positive_base_depth_m * positive_base_depth_m
                * positive_base_depth_m;
    }
    let deepened_depth_m = min(
        shaped_depth.value,
        ocean_floor_depth.value,
    );
    let deepened_depth_gradient = select(
        ocean_floor_depth.gradient,
        shaped_depth.gradient,
        shaped_depth.value <= ocean_floor_depth.value,
    );

    var terrain: TerrainSample;
    terrain.cluster = cluster;
    terrain.elevation_m = -deepened_depth_m;
    terrain.gradient = -deepened_depth_gradient;
    terrain.material_elevation_m = terrain.elevation_m;
    terrain.material_gradient = terrain.gradient;
    terrain.ice_amount = 0.0;
    terrain.water_ice_amount = 0.0;
    terrain.bedrock_elevation_m = terrain.elevation_m;
    terrain.maritime_temperature_anomaly_c = 0.0;
    terrain.coastal_rock_amount = 0.0;
    terrain.coastal_outcrop_amount = 0.0;
    terrain.coastal_sediment_amount = 0.0;
    let land_detail_mask = terrain_smoothstep_sample(4.0, 12.0, base);
    if land_detail_mask.value > 0.0 {
        let land_ruggedness_noise = terrain_simplex_fractal_sample_from_circle(
            vec2<u32>(0x1ce4e5b9u, 0xbf58476du),
            terrain_position_m,
            terrain_circle_position,
            2048.0,
            3u,
            0.5,
            TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
        );
        let land_ruggedness = terrain_smoothstep_sample(
            -0.25,
            0.25,
            land_ruggedness_noise,
        );
        let land_detail = terrain_simplex_fractal_sample_from_circle(
            vec2<u32>(0x4f6cdd1du, 0x2545f491u),
            terrain_position_m,
            terrain_circle_position,
            256.0,
            6u,
            0.65,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        );
        var ridged_detail: TerrainNoiseSample;
        ridged_detail.value = 1.0 - 2.0 * abs(land_detail.value);
        ridged_detail.gradient = -2.0
            * select(-1.0, 1.0, land_detail.value >= 0.0)
            * land_detail.gradient;
        let ridge_blend = terrain_smoothstep_sample(0.35, 0.75, land_ruggedness);
        var shaped_land_detail: TerrainNoiseSample;
        shaped_land_detail.value = mix(
            land_detail.value,
            ridged_detail.value,
            ridge_blend.value,
        );
        shaped_land_detail.gradient = mix(
            land_detail.gradient,
            ridged_detail.gradient,
            ridge_blend.value,
        ) + ridge_blend.gradient * (ridged_detail.value - land_detail.value);
        let land_detail_amount = 0.08 + 0.92 * land_ruggedness.value;
        terrain.elevation_m += 5.0 * land_detail_mask.value
            * land_detail_amount * shaped_land_detail.value * landscape_visibility.x;
        terrain.gradient += 5.0 * landscape_visibility.x * (
            land_detail_mask.gradient * land_detail_amount * shaped_land_detail.value
                + land_ruggedness.gradient * 0.92
                    * land_detail_mask.value * shaped_land_detail.value
                + shaped_land_detail.gradient
                    * land_detail_mask.value * land_detail_amount
        );
        terrain.material_elevation_m = terrain.elevation_m;
        terrain.material_gradient = terrain.gradient;
        let land_micro_relief_amount = terrain_smoothstep_sample(
            0.40,
            0.70,
            land_ruggedness,
        );
        if land_micro_relief_amount.value > 0.0 {
            let land_micro_relief = terrain_simplex_fractal_sample_from_circle(
                vec2<u32>(0xfe94f82bu, 0x3c6ef372u),
                terrain_position_m,
                terrain_circle_position,
                4.0,
                3u,
                0.6,
                FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
            );
            var ridged_micro_relief: TerrainNoiseSample;
            ridged_micro_relief.value = 1.0 - 2.0 * abs(land_micro_relief.value);
            ridged_micro_relief.gradient = -2.0
                * select(-1.0, 1.0, land_micro_relief.value >= 0.0)
                * land_micro_relief.gradient;
            var shaped_micro_relief: TerrainNoiseSample;
            shaped_micro_relief.value = mix(
                land_micro_relief.value,
                ridged_micro_relief.value,
                ridge_blend.value,
            );
            shaped_micro_relief.gradient = mix(
                land_micro_relief.gradient,
                ridged_micro_relief.gradient,
                ridge_blend.value,
            ) + ridge_blend.gradient
                * (ridged_micro_relief.value - land_micro_relief.value);
            terrain.elevation_m += 0.35 * landscape_visibility.x
                * land_detail_mask.value
                * land_micro_relief_amount.value
                * shaped_micro_relief.value;
            terrain.gradient += 0.35 * landscape_visibility.x * (
                land_detail_mask.gradient
                    * land_micro_relief_amount.value * shaped_micro_relief.value
                    + land_micro_relief_amount.gradient
                        * land_detail_mask.value * shaped_micro_relief.value
                    + shaped_micro_relief.gradient
                        * land_detail_mask.value * land_micro_relief_amount.value
            );
        }
    }
    var shaped_bed: TerrainNoiseSample;
    shaped_bed.value = terrain.elevation_m;
    shaped_bed.gradient = terrain.gradient;
    let seabed_deep_gate = terrain_smoothstep_sample(-4.0, -2.5, shaped_bed);
    let seabed_shore_rise = terrain_smoothstep_sample(-0.5, -0.1, shaped_bed);
    let seabed_shore_gate_value = 1.0 - seabed_shore_rise.value;
    let seabed_mask_value = seabed_deep_gate.value * seabed_shore_gate_value;
    let rock_deep_gate = terrain_smoothstep_sample(-2.0, -0.8, shaped_bed);
    let rock_shore_rise = terrain_smoothstep_sample(1.2, 2.0, shaped_bed);
    let rock_shore_gate_value = 1.0 - rock_shore_rise.value;
    let rock_mask_value = rock_deep_gate.value * rock_shore_gate_value;
    let outer_reef_deep_gate = terrain_smoothstep_sample(-22.0, -18.0, shaped_bed);
    let outer_reef_shore_rise = terrain_smoothstep_sample(-5.0, -4.0, shaped_bed);
    let outer_reef_shore_gate_value = 1.0 - outer_reef_shore_rise.value;
    let outer_reef_mask_value = outer_reef_deep_gate.value
        * outer_reef_shore_gate_value;
    if seabed_mask_value == 0.0
        && rock_mask_value == 0.0
        && outer_reef_mask_value == 0.0
    {
        return solid_ice_sample(terrain, cluster, climate, terrain_position_m);
    }
    let seabed_mask_gradient = seabed_deep_gate.gradient * seabed_shore_gate_value
        - seabed_shore_rise.gradient * seabed_deep_gate.value;
    let rock_mask_gradient = rock_deep_gate.gradient * rock_shore_gate_value
        - rock_shore_rise.gradient * rock_deep_gate.value;
    let outer_reef_mask_gradient = outer_reef_deep_gate.gradient
        * outer_reef_shore_gate_value
        - outer_reef_shore_rise.gradient * outer_reef_deep_gate.value;
    var shoal: TerrainNoiseSample;
    shoal.value = 0.0;
    shoal.gradient = vec2<f32>(0.0);
    if landscape_visibility.x > 0.0 && seabed_mask_value > 0.0 {
        shoal = terrain_simplex_fractal_sample_from_circle(
            vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            terrain_position_m,
            terrain_circle_position,
            256.0,
            3u,
            0.5,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        );
    }
    let coastal_rock_amount = coastal_rock.value;
    let coastal_rock_relief = terrain_smoothstep_sample(0.42, 0.60, coastal_rock);
    let reef_detail_amount = coastal_rock_relief.value
        * (0.05 + 1.55 * reef_ruggedness.value);
    var coastal_detail_mask_value = seabed_mask_value;
    var coastal_detail_mask_gradient = seabed_mask_gradient;
    if rock_mask_value > seabed_mask_value {
        coastal_detail_mask_value = rock_mask_value;
        coastal_detail_mask_gradient = rock_mask_gradient;
    }
    var reef: TerrainNoiseSample;
    reef.value = 0.0;
    reef.gradient = vec2<f32>(0.0);
    if landscape_visibility.x > 0.0
        && coastal_detail_mask_value > 0.0
        && coastal_rock_relief.value > 0.0
    {
        reef = terrain_simplex_fractal_sample_from_circle(
            vec2<u32>(0x75374cc3u, 0x589965ccu),
            terrain_position_m,
            terrain_circle_position,
            32.0,
            6u,
            0.55,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        );
    }
    let reef_peak = terrain_smoothstep_sample(0.72, 0.90, reef);
    var outer_reef: TerrainNoiseSample;
    outer_reef.value = 0.0;
    outer_reef.gradient = vec2<f32>(0.0);
    if landscape_visibility.x > 0.0
        && outer_reef_mask_value > 0.0
        && coastal_rock_relief.value > 0.0
    {
        outer_reef = terrain_simplex_fractal_sample_from_circle(
            vec2<u32>(0x9c88c6e3u, 0x8ebc6af0u),
            terrain_position_m,
            terrain_circle_position,
            128.0,
            8u,
            0.6,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        );
    }
    var ridged_outer_reef: TerrainNoiseSample;
    ridged_outer_reef.value = 1.0 - 2.0 * abs(outer_reef.value);
    ridged_outer_reef.gradient = -2.0
        * select(-1.0, 1.0, outer_reef.value >= 0.0)
        * outer_reef.gradient;
    let outer_reef_ridge_blend = terrain_smoothstep_sample(
        0.25,
        0.80,
        reef_ruggedness,
    );
    var shaped_outer_reef: TerrainNoiseSample;
    shaped_outer_reef.value = mix(
        outer_reef.value,
        ridged_outer_reef.value,
        outer_reef_ridge_blend.value,
    ) - 0.3 * outer_reef_ridge_blend.value;
    shaped_outer_reef.gradient = mix(
        outer_reef.gradient,
        ridged_outer_reef.gradient,
        outer_reef_ridge_blend.value,
    ) + outer_reef_ridge_blend.gradient
        * (ridged_outer_reef.value - outer_reef.value - 0.3);
    let outer_reef_detail_amount = coastal_rock_relief.value
        * (0.12 + 1.68 * reef_ruggedness.value);
    terrain.elevation_m += 0.25 * seabed_mask_value * shoal.value
        * landscape_visibility.x
        + 1.6 * coastal_detail_mask_value * reef_detail_amount * reef.value
            * landscape_visibility.x
        + 2.0 * rock_mask_value
            * coastal_rock_relief.value * reef_peak.value * landscape_visibility.x
        + 1.8 * outer_reef_mask_value
            * outer_reef_detail_amount * shaped_outer_reef.value
            * landscape_visibility.x;
    terrain.gradient += landscape_visibility.x * 0.25 * (
        seabed_mask_gradient * shoal.value + shoal.gradient * seabed_mask_value
    ) + 1.6 * landscape_visibility.x * (
        coastal_detail_mask_gradient * reef_detail_amount * reef.value
            + coastal_rock_relief.gradient
                * (0.05 + 1.55 * reef_ruggedness.value)
                * coastal_detail_mask_value * reef.value
            + reef_ruggedness.gradient * 1.55
                * coastal_rock_relief.value * coastal_detail_mask_value * reef.value
            + reef.gradient * coastal_detail_mask_value * reef_detail_amount
    ) + landscape_visibility.x * 2.0 * (
        rock_mask_gradient * coastal_rock_relief.value * reef_peak.value
            + coastal_rock_relief.gradient * rock_mask_value * reef_peak.value
            + reef_peak.gradient * rock_mask_value * coastal_rock_relief.value
    ) + 1.8 * landscape_visibility.x * (
        outer_reef_mask_gradient
            * outer_reef_detail_amount * shaped_outer_reef.value
            + coastal_rock_relief.gradient
                * (0.12 + 1.68 * reef_ruggedness.value)
                * outer_reef_mask_value * shaped_outer_reef.value
            + reef_ruggedness.gradient * 1.68
                * coastal_rock_relief.value
                * outer_reef_mask_value * shaped_outer_reef.value
            + shaped_outer_reef.gradient
                * outer_reef_mask_value * outer_reef_detail_amount
    );
    var visible_coastal_mask_value = coastal_detail_mask_value;
    if outer_reef_mask_value > visible_coastal_mask_value {
        visible_coastal_mask_value = outer_reef_mask_value;
    }
    let submerged_material_mask_value = 1.0
        - smoothstep(-0.1, 0.0, terrain.elevation_m);
    if submerged_material_mask_value > visible_coastal_mask_value {
        visible_coastal_mask_value = submerged_material_mask_value;
    }
    terrain.coastal_rock_amount = landscape_visibility.x
        * visible_coastal_mask_value * coastal_rock_amount;
    terrain.coastal_outcrop_amount = landscape_visibility.x
        * rock_mask_value * coastal_rock_relief.value * reef_peak.value;
    terrain.coastal_sediment_amount = landscape_visibility.x
        * visible_coastal_mask_value
        * (1.0 - coastal_rock_amount)
        * (1.0 - reef_ruggedness.value);
    return solid_ice_sample(terrain, cluster, climate, terrain_position_m);
}

fn procedural_terrain_sample_with_climate(
    position_m: vec2<f32>,
    climate: SurfaceClimateSample,
) -> TerrainSample {
    return procedural_terrain_sample_with_climate_from_circle(
        position_m,
        periodic_noise_circle_position(position_m),
        climate,
    );
}

fn procedural_terrain_sample(position_m: vec2<f32>) -> TerrainSample {
    return procedural_terrain_sample_with_climate(
        position_m,
        surface_climate_sample_at(position_m),
    );
}

fn broad_landform_elevation_m(position_m: vec2<f32>) -> f32 {
    // Long terrain shadows follow island-scale relief; coastal detail remains
    // locally lit instead of multiplying the full terrain cost at every sample.
    let terrain_position_m = position_m;
    let terrain_circle_position = periodic_noise_circle_position(terrain_position_m);
    let cluster = terrain_simplex_fractal_value_from_circle(
        vec2<u32>(0x78bd642fu, 0xa0761d64u),
        terrain_position_m,
        terrain_circle_position,
        8192.0,
        3u,
        0.5,
        TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
    );
    let island = terrain_simplex_fractal_value_from_circle(
        vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        terrain_position_m,
        terrain_circle_position,
        1536.0,
        3u,
        0.35,
        TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT,
    );
    let unscaled_elevation_m = 18.0 * (0.78 * cluster + 0.22 * island - 0.30);
    let island_ruggedness = smoothstep(0.32, 0.58, cluster);
    let trench_depth = smoothstep(0.50, 0.74, -cluster);
    let elevation_scale = tuning.terrain_tuning.y
        * (1.0 + 19.0 * island_ruggedness + 29.0 * trench_depth);
    return unscaled_elevation_m * elevation_scale;
}

fn landform_light_grade(
    surface_gradient: vec2<f32>,
    roughness: f32,
) -> vec2<f32> {
    let strength = clamp(
        tuning.landform_lighting.x
            * landscape_visibility.w * lighting_feature_visibility().x,
        0.0,
        2.0,
    );
    if strength <= 0.0 {
        return vec2<f32>(1.0);
    }
    let normal = normalize(vec3<f32>(-surface_gradient, 1.0));
    let source_amount = direct_sun_amount() + direct_moon_amount();
    let combined_light = direct_sun_amount() * sun_direction()
        + direct_moon_amount() * moon_direction();
    let light_direction = select(
        vec3<f32>(0.0, 0.0, 1.0),
        normalize(combined_light),
        length(combined_light) > 0.001,
    );
    let sky_direction = normalize(vec3<f32>(0.28 * light_direction.xy, 1.0));
    let sky_facing = clamp(dot(normal, sky_direction), 0.0, 1.0);
    let sky_grade = mix(0.70, 1.08, smoothstep(0.20, 1.0, sky_facing));
    let sun_facing = smoothstep(
        -0.12,
        1.0,
        dot(normal, sun_direction()),
    );
    let moon_facing = smoothstep(-0.12, 1.0, dot(normal, moon_direction()));
    let direct_facing = (
        direct_sun_amount() * sun_facing
            + direct_moon_amount() * moon_facing
    ) / max(source_amount, 0.0001);
    let specular_exponent = mix(56.0, 7.0, clamp(roughness, 0.0, 1.0));
    let half_direction_sum = light_direction + vec3<f32>(0.0, 0.0, 1.0);
    var specular = 0.0;
    if length(half_direction_sum) > 0.001 {
        specular = pow(
            clamp(dot(normal, normalize(half_direction_sum)), 0.0, 1.0),
            specular_exponent,
        ) * mix(0.16, 0.025, clamp(roughness, 0.0, 1.0));
    }
    let direct_grade = mix(0.44, 1.30, direct_facing) + specular;
    return clamp(
        vec2<f32>(1.0) + strength * (vec2<f32>(sky_grade, direct_grade) - 1.0),
        vec2<f32>(0.50),
        vec2<f32>(1.40),
    );
}

fn landform_shadow_transmittance_for_light(
    position_m: vec2<f32>,
    elevation_m: f32,
    light_direction: vec3<f32>,
    light_amount: f32,
) -> f32 {
    let strength = clamp(
        tuning.landform_lighting.y
            * landscape_visibility.w * lighting_feature_visibility().y,
        0.0,
        1.0,
    );
    let horizontal_light = length(light_direction.xy);
    if strength <= 0.0
        || light_amount <= 0.0
        || light_direction.z <= 0.0
        || horizontal_light <= 0.001
    {
        return 1.0;
    }
    let toward_light = light_direction.xy / horizontal_light;
    let ray_slope = light_direction.z / horizontal_light;
    let reach_m = max(tuning.landform_lighting.z, 1.0);
    var occlusion = 0.0;
    for (var sample_index = 0u; sample_index < 4u; sample_index++) {
        let distance_m = reach_m * exp2(mix(-4.0, 0.0, f32(sample_index) / 3.0));
        let blocker_elevation_m = sampled_broad_landform_elevation_m(
            position_m + distance_m * toward_light,
        );
        let ray_elevation_m = elevation_m + distance_m * ray_slope;
        let softness_m = 0.6
            + 2.0 * CELESTIAL_DISC_ANGULAR_RADIUS_RAD * distance_m;
        occlusion = max(
            occlusion,
            smoothstep(0.0, softness_m, blocker_elevation_m - ray_elevation_m),
        );
    }
    return 1.0 - strength * occlusion;
}

fn landform_shadow_transmittance(
    position_m: vec2<f32>,
    elevation_m: f32,
) -> vec2<f32> {
    var transmittance = vec2<f32>(1.0);
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let is_sun = source_index == 0u;
        transmittance[source_index] = landform_shadow_transmittance_for_light(
            position_m,
            elevation_m,
            select(moon_direction(), sun_direction(), is_sun),
            select(direct_moon_amount(), direct_sun_amount(), is_sun),
        );
    }
    return transmittance;
}

fn terrain_rock_surface(
    position_m: vec2<f32>,
    terrain_gradient: vec2<f32>,
    material_gradient: vec2<f32>,
    elevation_m: f32,
) -> TerrainSurface {
    let color_visibility = solid_material_color_visibility().x;
    if landscape_visibility.z <= 0.0 {
        return TerrainSurface(
            color_visibility * vec3<f32>(0.36, 0.34, 0.30),
            vec2<f32>(0.0),
            0.8,
        );
    }
    let rock_geology = material_fractal_value(
        vec2<u32>(0x1f83d9abu, 0x5be0cd19u),
        position_m,
        384.0,
        3u,
    );
    let surface_height = material_fractal_sample(
        vec2<u32>(0x243f6a88u, 0x85a308d3u),
        position_m,
        28.0,
        4u,
    );
    let micro_height = material_fractal_sample(
        vec2<u32>(0xcbbb9d5du, 0x629a292au),
        position_m,
        5.0,
        6u,
    );
    let physical_ruggedness = smoothstep(
        0.01,
        0.08,
        length(terrain_gradient - material_gradient),
    );
    let rock_character = smoothstep(-0.45, 0.45, rock_geology);
    let fracture_amount = smoothstep(0.18, 0.42, rock_character)
        * (1.0 - smoothstep(0.68, 0.88, rock_character));
    let rock_ruggedness = max(
        physical_ruggedness,
        smoothstep(0.62, 0.92, rock_character),
    );
    let surface_height_m = 0.18
        + 1.15 * fracture_amount
        + 2.20 * rock_ruggedness;
    let micro_height_m = 0.06
        + 0.18 * fracture_amount
        + 0.38 * rock_ruggedness;
    let plate_direction = surface_height.value
        / sqrt(surface_height.value * surface_height.value + 0.0025);
    let geology_grade = clamp(
        0.5 + landscape_visibility.z * (
            0.40 * rock_geology
                + 0.20 * surface_height.value
                + 0.10 * micro_height.value
        ),
        0.0,
        1.0,
    );
    let rock_color = mix(
        vec3<f32>(0.25, 0.23, 0.20),
        vec3<f32>(0.49, 0.46, 0.40),
        0.18 * smoothstep(20.0, 240.0, elevation_m) + 0.82 * geology_grade,
    );
    return TerrainSurface(
        color_visibility * rock_color,
        tuning.land_material_normals.x * (
            surface_height_m * mix(
                surface_height.gradient,
                plate_direction * surface_height.gradient,
                fracture_amount,
            )
                + micro_height_m * micro_height.gradient
        ),
        mix(0.92, 0.68, rock_ruggedness),
    );
}

fn ice_surface(position_m: vec2<f32>, surface: f32) -> TerrainSurface {
    let color_visibility = solid_material_color_visibility().z;
    if landscape_visibility.z <= 0.0 {
        return TerrainSurface(
            color_visibility * vec3<f32>(0.78, 0.86, 0.88),
            vec2<f32>(0.0),
            tuning.ice_surface.y,
        );
    }
    let micro_height = material_fractal_sample(
        vec2<u32>(0xcbbb9d5du, 0x629a292au),
        position_m,
        4.0,
        6u,
    );
    let fine = material_fractal_value(
        vec2<u32>(0x13198a2eu, 0x03707344u),
        position_m,
        72.0,
        3u,
    );
    let surface_grade = 0.5 + 0.5 * landscape_visibility.z * surface;
    return TerrainSurface(
        color_visibility * mix(
            vec3<f32>(0.56, 0.75, 0.80),
            vec3<f32>(0.94, 0.97, 0.96),
            clamp(0.45 + 0.35 * surface_grade + 0.22 * fine, 0.0, 1.0),
        ),
        tuning.ice_surface.x * micro_height.gradient,
        tuning.ice_surface.y,
    );
}

fn sea_ice_surface(
    position_m: vec2<f32>,
    cover: f32,
    bedrock_elevation_m: f32,
) -> TerrainSurface {
    let color_visibility = solid_material_color_visibility().w;
    if landscape_visibility.z <= 0.0 {
        return TerrainSurface(
            color_visibility * vec3<f32>(0.78, 0.86, 0.88),
            vec2<f32>(0.0),
            clamp(0.42 * tuning.ice_surface.w, 0.0, 1.0),
        );
    }
    let broad = material_fractal_sample(
        vec2<u32>(0xa4093822u, 0x299f31d0u),
        position_m,
        768.0,
        4u,
    );
    let fine = material_fractal_sample(
        vec2<u32>(0x13198a2eu, 0x03707344u),
        position_m,
        72.0,
        3u,
    );
    let micro = material_fractal_sample(
        vec2<u32>(0xcbbb9d5du, 0x629a292au),
        position_m,
        4.0,
        6u,
    );
    let target_slab_size_m = max(tuning.ice_structure.x, 1.0);
    let slab_x_cycles = max(1, i32(round(scene.world_size_m.x / target_slab_size_m)));
    let slab_size_m = vec2<f32>(
        scene.world_size_m.x / f32(slab_x_cycles),
        target_slab_size_m,
    );
    let canonical_position = canonical_world_position(position_m);
    let positive_x = select(
        canonical_position.x,
        canonical_position.x + scene.world_size_m.x,
        canonical_position.x < 0.0,
    );
    let slab_position = vec2<f32>(positive_x, position_m.y) / slab_size_m;
    let slab_base = vec2<i32>(floor(slab_position));
    let slab_local = fract(slab_position);
    var nearest_distance_m = 1e9;
    var second_distance_m = 1e9;
    var slab_hash = vec2<u32>(0u);
    for (var offset_y = -1; offset_y <= 1; offset_y++) {
        for (var offset_x = -1; offset_x <= 1; offset_x++) {
            let unwrapped_cell = slab_base + vec2<i32>(offset_x, offset_y);
            let wrapped_cell = vec2<i32>(
                positive_mod(unwrapped_cell.x, slab_x_cycles),
                unwrapped_cell.y,
            );
            let candidate_hash = terrain_hash(
                scene.match_seed
                    ^ multiply_u64(
                        signed_u64(wrapped_cell.x),
                        vec2<u32>(0x7f4a7c15u, 0x9e3779b9u),
                    )
                    ^ multiply_u64(
                        signed_u64(wrapped_cell.y),
                        vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
                    ),
            );
            let jitter = 0.05 + 0.90 * vec2<f32>(
                visual_hash_unit(candidate_hash.x),
                visual_hash_unit(candidate_hash.y),
            );
            let delta_m = (
                vec2<f32>(f32(offset_x), f32(offset_y)) + jitter - slab_local
            ) * slab_size_m;
            let distance_m = length(delta_m);
            if distance_m < nearest_distance_m {
                second_distance_m = nearest_distance_m;
                nearest_distance_m = distance_m;
                slab_hash = candidate_hash;
            } else if distance_m < second_distance_m {
                second_distance_m = distance_m;
            }
        }
    }
    let slab_visibility = 1.0 - smoothstep(
        0.18 * min(slab_size_m.x, slab_size_m.y),
        0.45 * min(slab_size_m.x, slab_size_m.y),
        visual_pixel_footprint_m,
    );
    let fracture_width = max(tuning.ice_structure.y, 0.01);
    let fracture_geometry = 1.0 - smoothstep(
        1.5 * fracture_width,
        (5.0 + visual_pixel_footprint_m) * fracture_width,
        second_distance_m - nearest_distance_m,
    );
    let fracture_stress = smoothstep(
        -0.45,
        0.45,
        0.72 * broad.value + 0.28 * fine.value,
    );
    let fracture = clamp(
        tuning.ice_structure.z
            * slab_visibility
            * fracture_geometry
            * mix(0.22, 1.0, fracture_stress),
        0.0,
        1.0,
    );
    let slab_tone = visual_hash_unit(slab_hash.x);
    let slab_tilt = 0.025 * vec2<f32>(
        2.0 * visual_hash_unit(slab_hash.x ^ 0x9e3779b9u) - 1.0,
        2.0 * visual_hash_unit(slab_hash.y ^ 0x7f4a7c15u) - 1.0,
    );
    let shallow = smoothstep(-120.0, -3.0, bedrock_elevation_m);
    let shore = smoothstep(-18.0, -1.0, bedrock_elevation_m);
    let compactness = smoothstep(0.50, 0.90, cover);
    let plate_boundary = 1.0 - smoothstep(0.0, 0.16, abs(broad.value));
    let surface_grade = clamp(
        0.55
            + 0.28 * broad.value
            + 0.10 * fine.value
            + 0.12 * slab_visibility * (slab_tone - 0.5)
            + 0.10 * compactness
            - 0.12 * plate_boundary
            - 0.22 * fracture,
        0.0,
        1.0,
    );
    let deep_color = mix(
        vec3<f32>(0.48, 0.68, 0.75),
        vec3<f32>(0.94, 0.98, 0.98),
        surface_grade,
    );
    let shallow_color = mix(
        deep_color,
        vec3<f32>(0.56, 0.82, 0.85),
        clamp(tuning.ice_structure.w * (0.28 + 0.18 * shore), 0.0, 1.0),
    );
    return TerrainSurface(
        color_visibility * mix(deep_color, shallow_color, shallow),
        tuning.ice_surface.z * (
            mix(slab_visibility * slab_tilt, 0.45 * broad.gradient, fracture)
                + 0.20 * fine.gradient
                + 0.16 * micro.gradient
        ),
        clamp(
            tuning.ice_surface.w * (
                0.24 + 0.28 * fracture + 0.16 * plate_boundary + 0.10 * shallow
            ),
            0.0,
            1.0,
        ),
    );
}

fn blend_terrain_surface(
    first: TerrainSurface,
    second: TerrainSurface,
    amount: f32,
) -> TerrainSurface {
    let blend = clamp(amount, 0.0, 1.0);
    return TerrainSurface(
        mix(first.color, second.color, blend),
        mix(first.detail_gradient, second.detail_gradient, blend),
        mix(first.roughness, second.roughness, blend),
    );
}
