// Broad terrain noises whose leading octaves follow the longitude circle.
// `terrain_broad_noise` reads them from the persistent terrain cache where a
// composition binds it, and evaluates them directly elsewhere.
const TERRAIN_CLUSTER_NOISE: u32 = 0u;
const TERRAIN_ISLAND_NOISE: u32 = 1u;
const TERRAIN_RUGGEDNESS_NOISE: u32 = 2u;
const TERRAIN_COASTAL_SUBSTRATE_NOISE: u32 = 3u;
const TERRAIN_BROAD_NOISE_FIELDS = array<u32, 4>(
    TERRAIN_CLUSTER_NOISE_FIELD,
    TERRAIN_ISLAND_NOISE_FIELD,
    TERRAIN_RUGGEDNESS_NOISE_FIELD,
    TERRAIN_COASTAL_SUBSTRATE_NOISE_FIELD,
);
const TERRAIN_BROAD_NOISE_WAVELENGTHS_M = array<f32, 4>(8192.0, 1536.0, 2048.0, 1536.0);
const TERRAIN_BROAD_NOISE_OCTAVES = array<u32, 4>(3u, 5u, 3u, 3u);
const TERRAIN_BROAD_NOISE_PERSISTENCE = array<f32, 4>(0.5, 0.35, 0.5, 0.5);
const TERRAIN_BROAD_NOISE_CIRCLE_OCTAVES = array<u32, 4>(
    TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
    TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT,
    TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
    TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
);

fn terrain_broad_octave_sum(
    field: u32,
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
    first_octave: u32,
    end_octave: u32,
) -> TerrainNoiseSample {
    return terrain_simplex_octave_sum_from_circle(
        TERRAIN_BROAD_NOISE_FIELDS[field],
        position_m,
        circle_position,
        TERRAIN_BROAD_NOISE_WAVELENGTHS_M[field],
        first_octave,
        end_octave,
        TERRAIN_BROAD_NOISE_PERSISTENCE[field],
        TERRAIN_BROAD_NOISE_CIRCLE_OCTAVES[field],
    );
}

const TERRAIN_BROAD_NOISE_COUNT: u32 = 4u;
// Sixteen texels per wavelength bound bicubic Hermite error near 6e-5 of an
// octave's amplitude: about a millimetre of elevation for the finest cached
// island octave on the most rugged islands.
const TERRAIN_CACHE_TEXELS_PER_WAVELENGTH: f32 = 16.0;

// Leading circle octaves of `field` that a cache level with this spacing holds.
fn terrain_cache_octaves(field: u32, spacing_m: f32) -> u32 {
    var octaves = 0u;
    var wavelength_m = TERRAIN_BROAD_NOISE_WAVELENGTHS_M[field];
    while octaves < TERRAIN_BROAD_NOISE_CIRCLE_OCTAVES[field]
        && wavelength_m >= TERRAIN_CACHE_TEXELS_PER_WAVELENGTH * spacing_m
    {
        octaves += 1u;
        wavelength_m *= 0.5;
    }
    return octaves;
}

// Eight-bit colour layers per baked surface level. Keep synchronized with
// `BAKED_SURFACE_COLOR_LAYERS` in baked_surface.rs.
const BAKED_SURFACE_COLOR_LAYERS: u32 = 5u;
// The largest land and land-ice detail gradient the baked surface encodes.
const BAKED_DETAIL_GRADIENT_RANGE: f32 = 4.0;
// The largest sea-ice relief gradient the baked surface encodes. Ranges of
// 0.25 and 1.0 render boat-zoom floes within two 8-bit levels of each other.
const BAKED_SEA_ICE_GRADIENT_RANGE: f32 = 0.5;

fn direct_terrain_broad_noise(
    field: u32,
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
) -> TerrainNoiseSample {
    return terrain_simplex_fractal_sample_from_circle(
        TERRAIN_BROAD_NOISE_FIELDS[field],
        position_m,
        circle_position,
        TERRAIN_BROAD_NOISE_WAVELENGTHS_M[field],
        TERRAIN_BROAD_NOISE_OCTAVES[field],
        TERRAIN_BROAD_NOISE_PERSISTENCE[field],
        TERRAIN_BROAD_NOISE_CIRCLE_OCTAVES[field],
    );
}

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

