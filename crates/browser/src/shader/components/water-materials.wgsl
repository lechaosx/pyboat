fn visual_hash_unit(value: u32) -> f32 {
    return f32(value >> 8u) / 16777216.0;
}

// Two independent units from the low and high halves of a hash.
fn visual_hash_unit_pair(value: u32) -> vec2<f32> {
    return vec2<f32>(vec2<u32>(value & 0xffffu, value >> 16u)) / 65536.0;
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
    let contacts_resolved = precipitation > 0.001 && visual_pixel_footprint_m < 0.30;
    if !contacts_resolved && !EVALUATE_ALL_BRANCHES {
        return sample;
    }
    // The contact rate follows the rain index; the cells, ring sizes, and heights
    // stand in for a drop-size distribution.
    let cell_x_cycles = max(1, i32(round(scene.world_size_m.x / 1.50)));
    let cell_size_m = vec2<f32>(scene.world_size_m.x / f32(cell_x_cycles), 1.50);
    let canonical_position = canonical_world_position(position_m);
    let positive_x = select(
        canonical_position.x,
        canonical_position.x + scene.world_size_m.x,
        canonical_position.x < 0.0,
    );
    let cell_position = vec2<f32>(positive_x, position_m.y) / cell_size_m;
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
    let contact_key = noise_key(PRECIPITATION_CONTACT_NOISE_FIELD, 0u);
    for (var offset_y = -1; offset_y <= 1; offset_y++) {
        for (var offset_x = -1; offset_x <= 1; offset_x++) {
            let unwrapped_cell = base_cell + vec2<i32>(offset_x, offset_y);
            let wrapped_cell = vec2<i32>(
                positive_mod(unwrapped_cell.x, cell_x_cycles),
                unwrapped_cell.y,
            );
            let cell_hash = hash32(
                contact_key
                    ^ (u32(wrapped_cell.x) * LATTICE_KEY_MULTIPLIERS[0])
                    ^ (bitcast<u32>(wrapped_cell.y) * LATTICE_KEY_MULTIPLIERS[1]),
            );
            let local_cycle_time = scene.precipitation_phase_and_css_view_height_px.x
                * f32(cycles_per_anchor)
                + visual_hash_unit(cell_hash);
            let local_cycle = u32(floor(local_cycle_time));
            let age = fract(local_cycle_time);
            let impact_low = anchor_cycles.x + local_cycle;
            let impact_epoch = vec2<u32>(
                impact_low,
                anchor_cycles.y + select(0u, 1u, impact_low < anchor_cycles.x),
            );
            let impact_hash = hash32(hash32(cell_hash ^ impact_epoch.x) ^ impact_epoch.y);
            let presence = smoothstep(
                visual_hash_unit(impact_hash) - 0.10,
                visual_hash_unit(impact_hash) + 0.10,
                precipitation,
            );
            // A forced absent contact adds exactly zero through `presence`.
            if presence <= 0.0 && !EVALUATE_ALL_BRANCHES {
                continue;
            }
            let impact_position = 0.05 + 0.90 * visual_hash_unit_pair(hash32(impact_hash));
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
    return noise_sample_or_zero(sample, contacts_resolved);
}

fn precipitation_water_contact_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    return precipitation_water_contact_sample_from_atmosphere(
        position_m,
        surface_atmosphere_sample_at(position_m),
    );
}

fn ocean_current_m_s_from_surface(
    position_m: vec2<f32>,
    terrain_cluster: TerrainNoiseSample,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
    surface_tide: SurfaceTideSample,
) -> vec2<f32> {
    let latitude_rad = radians(surface_latitude_degrees_at(position_m));
    let cluster = terrain_cluster;
    // The open-water envelope and cos² of latitude stand in for basin geometry
    // bounding the gyres.
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
            -open_water * sin(2.0 * latitude_rad)
                * surface_latitude_gradient_rad_per_m_at(position_m),
        );
    let basin_squared = basin * basin;
    let basin_squared_gradient = 2.0 * basin * basin_gradient;

    let gyre = spatiotemporal_fractal_sample(
        OCEAN_CURRENT_NOISE_FIELD,
        position_m,
        0.25 * scene.world_size_m.x,
        scene.ocean_current_time_coordinate_and_padding.x,
        3u,
        0.5,
    );
    let hemisphere = sin(latitude_rad);
    let hemisphere_gradient_y_per_m =
        cos(latitude_rad) * surface_latitude_gradient_rad_per_m_at(position_m);
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

    let tide = ocean_tide_height_sample(atmosphere, surface_tide);
    let tidal_potential_gradient = 1200.0
        * (basin * tide.gradient + tide.value * basin_gradient);
    let current_m_s = rotate_left(base_gradient + weather_gradient) - tidal_potential_gradient;
    return select(vec2<f32>(0.0), current_m_s, tide.value > terrain_elevation_m);
}

