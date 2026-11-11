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
const BAKED_SURFACE_COLOR_LAYERS: u32 = 6u;
// The largest land and land-ice detail gradient the baked surface encodes.
const BAKED_DETAIL_GRADIENT_RANGE: f32 = 4.0;
// The largest difference between the terrain slope that lights the surface and
// the elevation's that the baked surface encodes: twice landform slopes up to
// 8, within about 3% near a slope of 1.
const BAKED_LIGHTING_GRADIENT_RANGE: f32 = 16.0;
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
        surface.unresolved_gradient = vec2<f32>(0.0);
        surface.unresolved_slope_variance = 0.0;
        surface.micro_relief_gradient = vec2<f32>(0.0);
        surface.micro_relief_slope_variance = 0.0;
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
    // The share of the bedrock's slope that the ice surface keeps.
    let bedrock_slope_share = select(
        1.0,
        mix(1.0, select(1.0, 0.25, broad_surface >= local_surface), amount),
        candidate_elevation > bedrock.value,
    );
    surface.unresolved_gradient = bedrock_slope_share * terrain.unresolved_gradient;
    surface.unresolved_slope_variance = bedrock_slope_share * bedrock_slope_share
        * terrain.unresolved_slope_variance;
    surface.micro_relief_gradient = bedrock_slope_share * terrain.micro_relief_gradient;
    surface.micro_relief_slope_variance = bedrock_slope_share * bedrock_slope_share
        * terrain.micro_relief_slope_variance;
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
        let land_detail_footprint = terrain_simplex_fractal_footprint_sample_from_circle(
            TERRAIN_LAND_RELIEF_NOISE_FIELD,
            terrain_position_m,
            terrain_circle_position,
            256.0,
            6u,
            0.65,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        );
        let land_detail = land_detail_footprint.sample;
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
        let land_detail_split = ridged_footprint_split(
            land_detail_footprint,
            ridge_blend.value,
            5.0 * landscape_visibility.x * land_detail_mask.value * land_detail_amount,
        );
        terrain.unresolved_gradient += land_detail_split.xy;
        terrain.unresolved_slope_variance += land_detail_split.z;
        // Cover needs hill-scale variation without metre-scale aliased speckle.
        terrain.material_elevation_m = terrain.elevation_m;
        terrain.material_gradient = terrain.gradient;
        let land_micro_relief_amount = terrain_smoothstep_sample(
            0.40,
            0.70,
            land_ruggedness,
        );
        if land_micro_relief_amount.value > 0.0 || EVALUATE_ALL_BRANCHES {
            let micro_relief_footprint = terrain_simplex_fractal_footprint_sample_from_circle(
                TERRAIN_LAND_MICRO_RELIEF_NOISE_FIELD,
                terrain_position_m,
                terrain_circle_position,
                4.0,
                3u,
                0.6,
                FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
            );
            let land_micro_relief = micro_relief_footprint.sample;
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
            let micro_relief_gradient = 0.35 * landscape_visibility.x * (
                land_detail_mask.gradient
                    * land_micro_relief_amount.value * shaped_micro_relief.value
                    + land_micro_relief_amount.gradient
                        * land_detail_mask.value * shaped_micro_relief.value
                    + shaped_micro_relief.gradient
                        * land_detail_mask.value * land_micro_relief_amount.value
            );
            terrain.gradient += micro_relief_gradient;
            let micro_relief_split = ridged_footprint_split(
                micro_relief_footprint,
                ridge_blend.value,
                0.35 * landscape_visibility.x
                    * land_detail_mask.value * land_micro_relief_amount.value,
            );
            terrain.unresolved_gradient += micro_relief_split.xy;
            terrain.unresolved_slope_variance += micro_relief_split.z;
            terrain.micro_relief_gradient = micro_relief_gradient - micro_relief_split.xy;
            terrain.micro_relief_slope_variance = micro_relief_split.z;
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
    var reef_footprint: FootprintNoiseSample;
    if (landscape_visibility.x > 0.0
        && coastal_detail_mask_value > 0.0
        && coastal_rock_relief.value > 0.0)
        || EVALUATE_ALL_BRANCHES
    {
        reef_footprint = terrain_simplex_fractal_footprint_sample_from_circle(
            TERRAIN_REEF_NOISE_FIELD,
            terrain_position_m,
            terrain_circle_position,
            32.0,
            6u,
            0.55,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        );
        reef = reef_footprint.sample;
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
    // The reef's slopes reach the gradient through its own relief and its peaks.
    let reef_peak_amount = clamp((reef.value - 0.72) / (0.90 - 0.72), 0.0, 1.0);
    let reef_scale = landscape_visibility.x * (
        1.6 * coastal_detail_mask_value * reef_detail_amount
            + 2.0 * rock_mask_value * coastal_rock_relief.value
                * 6.0 * reef_peak_amount * (1.0 - reef_peak_amount) / (0.90 - 0.72)
    );
    terrain.unresolved_gradient += reef_scale * reef_footprint.unresolved_gradient;
    terrain.unresolved_slope_variance += reef_scale * reef_scale
        * reef_footprint.unresolved_variance;
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

// The unresolved part of the slope `scale` times a fractal blended by `blend`
// toward its ridged fold 1 - 2|n| adds, then the mean square of the slopes
// the pixel does not resolve. Across the footprint the fold's sign keeps its
// box mean m, from n changing at its gradient across the footprint's width,
// and spreads the resolved slope by 1 - m² of its square.
fn ridged_footprint_split(fractal: FootprintNoiseSample, blend: f32, scale: f32) -> vec3<f32> {
    let value = fractal.sample.value;
    let gradient = fractal.sample.gradient;
    let resolved = gradient - fractal.unresolved_gradient;
    let mean_sign = clamp(
        2.0 * value / max(length(gradient) * visual_pixel_footprint_m, 1e-12),
        -1.0,
        1.0,
    );
    let fold = mix(1.0, -2.0 * select(-1.0, 1.0, value >= 0.0), blend);
    let mean_fold = mix(1.0, -2.0 * mean_sign, blend);
    let fold_spread_squared = 4.0 * blend * blend * (1.0 - mean_sign * mean_sign);
    return vec3<f32>(
        scale * (fold * gradient - mean_fold * resolved),
        scale * scale * (
            (mean_fold * mean_fold + fold_spread_squared) * fractal.unresolved_variance
                + fold_spread_squared * dot(resolved, resolved)
        ),
    );
}

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

// Land relief's share of the sky's light, then each beam's light on the
// surface for beams of `normal_light` on a surface facing them, averaged over
// the slopes a pixel covers: those of `surface_gradient`, the pixel's mean,
// and unresolved slopes of root-mean-square `unresolved_slope` around it.
// Each slope takes an isotropic sky's view factor (1 + n_z) / 2 and Lambert's
// max(n.l, 0), so the pixel takes their means: n_z of the mean squared slope,
// and the clamped cosine's expectation over the unresolved slopes taken as
// normal. These are physical; compromises leave out light that surrounding
// ground reflects onto a slope, which darkens slopes facing away from the sky
// by at most the ground's albedo of the sky light they miss, and the
// unresolved slopes' shadowing and masking of each other.
fn landform_light_grade(
    surface_gradient: vec2<f32>,
    unresolved_slope: f32,
    direct_light: CelestialReceiverLight,
    normal_light: vec2<f32>,
) -> vec3<f32> {
    let strength = clamp(
        tuning.landform_lighting.x
            * landscape_visibility.w * lighting_feature_visibility().x,
        0.0,
        1.0,
    );
    let normal_z = inverseSqrt(
        1.0 + dot(surface_gradient, surface_gradient) + unresolved_slope * unresolved_slope,
    );
    var light = vec3<f32>(mix(1.0, 0.5 + 0.5 * normal_z, strength), 0.0, 0.0);
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let direction = celestial_source_light(direct_light, source_index).direction;
        // The unresolved slopes spread n.l along the light's horizontal
        // direction, by half their mean square on each axis.
        let facing = expected_clamped_normal(
            (direction.z - dot(surface_gradient, direction.xy)) * normal_z,
            unresolved_slope * length(direction.xy) * normal_z * inverseSqrt(2.0),
        );
        light[source_index + 1u] = normal_light[source_index]
            * mix(max(direction.z, 0.0), facing, strength);
    }
    return light;
}

// E[max(x, 0)] for x normal of this mean and deviation: m Φ(m/s) + s φ(m/s).
fn expected_clamped_normal(mean: f32, deviation: f32) -> f32 {
    if deviation <= 1e-6 {
        return max(mean, 0.0);
    }
    let t = mean / deviation;
    let density = 0.39894228 * exp(-0.5 * t * t);
    return mean * standard_normal_cdf(t) + deviation * density;
}

// Φ(t) through erf by Abramowitz and Stegun's 7.1.26, within 1.5e-7.
fn standard_normal_cdf(t: f32) -> f32 {
    let x = abs(t) * 0.70710678;
    let k = 1.0 / (1.0 + 0.3275911 * x);
    let erf = 1.0 - k * (0.254829592 + k * (-0.284496736 + k * (1.421413741
        + k * (-1.453152027 + k * 1.061405429)))) * exp(-x * x);
    return 0.5 + 0.5 * sign(t) * erf;
}

// Water's refractive index in visible light.
const WATER_REFRACTIVE_INDEX: f32 = 1.34;

// Unpolarised Fresnel reflectance of light from air meeting water at
// `cos_incidence` to the surface normal.
fn water_fresnel_reflectance(cos_incidence: f32) -> f32 {
    let n = WATER_REFRACTIVE_INDEX;
    let cos_i = clamp(cos_incidence, 0.0, 1.0);
    let cos_t = sqrt(1.0 - (1.0 - cos_i * cos_i) / (n * n));
    let s = (cos_i - n * cos_t) / (cos_i + n * cos_t);
    let p = (n * cos_i - cos_t) / (n * cos_i + cos_t);
    return 0.5 * (s * s + p * p);
}

// The cosine of a ray's angle to the vertical after it crosses level water at
// `cos_incidence`, by Snell's law.
fn water_refracted_cosine(cos_incidence: f32) -> f32 {
    let n = WATER_REFRACTIVE_INDEX;
    let cos_i = clamp(cos_incidence, 0.0, 1.0);
    return sqrt(1.0 - (1.0 - cos_i * cos_i) / (n * n));
}

// Light from an isotropic sky refracts into a cone out to the critical angle
// θ_c, of mean cosine (1 + cos θ_c)/2, and passes the cosine-weighted mean of
// Fresnel's transmittance.
const WATER_DIFFUSE_COSINE: f32 = 0.83282;
const WATER_DIFFUSE_TRANSMITTANCE: f32 = 0.93249;

// Water's optics in eight spectral bands, the first four then the last four.
// The bands split the visible at equal steps of the logarithm of pure water's
// absorption between 430 and 700 nm: at 450, 493.5, 509, 549.5, 583.5, 597
// and 650.5 nm. Each takes its optics at its centre weighted by sunlight's
// luminance: 437.8, 478.0, 501.8, 531.2, 565.9, 589.8, 615.8 and 665.3 nm.
// Against a 1 nm integration of the same optics, the bands stay within
// ΔE*ab 2.1 of white, 1.0 on average, from no water to deep water, clear to
// 20 g/m³ of sediment, over sand and grey beds, overhead and oblique.
struct WaterBands {
    low: vec4<f32>,
    high: vec4<f32>,
};

// Pure water's absorption by Pope and Fry (1997, Appl. Opt. 36), Smith and
// Baker's (1981, Appl. Opt. 20) beyond 727.5 nm, plus a faked absorbing
// background of 0.02 /m at 440 nm with Bricaud et al.'s (1981, Limnol.
// Oceanogr. 26) slope of 0.014 /nm, in /m. The background stands in for the
// dissolved and particulate matter that keeps the clearest ocean from being
// pure water.
const WATER_ABSORPTION = WaterBands(
    vec4<f32>(0.02649, 0.023966, 0.03058, 0.049588),
    vec4<f32>(0.068739, 0.13629, 0.27038, 0.43075),
);
// Seawater's backscattering, half its scattering 0.00288 (λ/500 nm)^-4.32 /m
// (Morel 1974).
const WATER_BACKSCATTERING = WaterBands(
    vec4<f32>(0.0025557, 0.0017495, 0.0014173, 0.001109),
    vec4<f32>(0.00084333, 0.00070567, 0.0005857, 0.0004192),
);
// Mineral sediment's absorption per g/m³, 0.041 e^(-0.0123 (λ - 443 nm))
// m²/g (Babin et al. 2003, J. Geophys. Res. 108, 3211).
const SEDIMENT_ABSORPTION = WaterBands(
    vec4<f32>(0.043697, 0.026667, 0.019883, 0.013861),
    vec4<f32>(0.0090395, 0.0067426, 0.0048973, 0.0026619),
);
// Mineral sediment's backscattering per g/m³, 0.010 (555 nm/λ)^0.5 m²/g:
// within the 0.005-0.02 m²/g of Woźniak and Stramski (2004, Appl. Opt. 43)
// and the 0.0137 ± 0.01 m²/g of Reynolds et al. (2016, Limnol. Oceanogr. 61),
// with the spectral slope Babin, Morel et al. (2003, Limnol. Oceanogr. 48)
// assume.
const SEDIMENT_BACKSCATTERING = WaterBands(
    vec4<f32>(0.011259, 0.010776, 0.010516, 0.010222),
    vec4<f32>(0.009903, 0.0097008, 0.0094939, 0.0091334),
);
// Each band's share of each linear sRGB channel's response to a 5778 K
// blackbody through the CIE 1931 2-degree colour-matching functions
// (CIE 018:2019, 360-830 nm), the outer bands reaching their ends. Band
// reflectances so become a reflectance under the Sun's white, which light
// colours multiply as any other.
const WATER_BANDS_TO_RGB_LOW = mat4x3<f32>(
    vec3<f32>(0.055157, -0.06083, 0.52409),
    vec3<f32>(-0.065016, 0.036054, 0.56508),
    vec3<f32>(-0.084185, 0.097563, 0.033629),
    vec3<f32>(-0.25332, 0.54754, -0.040186),
);
const WATER_BANDS_TO_RGB_HIGH = mat4x3<f32>(
    vec3<f32>(0.22982, 0.36321, -0.053045),
    vec3<f32>(0.22981, 0.052383, -0.012541),
    vec3<f32>(0.79738, -0.02527, -0.016221),
    vec3<f32>(0.090358, -0.010659, -0.00080973),
);
// Band reflectances of a linear sRGB albedo: of the spectra that project
// back to it exactly, the one with the least squared second differences
// across bands, plus 10⁻³ of its own square. Grey stays flat.
const RGB_TO_WATER_BANDS_LOW = mat3x4<f32>(
    vec4<f32>(0.086158, -0.025608, -0.094766, -0.053177),
    vec4<f32>(-0.21064, 0.31831, 0.76387, 0.99217),
    vec4<f32>(1.1248, 0.7071, 0.33043, 0.060854),
);
const RGB_TO_WATER_BANDS_HIGH = mat3x4<f32>(
    vec4<f32>(0.15201, 0.48777, 0.8881, 1.2943),
    vec4<f32>(0.89874, 0.5474, 0.067748, -0.42505),
    vec4<f32>(-0.050431, -0.034836, 0.043988, 0.12936),
);

// A body of water's absorption a, then its backscattering b_b, in each band.
struct InherentOptics {
    absorption: WaterBands,
    backscattering: WaterBands,
};

// Seawater with the absorbing background and no mineral sediment, which open
// water holds none of, as Babin et al. (2003, Limnol. Oceanogr. 48) find
// oceanic particles organic and of low density. The faked absorbing
// background stands in for their absorption. Compromises: their
// backscattering, which brightens the clearest ocean a little, is left out;
// and coastal water holds no more, though waves stir sediment from a bed in
// proportion to their stress on it where sediment supply and grain size
// allow.
const SEAWATER_OPTICS = InherentOptics(WATER_ABSORPTION, WATER_BACKSCATTERING);

// Seawater holding `sediment_g_m3` of suspended mineral sediment.
fn mineral_sediment_optics(sediment_g_m3: f32) -> InherentOptics {
    return InherentOptics(
        WaterBands(
            WATER_ABSORPTION.low + sediment_g_m3 * SEDIMENT_ABSORPTION.low,
            WATER_ABSORPTION.high + sediment_g_m3 * SEDIMENT_ABSORPTION.high,
        ),
        WaterBands(
            WATER_BACKSCATTERING.low + sediment_g_m3 * SEDIMENT_BACKSCATTERING.low,
            WATER_BACKSCATTERING.high + sediment_g_m3 * SEDIMENT_BACKSCATTERING.high,
        ),
    );
}

// Water's attenuation a + b_b and its backscattered share u = b_b/(a + b_b),
// in each band.
struct WaterOptics {
    attenuation: WaterBands,
    backscattered: WaterBands,
};

fn water_optics(inherent: InherentOptics) -> WaterOptics {
    let backscattering = inherent.backscattering;
    let attenuation = WaterBands(
        inherent.absorption.low + backscattering.low,
        inherent.absorption.high + backscattering.high,
    );
    return WaterOptics(
        attenuation,
        WaterBands(backscattering.low / attenuation.low, backscattering.high / attenuation.high),
    );
}

// Gordon et al.'s (1988, J. Geophys. Res. 93) reflectance below the surface
// of deep water, per steradian, from its backscattered share.
fn deep_water_reflectance(backscattered: vec4<f32>) -> vec4<f32> {
    return (0.0949 + 0.0794 * backscattered) * backscattered;
}

// Whether a bed `depth_m` below the surface adds at least 1% to the body of
// water of `inherent` optics in some band, for a bed of albedo one under a
// light and view at the zenith, where it shows most. Sediment only hides the
// bed sooner, so no bed deeper than seawater's shows.
fn water_shows_bed(depth_m: f32, inherent: InherentOptics) -> bool {
    var shows = false;
    if depth_m < bed_unseen_depth_m(SEAWATER_OPTICS) || EVALUATE_ALL_BRANCHES {
        shows = depth_m < bed_unseen_depth_m(SEAWATER_OPTICS)
            && depth_m < bed_unseen_depth_m(inherent);
    }
    return shows;
}

// The depth below the surface beyond which the bed adds less than 1% to the
// body of water of `inherent` optics in every band, for a bed of albedo one
// under a light and view at the zenith, where it shows most.
fn bed_unseen_depth_m(inherent: InherentOptics) -> f32 {
    let optics = water_optics(inherent);
    let u = optics.backscattered;
    let depth = max(
        -log(vec4<f32>(0.01 * PI) * deep_water_reflectance(u.low)) / (2.0 * optics.attenuation.low),
        -log(vec4<f32>(0.01 * PI) * deep_water_reflectance(u.high))
            / (2.0 * optics.attenuation.high),
    );
    return max(max(depth.x, depth.y), max(depth.z, depth.w));
}

// The depth below the surface beyond which water reads nothing from the
// bake: clear water hides its bed there, and coastal detail and depth-limited
// waves end within 32 m of the surface.
fn baked_bed_reach_m() -> f32 {
    return max(32.0, bed_unseen_depth_m(SEAWATER_OPTICS));
}

// A surface's unresolved slopes' mean squares along x and y, then their
// covariance.
fn unresolved_slope_covariance(surface: TerrainSurface) -> vec3<f32> {
    let half_variance = 0.5 * surface.unresolved_slope * surface.unresolved_slope;
    let anisotropy = surface.unresolved_slope_anisotropy;
    return vec3<f32>(half_variance + anisotropy.x, half_variance - anisotropy.x, anisotropy.y);
}

// π times the radiance water reflects from a beam toward `view`, per unit of
// the beam's light on a surface facing it: π R p / (4 cos θ_v cos⁴ β), with
// R the Fresnel reflectance and p the density of the facet slope that mirrors
// the beam toward the view, tilted β from level. Facet slopes spread
// normally about the resolved `gradient` with the unresolved `covariance`,
// widened by the source's disc of `angular_radius_rad`: the disc spreads its
// direction by r²/4 per axis, and the mirroring facet turns half as far.
// Compromises: the slopes are Gaussian, without Cox and Munk's skewness and
// peakedness, and facets neither shadow nor mask one another.
fn water_glint(
    view: vec3<f32>,
    light: vec3<f32>,
    gradient: vec2<f32>,
    covariance: vec3<f32>,
    angular_radius_rad: f32,
) -> f32 {
    let halfway = view + light;
    if halfway.z <= 0.0 {
        return 0.0;
    }
    let facet = normalize(halfway);
    let deviation = -facet.xy / facet.z - gradient;
    let disc_spread = angular_radius_rad * angular_radius_rad / 16.0;
    let spread = covariance + vec3<f32>(disc_spread, disc_spread, 0.0);
    let determinant = spread.x * spread.y - spread.z * spread.z;
    let exponent = (spread.y * deviation.x * deviation.x
        - 2.0 * spread.z * deviation.x * deviation.y
        + spread.x * deviation.y * deviation.y) / determinant;
    let density = exp(-0.5 * exponent) / (TAU * sqrt(determinant));
    // A facet too steep for any density to reach would divide zero by zero.
    if density <= 0.0 {
        return 0.0;
    }
    let facet_z_squared = facet.z * facet.z;
    return PI * water_fresnel_reflectance(dot(view, facet)) * density
        / (4.0 * max(view.z, 1e-4) * facet_z_squared * facet_z_squared);
}

// Linear sRGB above the surface from band reflectances `r` below it, per unit
// of the transmittances into and out of the water: r / (n² (1 - 1.7 r)),
// Lee et al.'s (2002, Appl. Opt. 41) relation, whose 1.7 r is the share of
// the light that the surface reflects back down from below. Times π, as
// reflectances are taken.
fn water_body_rgb(r: WaterBands) -> vec3<f32> {
    let n = WATER_REFRACTIVE_INDEX;
    return PI / (n * n) * (
        WATER_BANDS_TO_RGB_LOW * (r.low / (1.0 - 1.7 * r.low))
            + WATER_BANDS_TO_RGB_HIGH * (r.high / (1.0 - 1.7 * r.high))
    );
}

// `water_body_rgb` of deep seawater.
fn seawater_deep_rgb() -> vec3<f32> {
    let backscattered = water_optics(SEAWATER_OPTICS).backscattered;
    return water_body_rgb(WaterBands(
        deep_water_reflectance(backscattered.low),
        deep_water_reflectance(backscattered.high),
    ));
}

// Lekner and Dorf's (1988, Appl. Opt. 27, eq. 11) ratio of the light a
// mineral grain passes in under water to that under air, [1 - R̄(n_r/n_l)] /
// [1 - R̄(n_r)] with Stern's mean reflectance R̄ of diffuse light: 1.07 for
// a grain of index 1.5 under water, as they give it.
const IMMERSED_GRAIN_TRANSMITTANCE_RATIO: f32 = 1.07;

// Band reflectances below the surface of a bed of linear `albedo`, per
// steradian. Water against the grains reflects less light back out of them
// than air, so they absorb more: Lekner and Dorf's absorptance
// a_w = a_d [(1 - a_d) K + a_d] for the dry absorptance a_d in each band,
// which keeps a strong absorber's and raises a weak one's by the ratio K.
// Compromise: every bed takes quartz's K, about 1.08 and 1.10 for minerals
// of index 2 and 2.5.
fn immersed_bed_bands(albedo: vec3<f32>) -> WaterBands {
    let low = clamp(RGB_TO_WATER_BANDS_LOW * albedo, vec4<f32>(0.0), vec4<f32>(1.0));
    let high = clamp(RGB_TO_WATER_BANDS_HIGH * albedo, vec4<f32>(0.0), vec4<f32>(1.0));
    let k = IMMERSED_GRAIN_TRANSMITTANCE_RATIO;
    return WaterBands(
        (1.0 - (1.0 - low) * ((low * k) + 1.0 - low)) / PI,
        (1.0 - (1.0 - high) * ((high * k) + 1.0 - high)) / PI,
    );
}

// What water's body returns toward the view per unit of each light: diffuse
// light, then each beam's light on level ground.
struct WaterBodyResponse {
    diffuse: vec3<f32>,
    sun: vec3<f32>,
    moon: vec3<f32>,
};

// The body of water `depth_m` deep over a bed of linear `bed_albedo`, of
// `inherent` optics, whose light on the bed per
// unit of level ground's is `bed_light`, for diffuse light, then each beam.
// Below the surface it reflects deep water's r∞ and the immersed bed's A/π
// as Maritorena, Morel and Gentili (1994, Limnol. Oceanogr. 39) model shallow
// water,
// r∞ (1 - e) + e A/π with e = exp(-(a + b_b)(1/μ_d + 1/μ_u) H), for the mean
// cosines of each light and of the view refracted through the level surface.
// Each light enters through the level surface's Fresnel transmittance, and
// the body leaves toward the view through `view_transmittance`; `view_z` and
// `light_z` are the view's and each beam's cosines to the vertical.
// `visibility` scales deep water's reflectance, then the bed's. Compromises:
// r∞ keeps Gordon's nadir form for every path; light scattered up takes the
// refracted view's path, which Lee et al. (1998, Appl. Opt. 37) find
// lengthened by 1.04 (1 + 5.4 u)^0.5 from the bed and 1.03 (1 + 2.4 u)^0.5
// from the column, about 1.2 times from the bed in clear water's clearest
// band, so the bed shows a little deeper than it should, where separate
// paths for the column and the bed would take two exponentials per light
// rather than one; the bed is the one beneath the surface point, not where
// the refracted view meets it, H tan θ_w away;
// bands take their reflectance under the Sun's white; and clear shallow
// water, which lies outside sRGB, clips at zero, as do bands of saturated bed
// colours.
fn water_body_response(
    bed_albedo: vec3<f32>,
    depth_m: f32,
    inherent: InherentOptics,
    view_transmittance: f32,
    view_z: f32,
    light_z: vec2<f32>,
    visibility: vec2<f32>,
    bed_light: vec3<f32>,
) -> WaterBodyResponse {
    let optics = water_optics(inherent);
    let u = optics.backscattered;
    let deep = WaterBands(
        visibility.x * deep_water_reflectance(u.low),
        visibility.x * deep_water_reflectance(u.high),
    );
    let light_transmittance = vec3<f32>(
        WATER_DIFFUSE_TRANSMITTANCE,
        1.0 - water_fresnel_reflectance(light_z.x),
        1.0 - water_fresnel_reflectance(light_z.y),
    ) * view_transmittance;
    // Seawater takes its deep reflectance from constant optics; the
    // visibility gate is zero or one, which commutes with `water_body_rgb`.
    var deep_rgb = visibility.x * seawater_deep_rgb();
    let turbid = any(inherent.absorption.low != SEAWATER_OPTICS.absorption.low)
        || any(inherent.absorption.high != SEAWATER_OPTICS.absorption.high)
        || any(inherent.backscattering.low != SEAWATER_OPTICS.backscattering.low)
        || any(inherent.backscattering.high != SEAWATER_OPTICS.backscattering.high);
    if turbid || EVALUATE_ALL_BRANCHES {
        deep_rgb = select(deep_rgb, water_body_rgb(deep), turbid);
    }
    var body = WaterBodyResponse(
        light_transmittance.x * deep_rgb,
        light_transmittance.y * deep_rgb,
        light_transmittance.z * deep_rgb,
    );
    let bed_shows = water_shows_bed(depth_m, inherent);
    if bed_shows || EVALUATE_ALL_BRANCHES {
        let bed = immersed_bed_bands(bed_albedo);
        let column = ShallowWater(
            deep,
            WaterBands(visibility.y * bed.low, visibility.y * bed.high),
            optics.attenuation,
            depth_m / water_refracted_cosine(view_z),
        );
        body.diffuse = select(
            body.diffuse,
            light_transmittance.x
                * shallow_water_rgb(column, depth_m / WATER_DIFFUSE_COSINE, bed_light.x),
            bed_shows,
        );
        // A light below the horizon passes none of its beam into the water.
        if (bed_shows && light_transmittance.y > 0.0) || EVALUATE_ALL_BRANCHES {
            body.sun = select(
                body.sun,
                light_transmittance.y * shallow_water_rgb(
                    column,
                    depth_m / water_refracted_cosine(light_z.x),
                    bed_light.y,
                ),
                bed_shows && light_transmittance.y > 0.0,
            );
        }
        if (bed_shows && light_transmittance.z > 0.0) || EVALUATE_ALL_BRANCHES {
            body.moon = select(
                body.moon,
                light_transmittance.z * shallow_water_rgb(
                    column,
                    depth_m / water_refracted_cosine(light_z.y),
                    bed_light.z,
                ),
                bed_shows && light_transmittance.z > 0.0,
            );
        }
    }
    return WaterBodyResponse(
        max(body.diffuse, vec3<f32>(0.0)),
        max(body.sun, vec3<f32>(0.0)),
        max(body.moon, vec3<f32>(0.0)),
    );
}

// Shallow water's deep and bed reflectances below the surface, its
// attenuation, and the refracted view's path from the bed up to the surface.
struct ShallowWater {
    deep: WaterBands,
    bed: WaterBands,
    attenuation: WaterBands,
    upward_m: f32,
};

// `water_body_rgb` of shallow water for light that takes `downward_m` down
// to the bed and lights it by `bed_light` of level ground's.
fn shallow_water_rgb(water: ShallowWater, downward_m: f32, bed_light: f32) -> vec3<f32> {
    let path_m = downward_m + water.upward_m;
    return water_body_rgb(WaterBands(
        mix(water.deep.low, bed_light * water.bed.low, exp(-water.attenuation.low * path_m)),
        mix(water.deep.high, bed_light * water.bed.high, exp(-water.attenuation.high * path_m)),
    ));
}

// Ground under a film of water returns each light as `water_body_response`
// returns it from water of no depth, its view through level water of
// `view_z`, and each beam through level water of `light_z`.
fn immersed_ground_response(
    albedo: vec3<f32>,
    view_z: f32,
    light_z: vec2<f32>,
) -> WaterBodyResponse {
    let ground = water_body_rgb(immersed_bed_bands(albedo));
    let transmittance = vec3<f32>(
        WATER_DIFFUSE_TRANSMITTANCE,
        1.0 - water_fresnel_reflectance(light_z.x),
        1.0 - water_fresnel_reflectance(light_z.y),
    ) * (1.0 - water_fresnel_reflectance(view_z));
    return WaterBodyResponse(
        max(transmittance.x * ground, vec3<f32>(0.0)),
        max(transmittance.y * ground, vec3<f32>(0.0)),
        max(transmittance.z * ground, vec3<f32>(0.0)),
    );
}

// How water returns each light toward `view`: its body, by
// `water_body_response` with the resolved surface's Fresnel transmittance
// toward the view and the bed's `bed_light`, and its surface, which mirrors the sky and glints with
// each beam. `visibility` scales the body's deep reflectance, its bed, and
// the surface's reflection. The reflected sky is the clear sky's radiance,
// that of an overcast canopy, and for the light ground and sky exchange and
// the night sky an isotropic one, in the direction the resolved surface
// mirrors the view. Below the horizon a light's sky keeps that shape for the
// twilight glow of the air it still lights directly, and the light scattered
// more than once into the planet's shadow is isotropic. Compromises: the glow
// takes the clear sky's shape for a light at the horizon; the multiply
// scattered light keeps a broad glow toward the set light that its isotropic
// stand-in loses, so water brightens against land where the directly lit
// glow gives way to it; and the unresolved slopes neither blur the reflected
// sky nor spread its Fresnel reflectance.
fn water_light_response(
    bed_albedo: vec3<f32>,
    depth_m: f32,
    inherent: InherentOptics,
    surface: TerrainSurface,
    view: vec3<f32>,
    direct_light: CelestialReceiverLight,
    ambient_light: AtmosphericAmbientLight,
    visibility: vec3<f32>,
    bed_light: vec3<f32>,
) -> SurfaceLightResponse {
    let normal = normalize(vec3<f32>(-surface.detail_gradient, 1.0));
    let surface_reflectance = water_fresnel_reflectance(dot(view, normal));
    let body = water_body_response(
        bed_albedo,
        depth_m,
        inherent,
        1.0 - surface_reflectance,
        view.z,
        vec2<f32>(direct_light.sun_direction.z, direct_light.moon_direction.z),
        visibility.xy,
        bed_light,
    );
    let mirrored = reflect(-view, normal);
    let sky_direction = normalize(vec3<f32>(mirrored.xy, max(mirrored.z, 0.0)));
    var clear_sky = 1.0;
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let direction = celestial_source_light(direct_light, source_index).direction;
        let horizontal = length(direction.xy);
        let shaping_direction = select(
            vec3<f32>(0.0, 0.0, 1.0),
            vec3<f32>(direction.xy / horizontal, 0.0),
            horizontal > 0.0,
        );
        clear_sky += ambient_light.clear_sky_source_shares[source_index]
            * directly_lit_twilight_share(direction.z)
            * (clear_sky_radiance_factor(
                sky_direction,
                select(shaping_direction, direction, direction.z > 0.0),
            ) - 1.0);
    }
    // Moon and Spencer's (1942) overcast sky, of radiance in proportion to
    // 1 + 2 cos Z, carrying the canopy's diffuse light.
    let canopy = 3.0 * (1.0 + 2.0 * sky_direction.z) / 7.0;
    let sky = visibility.z * surface_reflectance;
    let covariance = unresolved_slope_covariance(surface);
    let split = ambient_light.split;
    var glint = vec2<f32>(0.0);
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let source = celestial_source_light(direct_light, source_index);
        glint[source_index] = visibility.z * split.normal[source_index] * water_glint(
            view,
            source.direction,
            surface.detail_gradient,
            covariance,
            source.angular_radius_rad,
        );
    }
    return SurfaceLightResponse(
        body.diffuse + sky * clear_sky,
        body.diffuse + sky * canopy,
        body.diffuse + sky,
        body.sun * split.direct.x + glint.x,
        body.moon * split.direct.y + glint.y,
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
// Every landform penumbra spans at least the landform grid's bilinear height
// error, 0.4 m at the 99th percentile and 0.7 m at most over rugged land
// (`LANDFORM_FIELD_SPACING_M` in field_grids.rs), so that error softens shadow
// edges rather than stepping them.
const LANDFORM_PENUMBRA_MINIMUM_M: f32 = 0.6;
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
// below h = (R + b) q - R. Its penumbra spans its minimum plus the disc's diameter
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
            let span_m = (LANDFORM_PENUMBRA_MINIMUM_M + 2.0 * angular_radius_rad * distance_m)
                * ray_scale;
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
            0.0,
            vec2<f32>(0.0),
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
    let plate_slope_scale = surface_height_m * abs(mix(1.0, plate_direction, fracture_amount));
    let micro_unresolved = micro_height_m * material_micro_relief_unresolved_slope(5.0);
    return TerrainSurface(
        color_visibility * rock_color,
        surface_height_m * mix(
            surface_height.gradient,
            plate_direction * surface_height.gradient,
            fracture_amount,
        )
            + micro_height_m * micro_height.gradient,
        sqrt(
            plate_slope_scale * plate_slope_scale
                * material_fractal_unresolved_slope_variance(28.0, 4u, 4u)
                + micro_unresolved * micro_unresolved,
        ),
        vec2<f32>(0.0),
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

fn land_ice_surface(detail: LandIceDetail) -> TerrainSurface {
    let color_visibility = solid_material_color_visibility().z;
    if landscape_visibility.z <= 0.0 {
        return TerrainSurface(
            color_visibility * vec3<f32>(0.78, 0.86, 0.88),
            vec2<f32>(0.0),
            0.0,
            vec2<f32>(0.0),
        );
    }
    return TerrainSurface(
        color_visibility * mix(
            vec3<f32>(0.56, 0.75, 0.80),
            vec3<f32>(0.94, 0.97, 0.96),
            detail.grade,
        ),
        LAND_ICE_DETAIL_NORMAL_SCALE * detail.gradient,
        LAND_ICE_DETAIL_NORMAL_SCALE * material_micro_relief_unresolved_slope(4.0),
        vec2<f32>(0.0),
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
        0.20 * fine.gradient + SEA_ICE_MICRO_RELIEF * micro.gradient,
    );
}

// Sea ice's relief per unit of the micro-relief field's.
const SEA_ICE_MICRO_RELIEF: f32 = 0.16;
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
            0.0,
            vec2<f32>(0.0),
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
        SEA_ICE_MICRO_RELIEF * material_micro_relief_unresolved_slope(4.0),
        vec2<f32>(0.0),
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
        mix(first.unresolved_slope, second.unresolved_slope, blend),
        mix(first.unresolved_slope_anisotropy, second.unresolved_slope_anisotropy, blend),
    );
}