// How far the cluster noise moves water temperature from its climate.
fn maritime_temperature_scale_c(climate: SurfaceClimateSample) -> f32 {
    return 16.0 * (1.0 - smoothstep(75.0, 90.0, abs(climate.latitude_degrees)));
}

// The share of the water left open by sea ice, which shows where it covers
// more than half.
fn sea_ice_open_water(
    climate: SurfaceClimateSample,
    maritime_temperature: TerrainNoiseSample,
) -> TerrainNoiseSample {
    let maritime_scale = maritime_temperature_scale_c(climate);
    var water_temperature: TerrainNoiseSample;
    water_temperature.value = water_temperature_c_from_climate(climate)
        + maritime_scale * maritime_temperature.value;
    water_temperature.gradient = maritime_scale * maritime_temperature.gradient;
    return terrain_smoothstep_sample(-2.0, 0.0, water_temperature);
}

fn solid_ice_sample(
    terrain: TerrainSample,
    maritime_temperature: TerrainNoiseSample,
    climate: SurfaceClimateSample,
    position_m: vec2<f32>,
) -> TerrainSample {
    var surface = terrain;
    let maritime_scale = maritime_temperature_scale_c(climate);
    surface.maritime_temperature_anomaly_c = maritime_scale * maritime_temperature.value;
    var bedrock: TerrainNoiseSample;
    bedrock.value = terrain.elevation_m;
    bedrock.gradient = terrain.gradient;
    if bedrock.value < 0.0 {
        let open_water = sea_ice_open_water(climate, maritime_temperature);
        let cover = 1.0 - open_water.value;
        if cover <= 0.5 {
            return surface;
        }
        // A gameplay solid-surface height standing in for sea-ice freeboard.
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
    // Far below the 3 °C onset of autumn colour, which texels of the baked
    // surface with land ice give up for its detail.
    if annual_temperature.value >= -8.0 {
        return surface;
    }
    // The annual-mean threshold stands in for glacier mass balance.
    let warm = terrain_smoothstep_sample(-12.0, -8.0, annual_temperature);
    var cold: TerrainNoiseSample;
    cold.value = 1.0 - warm.value;
    cold.gradient = -warm.gradient;
    let amount = cold.value;
    let amount_gradient = cold.gradient;
    // The 20 m and 4 m heights are gameplay surfaces standing in for glacier thickness.
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
    let bedrock = procedural_bedrock_sample_from_circle(position_m, terrain_circle_position);
    return solid_ice_sample(bedrock, bedrock.cluster, climate, position_m);
}

// Terrain before climate-derived ice.
fn procedural_bedrock_sample_from_circle(
    position_m: vec2<f32>,
    terrain_circle_position: vec4<f32>,
) -> TerrainSample {
    // Keep the density composition synchronized with
    // `game::procedural_terrain_elevation_m`; Rust remains authoritative
    // for physical depth.
    let terrain_position_m = position_m;
    let cluster = terrain_broad_noise(
        TERRAIN_CLUSTER_NOISE,
        terrain_position_m,
        terrain_circle_position,
    );
    let island = terrain_broad_noise(
        TERRAIN_ISLAND_NOISE,
        terrain_position_m,
        terrain_circle_position,
    );
    var unscaled: TerrainNoiseSample;
    unscaled.value = 18.0 * (0.78 * cluster.value + 0.22 * island.value - 0.30);
    unscaled.gradient = 18.0 * (0.78 * cluster.gradient + 0.22 * island.gradient);
    let island_ruggedness = terrain_smoothstep_sample(0.32, 0.58, cluster);
    var negative_cluster: TerrainNoiseSample;
    negative_cluster.value = -cluster.value;
    negative_cluster.gradient = -cluster.gradient;
    let trench_depth = terrain_smoothstep_sample(0.50, 0.74, negative_cluster);
    let elevation_scale = TERRAIN_ELEVATION_SCALE
        * (1.0 + 19.0 * island_ruggedness.value + 29.0 * trench_depth.value);
    var base: TerrainNoiseSample;
    base.value = unscaled.value * elevation_scale;
    base.gradient = unscaled.gradient * elevation_scale
        + unscaled.value
            * TERRAIN_ELEVATION_SCALE
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
    let coastal_relief = landscape_visibility.x > 0.0
        && base.value > -32.0
        && base.value < 4.0;
    if coastal_relief || EVALUATE_ALL_BRANCHES {
        let reef_ruggedness_noise = terrain_broad_noise(
            TERRAIN_RUGGEDNESS_NOISE,
            terrain_position_m,
            terrain_circle_position,
        );
        reef_ruggedness = noise_sample_or_zero(
            terrain_smoothstep_sample(-0.25, 0.25, reef_ruggedness_noise),
            coastal_relief,
        );
        let coastal_substrate = terrain_broad_noise(
            TERRAIN_COASTAL_SUBSTRATE_NOISE,
            terrain_position_m,
            terrain_circle_position,
        );
        coastal_rock = noise_sample_or_zero(
            terrain_smoothstep_sample(-0.25, 0.35, coastal_substrate),
            coastal_relief,
        );
    }
    var coastal_sediment: TerrainNoiseSample;
    coastal_sediment.value = (1.0 - coastal_rock.value)
        * (1.0 - reef_ruggedness.value);
    coastal_sediment.gradient = -coastal_rock.gradient
        * (1.0 - reef_ruggedness.value)
        - reef_ruggedness.gradient * (1.0 - coastal_rock.value);
    var ocean_floor_depth: TerrainNoiseSample;
    ocean_floor_depth.value = OCEAN_FLOOR_DEPTH_M + 3000.0 * trench_depth.value;
    ocean_floor_depth.gradient = 3000.0 * trench_depth.gradient;
    let positive_base_depth_m = max(base_depth.value, 0.0);
    var shaped_depth: TerrainNoiseSample;
    shaped_depth.value = shaped_bed_depth_m(base_depth.value);
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
    // Forced detail below adds exactly zero where its mask is zero.
    if land_detail_mask.value > 0.0 || EVALUATE_ALL_BRANCHES {
        let land_ruggedness_noise = terrain_broad_noise(
            TERRAIN_RUGGEDNESS_NOISE,
            terrain_position_m,
            terrain_circle_position,
        );
        let land_ruggedness = terrain_smoothstep_sample(
            -0.25,
            0.25,
            land_ruggedness_noise,
        );
        let land_detail = terrain_simplex_fractal_sample_from_circle(
            TERRAIN_LAND_RELIEF_NOISE_FIELD,
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
        // Cover needs hill-scale variation without metre-scale aliased speckle.
        terrain.material_elevation_m = terrain.elevation_m;
        terrain.material_gradient = terrain.gradient;
        let land_micro_relief_amount = terrain_smoothstep_sample(
            0.40,
            0.70,
            land_ruggedness,
        );
        if land_micro_relief_amount.value > 0.0 || EVALUATE_ALL_BRANCHES {
            let land_micro_relief = terrain_simplex_fractal_sample_from_circle(
                TERRAIN_LAND_MICRO_RELIEF_NOISE_FIELD,
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
    let coastal_detail = seabed_mask_value != 0.0
        || rock_mask_value != 0.0
        || outer_reef_mask_value != 0.0;
    if !coastal_detail && !EVALUATE_ALL_BRANCHES {
        return terrain;
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
    // Forced reef detail below adds exactly zero where its masks are zero.
    if (landscape_visibility.x > 0.0 && seabed_mask_value > 0.0) || EVALUATE_ALL_BRANCHES {
        shoal = terrain_simplex_fractal_sample_from_circle(
            TERRAIN_SHOAL_NOISE_FIELD,
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
    if (landscape_visibility.x > 0.0
        && coastal_detail_mask_value > 0.0
        && coastal_rock_relief.value > 0.0)
        || EVALUATE_ALL_BRANCHES
    {
        reef = terrain_simplex_fractal_sample_from_circle(
            TERRAIN_REEF_NOISE_FIELD,
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
    if (landscape_visibility.x > 0.0
        && outer_reef_mask_value > 0.0
        && coastal_rock_relief.value > 0.0)
        || EVALUATE_ALL_BRANCHES
    {
        outer_reef = terrain_simplex_fractal_sample_from_circle(
            TERRAIN_OUTER_REEF_NOISE_FIELD,
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
    terrain.coastal_rock_amount = select(0.0, landscape_visibility.x
        * visible_coastal_mask_value * coastal_rock_amount, coastal_detail);
    terrain.coastal_outcrop_amount = landscape_visibility.x
        * rock_mask_value * coastal_rock_relief.value * reef_peak.value;
    terrain.coastal_sediment_amount = select(0.0, landscape_visibility.x
        * visible_coastal_mask_value
        * (1.0 - coastal_rock_amount)
        * (1.0 - reef_ruggedness.value), coastal_detail);
    return terrain;
}

const OCEAN_FLOOR_DEPTH_M: f32 = 3000.0;

// Bed depth below sea level from the broad depth, deepening offshore.
fn shaped_bed_depth_m(broad_depth_m: f32) -> f32 {
    let positive_depth_m = max(broad_depth_m, 0.0);
    return broad_depth_m
        + 0.015 * positive_depth_m * positive_depth_m * positive_depth_m * positive_depth_m;
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
        TERRAIN_CLUSTER_NOISE_FIELD,
        terrain_position_m,
        terrain_circle_position,
        8192.0,
        3u,
        0.5,
        TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
    );
    let island = terrain_simplex_fractal_value_from_circle(
        TERRAIN_ISLAND_NOISE_FIELD,
        terrain_position_m,
        terrain_circle_position,
        1536.0,
        TERRAIN_BROAD_NOISE_OCTAVES[TERRAIN_ISLAND_NOISE],
        0.35,
        TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT,
    );
    let unscaled_elevation_m = 18.0 * (0.78 * cluster + 0.22 * island - 0.30);
    let island_ruggedness = smoothstep(0.32, 0.58, cluster);
    let trench_depth = smoothstep(0.50, 0.74, -cluster);
    let elevation_scale = TERRAIN_ELEVATION_SCALE
        * (1.0 + 19.0 * island_ruggedness + 29.0 * trench_depth);
    return unscaled_elevation_m * elevation_scale;
}

// Relief grades of the sky's and each beam's light. The beams' glint toward
// the overhead viewer, scaled by `specular_scale`, is light the surface
// reflects rather than receives.
fn landform_light_grade(
    surface_gradient: vec2<f32>,
    roughness: f32,
    direct_light: CelestialReceiverLight,
    specular_scale: f32,
) -> vec3<f32> {
    let strength = clamp(
        tuning.landform_lighting.x
            * landscape_visibility.w * lighting_feature_visibility().x,
        0.0,
        1.0,
    );
    if strength <= 0.0 {
        return vec3<f32>(1.0);
    }
    let normal = normalize(vec3<f32>(-surface_gradient, 1.0));
    let sun = celestial_source_light(direct_light, 0u);
    let moon = celestial_source_light(direct_light, 1u);
    let combined_light = sun.amount * sun.direction
        + moon.amount * moon.direction;
    let light_direction = select(
        vec3<f32>(0.0, 0.0, 1.0),
        normalize(combined_light),
        length(combined_light) > 0.0,
    );
    let sky_direction = normalize(vec3<f32>(0.28 * light_direction.xy, 1.0));
    let sky_facing = clamp(dot(normal, sky_direction), 0.0, 1.0);
    let sky_grade = mix(0.70, 1.08, smoothstep(0.20, 1.0, sky_facing));
    let specular_exponent = mix(56.0, 7.0, clamp(roughness, 0.0, 1.0));
    var direct_grades = vec2<f32>(1.0);
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let source = celestial_source_light(direct_light, source_index);
        let direct_facing = smoothstep(
            -0.12,
            1.0,
            dot(normal, source.direction),
        );
        let half_direction_sum = source.direction + vec3<f32>(0.0, 0.0, 1.0);
        var specular = 0.0;
        if length(half_direction_sum) > 0.001 {
            specular = pow(
                clamp(dot(normal, normalize(half_direction_sum)), 0.0, 1.0),
                specular_exponent,
            ) * mix(0.16, 0.025, clamp(roughness, 0.0, 1.0)) * specular_scale;
        }
        direct_grades[source_index] = mix(0.44, 1.30, direct_facing) + specular;
    }
    return clamp(
        vec3<f32>(1.0)
            + strength * (vec3<f32>(sky_grade, direct_grades) - 1.0),
        vec3<f32>(0.50),
        vec3<f32>(1.40),
    );
}

fn landform_shadow_transmittance_for_light(
    position_m: vec2<f32>,
    elevation_m: f32,
    light_direction: vec3<f32>,
    light_amount: f32,
    angular_radius_rad: f32,
    source_index: u32,
) -> f32 {
    var strength = landform_shadow_strength();
    if LEGACY_SHADOW_LOOK {
        strength *= 0.7;
    }
    let unshadowed = strength <= 0.0
        || light_amount <= 0.0
        || length(light_direction.xy) <= 0.001;
    if unshadowed && !EVALUATE_ALL_BRANCHES {
        return 1.0;
    }
    let horizon = sampled_landform_horizon(
        position_m,
        light_direction,
        angular_radius_rad,
        source_index,
    );
    let occlusion = landform_disc_occlusion(horizon.x - elevation_m, horizon.y);
    return select(1.0 - strength * occlusion, 1.0, unshadowed);
}

fn landform_shadow_strength() -> f32 {
    return clamp(
        tuning.landform_lighting.y
            * landscape_visibility.w * lighting_feature_visibility().y,
        0.0,
        1.0,
    );
}

// The highest surface a shadow can fall on. Keep synchronized with
// `highest_receiver_m` in field_grids.rs.
fn highest_shadow_receiver_m() -> f32 {
    return 252.0 * TERRAIN_ELEVATION_SCALE + 6.0;
}

// Stored where no landform shadow can fall: the light is overhead, unseen by
// any receiver, or landform shadows are off.
const LANDFORM_UNSHADOWED_HEIGHT_M: f32 = -10000.0;
// Steps start this far apart and widen to this share of their distance.
// Production reaches take about 30 steps under a high light and about 80 at
// the highest receiver's horizon; the step bound only limits lab reach
// tuning. On the rugged island near (-9000, 2017) m under lights 3° to 15°
// high, up to about 0.6% of sunlit pixels in 240 m and 1.5 km views then
// differ by more than a quarter of the disc from the largest occlusion of a
// per-pixel march over directly evaluated landform every 4 m widening to 1%;
// steps widening to an eighth leave twice as many in the 1.5 km view at 15°.
const LANDFORM_HORIZON_STEP_M: f32 = 8.0;
const LANDFORM_HORIZON_STEP_RATIO: f32 = 0.0625;
const LANDFORM_HORIZON_MAX_STEPS: u32 = 256u;

// The receiver height midway through the landform's penumbra toward a light,
// then the height over which its cover of the disc goes from none to all. A
// ray toward the light from height h stays above a blocker of elevation b at
// surface distance d while (R + h) / q - R > b, with
// q = cos(d / R) - tan(light) sin(d / R), so the blocker covers half the disc
// below h = (R + b) q - R. Its penumbra spans 0.6 m plus the disc's diameter
// at that distance along the ray, and each metre of receiver height moves the
// ray 1 / q metres there, so the receiver crosses the penumbra over q times
// its span. Some blocker touches the disc below the highest top of the
// blockers' penumbrae and one covers it all below the highest bottom, so the
// cover changes continuously as blockers overtake each other. Steps run from a sixteenth of the tuned reach to the march's
// reach. Under EVALUATE_ALL_BRANCHES every light marches the reach at the
// highest receiver's horizon and keeps only the steps within its own.
fn landform_horizon(
    position_m: vec2<f32>,
    light_direction: vec3<f32>,
    angular_radius_rad: f32,
) -> vec2<f32> {
    let horizontal_light = length(light_direction.xy);
    let overhead = horizontal_light <= 0.001;
    if overhead && !EVALUATE_ALL_BRANCHES {
        return vec2<f32>(LANDFORM_UNSHADOWED_HEIGHT_M, 1.0);
    }
    let toward_light = select(
        light_direction.xy / horizontal_light,
        vec2<f32>(1.0, 0.0),
        overhead,
    );
    let light_tangent = select(light_direction.z / horizontal_light, 0.0, overhead);
    let world_radius_m = scene.world_size_m.x / TAU;
    let reach_m = select(
        landform_shadow_reach_m(light_direction.z, horizontal_light),
        0.0,
        overhead,
    );
    var march_m = reach_m;
    if EVALUATE_ALL_BRANCHES {
        let lowest_z = celestial_horizon_z_at_height(highest_shadow_receiver_m());
        march_m = landform_shadow_reach_m(lowest_z, sqrt(1.0 - lowest_z * lowest_z));
    }
    // Penumbra bottom and top.
    var penumbra = vec2<f32>(LANDFORM_UNSHADOWED_HEIGHT_M) + vec2<f32>(-0.5, 0.5);
    var distance_m = min(max(tuning.landform_lighting.z, 1.0) / 16.0, march_m);
    for (var step = 0u; step < LANDFORM_HORIZON_MAX_STEPS; step++) {
        let angular_distance = distance_m / world_radius_m;
        let ray_scale = cos(angular_distance) - light_tangent * sin(angular_distance);
        let counted = ray_scale > 0.0 && distance_m <= reach_m;
        if counted || EVALUATE_ALL_BRANCHES {
            let blocker_elevation_m = sampled_broad_landform_elevation_m(
                position_m + distance_m * toward_light,
            );
            let half_cover_height_m = (world_radius_m + blocker_elevation_m) * ray_scale
                - world_radius_m;
            let span_m = (0.6 + 2.0 * angular_radius_rad * distance_m) * ray_scale;
            penumbra = select(
                penumbra,
                max(
                    penumbra,
                    vec2<f32>(half_cover_height_m - 0.5 * span_m, half_cover_height_m + 0.5 * span_m),
                ),
                counted,
            );
        }
        if distance_m >= march_m {
            break;
        }
        // The light's own reach is always one of the steps.
        distance_m = min(
            distance_m + max(LANDFORM_HORIZON_STEP_M, LANDFORM_HORIZON_STEP_RATIO * distance_m),
            select(march_m, reach_m, distance_m < reach_m),
        );
    }
    return vec2<f32>(0.5 * (penumbra.x + penumbra.y), penumbra.y - penumbra.x);
}

// A light at 45° or higher marches the tuned reach. A lower light extends it
// to where the ray toward it from the lowest receiver that sees it has climbed
// that height above sea level: from sea level, or for a light below the
// sea-level horizon from the receiver on whose horizon it lies, whose ray
// grazes sea level an angle of minus the light's elevation away. Marches then
// meet about the same relief heights under every light. Keep synchronized with
// `landform_shadow_reach_m` in field_grids.rs, which bounds the landform grid
// window with it.
fn landform_shadow_reach_m(light_z: f32, horizontal_light: f32) -> f32 {
    let tuned_reach_m = max(tuning.landform_lighting.z, 1.0);
    if LEGACY_SHADOW_LOOK {
        return tuned_reach_m;
    }
    let world_radius_m = scene.world_size_m.x / TAU;
    let along_ray_m = world_radius_m * max(light_z, 0.0);
    let ray_m = -along_ray_m + sqrt(
        along_ray_m * along_ray_m
            + tuned_reach_m * (2.0 * world_radius_m + tuned_reach_m),
    );
    let climb_rad = select(
        atan2(ray_m * horizontal_light, world_radius_m + ray_m * light_z),
        acos(world_radius_m / (world_radius_m + tuned_reach_m))
            - atan2(light_z, horizontal_light),
        light_z < 0.0,
    );
    return max(tuned_reach_m, world_radius_m * climb_rad);
}

fn landform_disc_occlusion(
    blocker_clearance_m: f32,
    full_width_m: f32,
) -> f32 {
    return smoothstep(
        -0.5 * full_width_m,
        0.5 * full_width_m,
        blocker_clearance_m,
    );
}

fn landform_shadow_sample_direction(
    direct_light: CelestialReceiverLight,
    source: CelestialSourceLight,
) -> vec3<f32> {
    return celestial_visible_disc_sample_direction_at_horizon(
        direct_light.horizon_z,
        source.direction,
        source.angular_radius_rad,
    );
}

fn landform_shadow_transmittance(
    direct_light: CelestialReceiverLight,
) -> vec2<f32> {
    var transmittance = vec2<f32>(1.0);
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let source = celestial_source_light(direct_light, source_index);
        transmittance[source_index] = landform_shadow_transmittance_for_light(
            direct_light.position_m,
            direct_light.height_m,
            landform_shadow_sample_direction(direct_light, source),
            source.amount,
            source.angular_radius_rad,
            source_index,
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
        ROCK_GEOLOGY_NOISE_FIELD,
        position_m,
        384.0,
        3u,
    );
    let surface_height = material_fractal_sample(
        ROCK_SURFACE_HEIGHT_NOISE_FIELD,
        position_m,
        28.0,
        4u,
    );
    let micro_height = material_micro_relief_sample(position_m, 5.0);
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
        surface_height_m * mix(
            surface_height.gradient,
            plate_direction * surface_height.gradient,
            fracture_amount,
        )
            + micro_height_m * micro_height.gradient,
        mix(0.92, 0.68, rock_ruggedness),
    );
}

fn ice_surface(position_m: vec2<f32>, surface: f32) -> TerrainSurface {
    var detail = LandIceDetail(0.0, vec2<f32>(0.0));
    if landscape_visibility.z > 0.0 {
        detail = land_ice_detail_at(position_m, surface, ice_micro_relief_sample(position_m));
    }
    return land_ice_surface(detail);
}

// The land-ice colour grade and micro-relief gradient, which depend only on
// position.
struct LandIceDetail {
    grade: f32,
    gradient: vec2<f32>,
};

// Land and sea ice share one micro-relief field, which the bake evaluates once
// for both.
fn ice_micro_relief_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    return material_micro_relief_sample(position_m, 4.0);
}

fn land_ice_detail_at(
    position_m: vec2<f32>,
    surface: f32,
    micro_height: TerrainNoiseSample,
) -> LandIceDetail {
    let fine = material_fractal_value(
        ICE_FINE_NOISE_FIELD,
        position_m,
        72.0,
        3u,
    );
    let surface_grade = 0.5 + 0.5 * landscape_visibility.z * surface;
    return LandIceDetail(
        clamp(0.45 + 0.35 * surface_grade + 0.22 * fine, 0.0, 1.0),
        micro_height.gradient,
    );
}

const LAND_ICE_DETAIL_NORMAL_SCALE: f32 = 0.18;
const LAND_ICE_ROUGHNESS: f32 = 0.32;

fn land_ice_surface(detail: LandIceDetail) -> TerrainSurface {
    let color_visibility = solid_material_color_visibility().z;
    if landscape_visibility.z <= 0.0 {
        return TerrainSurface(
            color_visibility * vec3<f32>(0.78, 0.86, 0.88),
            vec2<f32>(0.0),
            LAND_ICE_ROUGHNESS,
        );
    }
    return TerrainSurface(
        color_visibility * mix(
            vec3<f32>(0.56, 0.75, 0.80),
            vec3<f32>(0.94, 0.97, 0.96),
            detail.grade,
        ),
        LAND_ICE_DETAIL_NORMAL_SCALE * detail.gradient,
        LAND_ICE_ROUGHNESS,
    );
}

// The sea-ice plate noises and their relief, which depend only on position.
struct SeaIceDetail {
    broad: f32,
    fine: f32,
    gradient: vec2<f32>,
};

fn sea_ice_detail_at(position_m: vec2<f32>, micro: TerrainNoiseSample) -> SeaIceDetail {
    let fine = material_fractal_sample(
        SEA_ICE_FINE_NOISE_FIELD,
        position_m,
        72.0,
        3u,
    );
    return SeaIceDetail(
        material_fractal_value(SEA_ICE_BROAD_NOISE_FIELD, position_m, 768.0, 4u),
        fine.value,
        0.20 * fine.gradient + 0.16 * micro.gradient,
    );
}

const SEA_ICE_SLAB_SIZE_M: f32 = 192.0;

fn sea_ice_surface(
    position_m: vec2<f32>,
    cover: f32,
    bedrock_elevation_m: f32,
    detail: SeaIceDetail,
) -> TerrainSurface {
    let color_visibility = solid_material_color_visibility().w;
    if landscape_visibility.z <= 0.0 {
        return TerrainSurface(
            color_visibility * vec3<f32>(0.78, 0.86, 0.88),
            vec2<f32>(0.0),
            0.42,
        );
    }
    let slab_x_cycles = max(1, i32(round(scene.world_size_m.x / SEA_ICE_SLAB_SIZE_M)));
    let slab_size_m = vec2<f32>(
        scene.world_size_m.x / f32(slab_x_cycles),
        SEA_ICE_SLAB_SIZE_M,
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
    let slab_key = noise_key(SEA_ICE_SLAB_NOISE_FIELD, 0u);
    var slab_hash = 0u;
    for (var offset_y = -1; offset_y <= 1; offset_y++) {
        for (var offset_x = -1; offset_x <= 1; offset_x++) {
            let unwrapped_cell = slab_base + vec2<i32>(offset_x, offset_y);
            let wrapped_cell = vec2<i32>(
                positive_mod(unwrapped_cell.x, slab_x_cycles),
                unwrapped_cell.y,
            );
            let candidate_hash = hash32(
                slab_key
                    ^ (u32(wrapped_cell.x) * LATTICE_KEY_MULTIPLIERS[0])
                    ^ (bitcast<u32>(wrapped_cell.y) * LATTICE_KEY_MULTIPLIERS[1]),
            );
            let jitter = 0.05 + 0.90 * visual_hash_unit_pair(candidate_hash);
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
    let fracture_geometry = 1.0 - smoothstep(
        1.5,
        5.0 + visual_pixel_footprint_m,
        second_distance_m - nearest_distance_m,
    );
    let fracture_stress = smoothstep(
        -0.45,
        0.45,
        0.72 * detail.broad + 0.28 * detail.fine,
    );
    let fracture = slab_visibility
        * fracture_geometry
        * mix(0.22, 1.0, fracture_stress);
    let slab_shading_hash = hash32(slab_hash);
    let slab_tone = visual_hash_unit(hash32(slab_shading_hash));
    let slab_tilt = 0.025 * (2.0 * visual_hash_unit_pair(slab_shading_hash) - 1.0);
    // The bedrock tint under frozen water is the compromise recorded in PRODUCT.md.
    let shallow = smoothstep(-120.0, -3.0, bedrock_elevation_m);
    let shore = smoothstep(-18.0, -1.0, bedrock_elevation_m);
    let compactness = smoothstep(0.50, 0.90, cover);
    let plate_boundary = 1.0 - smoothstep(0.0, 0.16, abs(detail.broad));
    let surface_grade = clamp(
        0.55
            + 0.28 * detail.broad
            + 0.10 * detail.fine
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
        0.28 + 0.18 * shore,
    );
    return TerrainSurface(
        color_visibility * mix(deep_color, shallow_color, shallow),
        (1.0 - fracture) * slab_visibility * slab_tilt + detail.gradient,
        0.24 + 0.28 * fracture + 0.16 * plate_boundary + 0.10 * shallow,
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
