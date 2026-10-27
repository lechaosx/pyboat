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

struct OceanForcingSample {
    significant_height_m: f32,
    surface_wind_m_s: vec2<f32>,
    upper_wind_m_s: vec2<f32>,
}

fn ocean_tide_height_sample(
    atmosphere: SurfaceAtmosphereSample,
    tide: SurfaceTideSample,
) -> TerrainNoiseSample {
    // The seeded range and lag stand in for unresolved basin resonance.
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
    return TerrainNoiseSample(tide_height_m, tide_gradient);
}

fn ocean_forcing_from_surface(
    position_m: vec2<f32>,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
    surface_tide: SurfaceTideSample,
) -> OceanForcingSample {
    let tide_height_m = ocean_tide_height_sample(atmosphere, surface_tide).value;
    let water_depth_m = max(tide_height_m - terrain_elevation_m, 0.0);
    let surface_wind_m_s = resolved_surface_wind_m_s_from_pressure(
        position_m,
        atmosphere.pressure,
    );
    let upper_wind_m_s = upper_wind_m_s_from_atmosphere(position_m, atmosphere);
    if water_depth_m == 0.0 {
        return OceanForcingSample(0.0, surface_wind_m_s, upper_wind_m_s);
    }
    // Wave energy responds to sustained resolved flow, not an instantaneous
    // 256 m surface gust.
    let surface_speed_m_s = length(surface_wind_m_s);
    let upper_speed_m_s = length(upper_wind_m_s);
    let wind_sea_height_m = 0.05 + 0.016 * surface_speed_m_s * surface_speed_m_s;
    // Swell from the upper wind stands in for swell travelling from distant storms.
    let swell_height_m = 0.006 * upper_speed_m_s * upper_speed_m_s;
    let unrestricted_height_m = length(vec2<f32>(wind_sea_height_m, swell_height_m));
    // 0.78 is McCowan's (1894) breaker limit; the 14 m cap stands in for fetch
    // and duration limits.
    return OceanForcingSample(
        min(unrestricted_height_m, min(0.78 * water_depth_m, 14.0)),
        surface_wind_m_s,
        upper_wind_m_s,
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
    return vec3<f32>(
        ocean_current_m_s_from_surface(
            position_m,
            terrain.cluster,
            terrain.elevation_m,
            atmosphere,
            tide,
        ),
        ocean_forcing_from_surface(position_m, terrain.elevation_m, atmosphere, tide)
            .significant_height_m,
    );
}

fn physical_water_wave_sample_from_surface(
    position_m: vec2<f32>,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
    tide: SurfaceTideSample,
) -> TerrainNoiseSample {
    if material_feature_visibility().y <= 0.0 {
        return TerrainNoiseSample(0.0, vec2<f32>(0.0));
    }
    let forcing = ocean_forcing_from_surface(
        position_m,
        terrain_elevation_m,
        atmosphere,
        tide,
    );
    let significant_height_m = forcing.significant_height_m;
    var sample: TerrainNoiseSample;
    sample.value = significant_height_m;
    sample.gradient = vec2<f32>(0.0);
    if significant_height_m <= 0.0 && !EVALUATE_ALL_BRANCHES {
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
    // Swell from the upper wind stands in for swell travelling from distant storms.
    let swell_height_m = 0.006 * upper_speed * upper_speed;
    let energy_direction = wind_sea_height_m * wind_sea_height_m * surface_direction
        + swell_height_m * swell_height_m * upper_direction;
    let energy_direction_length = length(energy_direction);
    let dominant_direction = select(
        vec2<f32>(1.0, 0.0),
        energy_direction / max(energy_direction_length, 0.0001),
        energy_direction_length > 0.0,
    );
    let amplitude_scale = significant_height_m / 5.676266;
    var displacement_m = 0.0;

    for (var mode_index = 0u; mode_index < 16u; mode_index++) {
        let mode = scene.ocean_wave_modes[mode_index];
        let wave_number = mode.z;
        let direction = mode.xy / wave_number;
        let alignment = max(dot(direction, dominant_direction), 0.0);
        let weight = 0.20 + pow(alignment, 4.0);
        let amplitude_m = weight * amplitude_scale;
        let phase = mode.x
            * fract(position_m.x * scene.inverse_world_size_m.x)
            * scene.world_size_m.x
            + mode.y * position_m.y
            + mode.w;
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
    sample.gradient = select(vec2<f32>(0.0), sample.gradient, significant_height_m > 0.0);
    return sample;
}

fn physical_water_wave_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    let terrain = procedural_terrain_sample(position_m);
    return physical_water_wave_sample_from_surface(
        position_m,
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
    // Normalized over all four octaves, so a graphics tier that leaves out
    // finer ones keeps the coarser octaves' amplitudes.
    for (var octave = 0u; octave < 4u; octave++) {
        let octave_visibility = smoothstep(
            visual_pixel_footprint_m,
            2.0 * visual_pixel_footprint_m,
            wavelength_m,
        );
        if octave < DECORATIVE_WATER_OCTAVES
            && (octave_visibility > 0.0 || EVALUATE_ALL_BRANCHES)
        {
            let octave_sample = spatiotemporal_noise_sample(
                noise_key(DECORATIVE_WATER_NOISE_FIELD, octave),
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

// Wave slopes exaggerated 2.5× for top-down reading; the decorative weight
// stands in for short waves the physical spectrum omits.
const WATER_WAVE_SLOPE_SCALE: f32 = 2.5;
const DECORATIVE_WATER_NORMAL_WEIGHT: f32 = 0.20;

fn visual_water_normal_sample_from_surface(
    position_m: vec2<f32>,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
    tide: SurfaceTideSample,
) -> TerrainNoiseSample {
    let physical = physical_water_wave_sample_from_surface(
        position_m,
        terrain_elevation_m,
        atmosphere,
        tide,
    );
    let decorative = decorative_water_normal_sample(position_m);
    return TerrainNoiseSample(
        physical.value,
        WATER_WAVE_SLOPE_SCALE * physical.gradient
            + DECORATIVE_WATER_NORMAL_WEIGHT * decorative.gradient,
    );
}

fn visual_water_normal_sample(position_m: vec2<f32>) -> TerrainNoiseSample {
    let terrain = procedural_terrain_sample(position_m);
    return visual_water_normal_sample_from_surface(
        position_m,
        terrain.elevation_m,
        surface_atmosphere_sample_at(position_m),
        surface_tide_sample_at(position_m),
    );
}

fn visual_water_surface(
    position_m: vec2<f32>,
    base_color: vec3<f32>,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> TerrainSurface {
    let normal_detail = visual_water_normal_sample_from_surface(
        position_m,
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
        undisturbed_gradient + precipitation_contact.gradient,
        mix(
            mix(0.08, 0.20, smoothstep(0.10, 4.0, normal_detail.value)),
            0.26,
            surface_cloud_weather.rain,
        ),
    );
}

// While `surface_baking` is set, the land composition takes the dry,
// snowless, unseasoned state and records how its colour responds to wetness,
// autumn, and dormancy, so the scene applies their live amounts per pixel.
struct SurfaceBakeFactors {
    // Share of the dry colour's luminance that full wetness removes.
    wet_fraction: f32,
    // Colour changes at full autumn and at full dormancy, each alone.
    autumn_color: vec3<f32>,
    dormancy_color: vec3<f32>,
    surface_grade: f32,
    snow_detail_gradient: vec2<f32>,
};

var<private> surface_baking: bool = false;
var<private> surface_bake_factors: SurfaceBakeFactors;

fn bottom_reflectance(
    coastal_rock_amount: f32,
    coastal_outcrop_amount: f32,
    coastal_sediment_amount: f32,
) -> f32 {
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
    let reflectance = mix(0.32, sediment_reflectance, sediment_amount);
    return mix(reflectance, rock_reflectance, rock_amount);
}

fn water_column_surface(
    position_m: vec2<f32>,
    elevation_m: f32,
    water_height_m: f32,
    bottom_reflectance: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> TerrainSurface {
    let water_color_gate = water_material_color_visibility();
    let depth_m = water_height_m - elevation_m;
    let water_color = vec3<f32>(0.025, 0.11, 0.15);
    let bottom_transmittance = exp(
        -2.0 * depth_m / max(tuning.water_tuning.x, 0.01),
    );
    if effect_feature_visibility().y > 0.0 && bottom_transmittance > 0.02 {
        let bottom_water_color = water_color
            * (1.0 + WATER_BOTTOM_REFLECTANCE * bottom_reflectance);
        let water_veiled_bottom = mix(
            bottom_water_color,
            water_color,
            WATER_BOTTOM_VEIL * (1.0 - bottom_transmittance),
        );
        let visible_water_color = water_color_gate.x * water_color
            + water_color_gate.y * bottom_transmittance
                * (water_veiled_bottom - water_color);
        return visual_water_surface(
            position_m,
            visible_water_color,
            elevation_m,
            atmosphere,
        );
    }
    return visual_water_surface(
        position_m,
        water_color_gate.x * water_color,
        elevation_m,
        atmosphere,
    );
}

// Wet intertidal and beach colours as fractions of their dry colours.
const INTERTIDAL_WET_SHADE: f32 = 0.70;
const BEACH_WET_SHADE: f32 = 0.72;

const SNOW_ROUGHNESS: f32 = 0.70;

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
            select(vec3<f32>(0.16, 0.24, 0.12), vec3<f32>(0.025, 0.11, 0.15), elevation_m < water_height_m),
            vec2<f32>(0.0),
            1.0,
        );
    }
    let slope = length(material_gradient);
    let is_water = elevation_m < water_height_m;
    var water_surface: TerrainSurface;
    if (is_water || EVALUATE_ALL_BRANCHES) && !surface_baking {
        water_surface = water_column_surface(
            position_m,
            elevation_m,
            water_height_m,
            bottom_reflectance(
                coastal_rock_amount,
                coastal_outcrop_amount,
                coastal_sediment_amount,
            ),
            atmosphere,
        );
    }
    if is_water && !EVALUATE_ALL_BRANCHES {
        return water_surface;
    }

    var micro_height = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    if landscape_visibility.z > 0.0 {
        micro_height = material_micro_relief_sample(position_m, 2.5);
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
    if surface_baking {
        snow_amount = 0.0;
    }
    let snow_surface = TerrainSurface(
        solid_color_gate.y * snow_color(surface_grade),
        0.22 * micro_height.gradient,
        SNOW_ROUGHNESS,
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
    let rock_cover = clamp(
        1.0
            - (1.0 - slope_rock)
                * (1.0 - outcrop_rock)
                * (1.0 - alpine_rock)
                * (1.0 - coastal_rock),
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
        beach_sediment
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
    var exposed_dryness = smoothstep(
        0.05,
        0.8,
        material_elevation_m - water_height_m,
    );
    if surface_baking {
        exposed_dryness = 1.0;
    }
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
            INTERTIDAL_WET_SHADE * intertidal_color,
            intertidal_color,
            exposed_dryness,
        ),
        mix(0.12, 0.28, coastal_rock_amount) * micro_height.gradient,
        mix(0.96, 0.88, exposed_dryness),
    );
    let beach_surface = TerrainSurface(
        land_color_gate.w * mix(BEACH_WET_SHADE * beach_color, beach_color, exposed_dryness),
        beach_micro_height_m * micro_height.gradient,
        mix(0.90, 0.82, exposed_dryness),
    );

    // A treeline compressed to the islands' low relief.
    let tree_line = 1.0 - smoothstep(150.0, 250.0, material_elevation_m);
    let forest_amount = smoothstep(-0.18, 0.35, vegetation)
        * tree_line
        * (1.0 - smoothstep(0.04, 0.08, slope));
    var canopy_detail = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    var canopy_cluster = 0.0;
    let canopy_visible = forest_amount > 0.0 && landscape_visibility.y > 0.0;
    if canopy_visible || EVALUATE_ALL_BRANCHES {
        canopy_detail = noise_sample_or_zero(material_fractal_sample(
            CANOPY_DETAIL_NOISE_FIELD,
            position_m,
            56.0,
            3u,
        ), canopy_visible);
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
    if surface_baking {
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
        mix(0.88, 0.96, forest_visibility),
    );
    let non_rock_cover = 1.0 - rock_cover;
    let sediment_cover = non_rock_cover * sediment_capacity;
    let non_rock_sediment_cover = non_rock_cover - sediment_cover;
    let intertidal_cover = non_rock_sediment_cover * intertidal_capacity;
    let vegetation_cover = non_rock_sediment_cover - intertidal_cover;
    var rock_surface = vegetation_surface;
    if rock_cover > 0.0 || EVALUATE_ALL_BRANCHES {
        rock_surface = select_terrain_surface(rock_surface, terrain_rock_surface(
            position_m,
            terrain_gradient,
            material_gradient,
            elevation_m,
        ), rock_cover > 0.0);
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
        let drying_color = intertidal_cover * land_color_gate.z
            * (1.0 - INTERTIDAL_WET_SHADE) * intertidal_color
            + sediment_cover * land_color_gate.w * (1.0 - BEACH_WET_SHADE) * beach_color;
        let luminance = vec3<f32>(0.2126, 0.7152, 0.0722);
        surface_bake_factors.wet_fraction = dot(drying_color, luminance)
            / max(dot(composed_surface.color, luminance), 1e-4);
        surface_bake_factors.surface_grade = surface_grade;
        surface_bake_factors.snow_detail_gradient = snow_surface.detail_gradient;
    }
    let land_surface = select_terrain_surface(
        blend_terrain_surface(composed_surface, snow_surface, snow_amount),
        snow_surface,
        snow_amount >= 1.0,
    );
    return select_terrain_surface(land_surface, water_surface, is_water);
}