fn ocean_forcing_at(position_m: vec2<f32>) -> vec3<f32> {
    let terrain = procedural_terrain_sample(position_m);
    let atmosphere = surface_atmosphere_sample_at(position_m);
    let tide = surface_tide_sample_at(position_m);
    let forcing = ocean_forcing_from_surface(position_m, terrain.elevation_m, atmosphere, tide);
    return vec3<f32>(
        ocean_current_m_s_from_surface(
            position_m,
            terrain.cluster,
            terrain.elevation_m,
            atmosphere,
            tide,
        ),
        combined_sea_height_m(forcing.sea, background_swell(position_m), forcing.water_depth_m),
    );
}

// Water's surface over a bed of `bed_color`, which its body shows.
fn visual_water_surface(
    position_m: vec2<f32>,
    bed_color: vec3<f32>,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> TerrainSurface {
    let slopes = visual_water_slopes_from_surface(
        position_m,
        terrain_elevation_m,
        atmosphere,
        surface_tide_sample,
    );
    let precipitation_contact = precipitation_water_contact_sample_from_atmosphere(
        position_m,
        atmosphere,
    );
    // Compromise: rain's damping of the resolved slopes and its rings' slopes
    // stand outside the Cox and Munk budget the unresolved slopes complete.
    let undisturbed_gradient = mix(
        slopes.gradient,
        0.35 * slopes.gradient,
        clamp(2.0 * precipitation_contact.value, 0.0, 1.0),
    );
    let covariance = slopes.unresolved_covariance;
    return TerrainSurface(
        bed_color,
        undisturbed_gradient + precipitation_contact.gradient,
        sqrt(covariance.x + covariance.y),
        vec2<f32>(0.5 * (covariance.x - covariance.y), covariance.z),
    );
}

// The land composition records the share of its covers that wet: sand,
// intertidal ground and rock, where no snow covers them; the share of those
// that is sand or intertidal ground; and its unresolved slope, which the
// water above hides in its own. Compromises: vegetation, whose leaves wet
// too, never darkens; and snow, whose wetting a film over a diffuse surface
// does not describe, is left out. While `surface_baking` is set, it takes
// the dry, snowless, unseasoned state and also records how its colour
// responds to autumn and dormancy, so the scene applies their live amounts
// per pixel.
struct SurfaceBakeFactors {
    wettable_share: f32,
    sediment_share: f32,
    sand_cover: f32,
    unresolved_slope: f32,
    // Colour changes at full autumn and at full dormancy, each alone.
    autumn_color: vec3<f32>,
    dormancy_color: vec3<f32>,
    surface_grade: f32,
    snow_detail_gradient: vec2<f32>,
};

var<private> surface_baking: bool = false;
var<private> surface_bake_factors: SurfaceBakeFactors;

// Snow's relief per unit of the micro-relief field's.
const SNOW_MICRO_RELIEF: f32 = 0.22;

fn snow_color(surface_grade: f32) -> vec3<f32> {
    return mix(
        vec3<f32>(0.72, 0.76, 0.77),
        vec3<f32>(0.96, 0.97, 0.96),
        0.55 + 0.20 * surface_grade,
    );
}

fn seasoned_grass_color(color: vec3<f32>, autumn: f32, dormancy: f32) -> vec3<f32> {
    let autumn_color = mix(color, vec3<f32>(0.52, 0.28, 0.04), 0.62 * autumn);
    return mix(autumn_color, vec3<f32>(0.23, 0.22, 0.15), 0.42 * dormancy);
}

fn seasoned_forest_color(
    color: vec3<f32>,
    autumn: f32,
    dormancy: f32,
    canopy_cluster: f32,
) -> vec3<f32> {
    let autumn_color = mix(
        color,
        vec3<f32>(0.62, 0.18, 0.025),
        0.80 * autumn * (0.45 + 0.55 * canopy_cluster),
    );
    return mix(autumn_color, vec3<f32>(0.12, 0.13, 0.10), 0.32 * dormancy);
}

fn terrain_surface(
    position_m: vec2<f32>,
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
    if material_feature_visibility().x <= 0.0 {
        return TerrainSurface(
            vec3<f32>(0.16, 0.24, 0.12),
            vec2<f32>(0.0),
            0.0,
            vec2<f32>(0.0),
        );
    }
    let slope = length(material_gradient);
    let is_water = elevation_m < water_height_m;
    let bed_unseen = is_water
        && !water_shows_bed(water_height_m - elevation_m, SEAWATER_OPTICS);
    if bed_unseen && !EVALUATE_ALL_BRANCHES {
        return visual_water_surface(position_m, vec3<f32>(0.0), elevation_m, atmosphere);
    }
    // Ground beneath water keeps the composition it has dry, as the bake
    // holds it, without snow or seasons.
    let dry = surface_baking || is_water;

    var micro_height = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    var micro_unresolved = 0.0;
    if landscape_visibility.z > 0.0 {
        micro_height = material_micro_relief_sample(position_m, 2.5);
        micro_unresolved = material_micro_relief_unresolved_slope(2.5);
    }
    let surface_grade = clamp(
        0.5 + landscape_visibility.z
            * (0.40 * surface + 0.22 * micro_height.value),
        0.0,
        1.0,
    );
    var snow_amount = smoothstep(
        0.10,
        0.55,
        local_snow_cover_from_atmosphere(
            material_elevation_m,
            atmosphere,
        ),
    );
    if dry {
        snow_amount = 0.0;
    }
    let snow_surface = TerrainSurface(
        solid_color_gate.y * snow_color(surface_grade),
        SNOW_MICRO_RELIEF * micro_height.gradient,
        SNOW_MICRO_RELIEF * micro_unresolved,
        vec2<f32>(0.0),
    );
    if snow_amount >= 1.0 && !EVALUATE_ALL_BRANCHES {
        return snow_surface;
    }
    var vegetation = 0.0;
    if landscape_visibility.y > 0.0 {
        vegetation = material_fractal_value(
            VEGETATION_NOISE_FIELD,
            position_m,
            1536.0,
            4u,
        );
    }
    // Exposure and alpine rock lines are compressed to the islands' low relief.
    let altitude_exposure = smoothstep(50.0, 200.0, material_elevation_m);
    let rock_slope_threshold = TERRAIN_CLIFF_SLOPE
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
    let above_beach_bottom = smoothstep(
        beach_bottom_m - 0.8,
        beach_bottom_m + 0.3,
        material_elevation_m,
    );
    // Below the beach, the bed's slope bares rock only beyond sand's angle of
    // rest, which the bed's sand cover takes, rather than by the land's cliff
    // rule; the two meet across the beach's lower edge.
    let rock_cover = clamp(
        1.0
            - (1.0 - above_beach_bottom * slope_rock)
                * (1.0 - outcrop_rock)
                * (1.0 - alpine_rock)
                * (1.0 - coastal_rock),
        0.0,
        1.0,
    );
    let beach_elevation = above_beach_bottom * (1.0 - smoothstep(
        beach_top_m - 0.2,
        beach_top_m + 1.5,
        material_elevation_m,
    ));
    // Below the beach, sand covers the bed in proportion to the beach's
    // sediment b against the share of time M the waves move it, b / (b + M
    // (1 - b)), where supply from the beach and export by the waves balance:
    // the beach's share where the waves always move it, filling quiet beds,
    // and none on bare rock. The bed takes it across the beach's lower edge.
    // Compromises: supply and export run at equal rates; and reefs and
    // headlands shelter no bed, as the waves have no fetch.
    let sand_mobility = seabed_mobility(-material_elevation_m).x;
    let bed_sand = beach_sediment
        / max(beach_sediment + sand_mobility * (1.0 - beach_sediment), 1e-6);
    // The bed's sand rests on slopes up to its angle of rest, which is the
    // same under water as in air (Carrigy 1970, Sedimentology 14): 30-35° for
    // natural sands, figures taken from the paper's body as others cite it
    // (unverified; Kleinhans et al. 2011 cite it for 25-45° by grain shape).
    let sediment_capacity = clamp(
        beach_sediment * beach_elevation * (1.0 - smoothstep(0.10, 0.30, slope))
            + bed_sand * (1.0 - above_beach_bottom) * (1.0 - smoothstep(0.577, 0.700, slope)),
        0.0,
        1.0,
    );
    // Intertidal ground spans the astronomical tide's reach; ground the tide
    // never uncovers is bare rock where sand leaves it, taking it across the
    // same width as the beach's lower edge.
    let intertidal_ceiling_m = 0.5 * tide_reference_range_m();
    let tidal_capacity = 1.0 - smoothstep(
        intertidal_ceiling_m - 0.2,
        intertidal_ceiling_m + 1.0,
        material_elevation_m,
    );
    let above_intertidal_floor = smoothstep(
        -intertidal_ceiling_m - 0.8,
        -intertidal_ceiling_m + 0.3,
        material_elevation_m,
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
        land_color_gate.z * intertidal_color,
        mix(0.12, 0.28, coastal_rock_amount) * micro_height.gradient,
        mix(0.12, 0.28, coastal_rock_amount) * micro_unresolved,
        vec2<f32>(0.0),
    );
    let beach_surface = TerrainSurface(
        land_color_gate.w * beach_color,
        beach_micro_height_m * micro_height.gradient,
        beach_micro_height_m * micro_unresolved,
        vec2<f32>(0.0),
    );

    // A treeline compressed to the islands' low relief.
    let tree_line = 1.0 - smoothstep(150.0, 250.0, material_elevation_m);
    let forest_amount = smoothstep(-0.18, 0.35, vegetation)
        * tree_line
        * (1.0 - smoothstep(0.04, 0.08, slope));
    var canopy_detail = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    var canopy_unresolved = 0.0;
    var canopy_cluster = 0.0;
    let canopy_visible = forest_amount > 0.0 && landscape_visibility.y > 0.0;
    if canopy_visible || EVALUATE_ALL_BRANCHES {
        canopy_detail = noise_sample_or_zero(material_fractal_sample(
            CANOPY_DETAIL_NOISE_FIELD,
            position_m,
            56.0,
            3u,
        ), canopy_visible);
        canopy_unresolved = select(
            0.0,
            sqrt(material_fractal_unresolved_slope_variance(56.0, 3u, 3u)),
            canopy_visible,
        );
        canopy_cluster = select(0.0, smoothstep(
            -0.22,
            0.38,
            canopy_detail.value + 0.32 * vegetation,
        ), canopy_visible);
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
    var autumn = autumn_vegetation_amount_from_anomaly(
        material_elevation_m,
        atmosphere.temperature_anomaly.value,
        atmosphere.climate,
    );
    var dormancy = vegetation_dormancy_from_anomaly(
        material_elevation_m,
        atmosphere.temperature_anomaly.value,
        atmosphere.climate,
    );
    if dry {
        autumn = 0.0;
        dormancy = 0.0;
    }
    let unseasoned_grass_color = grass_color;
    let unseasoned_forest_color = forest_color;
    grass_color = seasoned_grass_color(grass_color, autumn, dormancy);
    forest_color = seasoned_forest_color(forest_color, autumn, dormancy, canopy_cluster);
    let forest_visibility = clamp(
        forest_amount
            * canopy_cluster
            * landscape_visibility.y,
        0.0,
        1.0,
    );
    let vegetation_surface = TerrainSurface(
        land_color_gate.x * (1.0 - forest_visibility) * grass_color
            + land_color_gate.y * forest_visibility * forest_color,
        mix(
            0.45 * micro_height.gradient,
            3.5 * canopy_detail.gradient + 0.24 * micro_height.gradient,
            forest_visibility,
        ),
        mix(
            0.45 * micro_unresolved,
            length(vec2<f32>(3.5 * canopy_unresolved, 0.24 * micro_unresolved)),
            forest_visibility,
        ),
        vec2<f32>(0.0),
    );
    let non_rock_cover = 1.0 - rock_cover;
    let sediment_cover = non_rock_cover * sediment_capacity;
    let non_rock_sediment_cover = non_rock_cover - sediment_cover;
    let tidal_cover = non_rock_sediment_cover * tidal_capacity;
    let intertidal_cover = tidal_cover * above_intertidal_floor;
    let bare_rock_cover = rock_cover + tidal_cover - intertidal_cover;
    let vegetation_cover = non_rock_sediment_cover - tidal_cover;
    var rock_surface = vegetation_surface;
    if bare_rock_cover > 0.0 || EVALUATE_ALL_BRANCHES {
        rock_surface = select_terrain_surface(rock_surface, terrain_rock_surface(
            position_m,
            terrain_gradient,
            material_gradient,
            elevation_m,
        ), bare_rock_cover > 0.0);
    }
    let composed_surface = TerrainSurface(
        vegetation_cover * vegetation_surface.color
            + intertidal_cover * intertidal_surface.color
            + sediment_cover * beach_surface.color
            + bare_rock_cover * rock_surface.color,
        vegetation_cover * vegetation_surface.detail_gradient
            + intertidal_cover * intertidal_surface.detail_gradient
            + sediment_cover * beach_surface.detail_gradient
            + bare_rock_cover * rock_surface.detail_gradient,
        vegetation_cover * vegetation_surface.unresolved_slope
            + intertidal_cover * intertidal_surface.unresolved_slope
            + sediment_cover * beach_surface.unresolved_slope
            + bare_rock_cover * rock_surface.unresolved_slope,
        vec2<f32>(0.0),
    );
    surface_bake_factors.wettable_share = (1.0 - vegetation_cover) * (1.0 - snow_amount);
    surface_bake_factors.sediment_share = (sediment_cover + intertidal_cover)
        / max(1.0 - vegetation_cover, 1e-4);
    surface_bake_factors.sand_cover = sediment_cover;
    surface_bake_factors.unresolved_slope = composed_surface.unresolved_slope;
    if surface_baking {
        // Linear in each amount, so their cross term is dropped.
        let grass_share = land_color_gate.x * vegetation_cover * (1.0 - forest_visibility);
        let forest_share = land_color_gate.y * vegetation_cover * forest_visibility;
        surface_bake_factors.autumn_color = grass_share
            * (seasoned_grass_color(unseasoned_grass_color, 1.0, 0.0) - unseasoned_grass_color)
            + forest_share * (
                seasoned_forest_color(unseasoned_forest_color, 1.0, 0.0, canopy_cluster)
                    - unseasoned_forest_color
            );
        surface_bake_factors.dormancy_color = grass_share
            * (seasoned_grass_color(unseasoned_grass_color, 0.0, 1.0) - unseasoned_grass_color)
            + forest_share * (
                seasoned_forest_color(unseasoned_forest_color, 0.0, 1.0, canopy_cluster)
                    - unseasoned_forest_color
            );
        surface_bake_factors.surface_grade = surface_grade;
        surface_bake_factors.snow_detail_gradient = snow_surface.detail_gradient;
    }
    let land_surface = select_terrain_surface(
        blend_terrain_surface(composed_surface, snow_surface, snow_amount),
        snow_surface,
        snow_amount >= 1.0,
    );
    var water_surface: TerrainSurface;
    if (is_water || EVALUATE_ALL_BRANCHES) && !surface_baking {
        water_surface = visual_water_surface(
            position_m,
            select(composed_surface.color, vec3<f32>(0.0), bed_unseen),
            elevation_m,
            atmosphere,
        );
    }
    return select_terrain_surface(land_surface, water_surface, is_water);
}
