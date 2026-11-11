struct OceanForcingSample {
    significant_height_m: f32,
    surface_wind_m_s: vec2<f32>,
    upper_wind_m_s: vec2<f32>,
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
    return OceanForcingSample(
        significant_wave_height_m(
            length(surface_wind_m_s),
            length(upper_wind_m_s),
            water_depth_m,
        ),
        surface_wind_m_s,
        upper_wind_m_s,
    );
}

// Significant wave height over water `depth_m` deep under sustained resolved
// winds: wave energy responds to sustained resolved flow, not an
// instantaneous 256 m surface gust.
fn significant_wave_height_m(
    surface_speed_m_s: f32,
    upper_speed_m_s: f32,
    depth_m: f32,
) -> f32 {
    let wind_sea_height_m = 0.05 + 0.016 * surface_speed_m_s * surface_speed_m_s;
    // Swell from the upper wind stands in for swell travelling from distant storms.
    let swell_height_m = 0.006 * upper_speed_m_s * upper_speed_m_s;
    let unrestricted_height_m = length(vec2<f32>(wind_sea_height_m, swell_height_m));
    // 0.78 is McCowan's (1894) breaker limit; the 14 m cap stands in for fetch
    // and duration limits.
    return min(unrestricted_height_m, min(0.78 * depth_m, 14.0));
}

// The resolved wave modes at a position, with the forcing that sets them.
struct WaterWaveSample {
    significant_height_m: f32,
    // The slope the pixel resolves, then the mean squares of that slope along
    // x and y and their covariance over the modes' phases.
    gradient: vec2<f32>,
    slope_covariance: vec3<f32>,
    surface_wind_m_s: vec2<f32>,
};

fn physical_water_wave_sample_from_surface(
    position_m: vec2<f32>,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
    tide: SurfaceTideSample,
) -> WaterWaveSample {
    var sample = WaterWaveSample(0.0, vec2<f32>(0.0), vec3<f32>(0.0), vec2<f32>(0.0));
    let forcing = ocean_forcing_from_surface(
        position_m,
        terrain_elevation_m,
        atmosphere,
        tide,
    );
    // The wind sets the slope statistics of waves too fine to show.
    sample.surface_wind_m_s = forcing.surface_wind_m_s;
    if material_feature_visibility().y <= 0.0 {
        return sample;
    }
    let significant_height_m = forcing.significant_height_m;
    sample.significant_height_m = significant_height_m;
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
        let slope_amplitude = footprint_attenuation * amplitude_m * wave_number;
        sample.gradient -= slope_amplitude * sin(phase) * direction;
        sample.slope_covariance += 0.5 * slope_amplitude * slope_amplitude
            * vec3<f32>(direction * direction, direction.x * direction.y);
    }
    // Compromise: the mean squared slope leaves out this steepening of crests.
    let nonlinear_strength = 0.5 * smoothstep(1.5, 3.0, significant_height_m);
    sample.gradient *= 1.0
        + 2.0 * nonlinear_strength * displacement_m / significant_height_m;
    sample.gradient = select(vec2<f32>(0.0), sample.gradient, significant_height_m > 0.0);
    return sample;
}

fn physical_water_wave_sample(position_m: vec2<f32>) -> WaterWaveSample {
    let terrain = procedural_terrain_sample(position_m);
    return physical_water_wave_sample_from_surface(
        position_m,
        terrain.elevation_m,
        surface_atmosphere_sample_at(position_m),
        surface_tide_sample_at(position_m),
    );
}

// Cox and Munk's (1954, J. Opt. Soc. Am. 44) mean squared slopes of a clean
// sea surface along and across the wind. Their fit puts its 0.003 intercept
// across the wind; it is shared equally here, within the fit's scatter, so the
// slopes lose their wind axis as the wind that sets it calms. Compromise: they
// took the wind at 12.5 m, for which the resolved surface wind stands in.
fn cox_munk_slope_variance(wind_speed_m_s: f32) -> vec2<f32> {
    return vec2<f32>(0.0015 + 3.16e-3 * wind_speed_m_s, 0.0015 + 1.92e-3 * wind_speed_m_s);
}

// Mean squared gradient of `spatiotemporal_noise_sample` times its wavelength
// squared, a property of the noise: a Monte Carlo average over 400 000
// positions, longitudes, and independent lattice gradients.
const SPATIOTEMPORAL_SIMPLEX_SQUARED_GRADIENT: f32 = 4.24;
// The shortest gravity wave, at the gravity-capillary minimum of phase speed.
const GRAVITY_CAPILLARY_WAVELENGTH_M: f32 = 0.017;

// The decorative octaves' slope a pixel resolves and its mean square, then
// the mean squared slope of the waves shorter than the octaves, which nothing
// resolves.
struct DecorativeWaterSample {
    gradient: vec2<f32>,
    slope_variance: f32,
    short_wave_slope_variance: f32,
};

// The height above still water that 2% of waves of significant height
// `height_m` and deep-water peak wavelength `wavelength_m` run up a shore of
// `slope`: Stockdon et al.'s (2006, Coastal Eng. 53) general fit, setup and
// swash together, 1.1 (0.35 β √(H₀L₀) + √(H₀L₀ (0.563 β² + 0.004)) / 2), of
// 0.38 m root-mean-square error. Compromise: it stands for dissipative
// shores too, of Iribarren number β / √(H₀/L₀) below 0.3, where their own
// fit, 0.043 √(H₀L₀), runs from 24% higher on the flattest shores to 22%
// lower at that number for the wind sea's steepness, H₀/L₀ = 1/52, and
// would step against the general one wherever a shore's slope crosses it.
fn wave_run_up_m(height_m: f32, wavelength_m: f32, slope: f32) -> f32 {
    let scale_m2 = height_m * wavelength_m;
    return 1.1 * (
        0.35 * slope * sqrt(scale_m2)
            + 0.5 * sqrt(scale_m2 * (0.563 * slope * slope + 0.004))
    );
}

// The decorative octaves' longest wavelength.
const DECORATIVE_WATER_WAVELENGTH_M: f32 = 18.0;

// The wave spectrum's peak wavelength: the wind sea's, Pierson and
// Moskowitz's (1964) 2πU²/(0.769 g). Compromises: the resolved surface wind
// stands in for the wind at 19.5 m; and the peak is no shorter than the
// decorative octaves reach, half an octave above the longest, so the octave
// layout stands in for gravity waves that outlast a calm of the local wind,
// where a real spectrum falls off above its peak.
fn wave_peak_wavelength_m(wind_speed_m_s: f32) -> f32 {
    return max(
        TAU * wind_speed_m_s * wind_speed_m_s / (0.769 * 9.81),
        1.4142135 * DECORATIVE_WATER_WAVELENGTH_M,
    );
}

// Short waves the wave modes leave out. In Phillips' (1958) saturation range
// each octave of wavelength holds equal slope variance, so each octave
// carries an equal share of Cox and Munk's total over the octaves from the
// spectrum's peak to the shortest gravity wave. The waves shorter than the
// last octave, down to the shortest gravity wave, always keep their share.
// Cox and Munk measured the slopes of all waves, so the octaves share only
// what those and the resolved wave modes leave of that total, their
// `slope_budget`, scaled alike. Compromise: the octaves are alike in every
// direction. Below about 5.5 m/s the peak's floor gives every octave a full
// share, an 18 m octave included.
fn decorative_water_normal_sample(
    position_m: vec2<f32>,
    wind_speed_m_s: f32,
    slope_budget: f32,
) -> DecorativeWaterSample {
    var sample = DecorativeWaterSample(vec2<f32>(0.0), 0.0, 0.0);
    let shown = material_feature_visibility().w > 0.0;
    var wavelength_m = DECORATIVE_WATER_WAVELENGTH_M;
    // The octaves reach half an octave above the first and below the last,
    // which is eight times shorter.
    let octave_range_m = vec2<f32>(1.4142135, 0.0883883) * wavelength_m;
    let peak_wavelength_m = wave_peak_wavelength_m(wind_speed_m_s);
    let total = cox_munk_slope_variance(wind_speed_m_s);
    let phillips_octave_variance = (total.x + total.y)
        / log2(peak_wavelength_m / GRAVITY_CAPILLARY_WAVELENGTH_M);
    sample.short_wave_slope_variance = phillips_octave_variance
        * log2(octave_range_m.y / GRAVITY_CAPILLARY_WAVELENGTH_M);
    let band_variance = phillips_octave_variance * log2(octave_range_m.x / octave_range_m.y);
    let octave_variance = phillips_octave_variance
        * clamp((slope_budget - sample.short_wave_slope_variance) / band_variance, 0.0, 1.0);
    var time_coordinate = scene.scalar_field_time_coordinates.w;
    for (var octave = 0u; octave < 4u; octave++) {
        let evaluated = shown && octave < DECORATIVE_WATER_OCTAVES;
        let octave_visibility = select(
            0.0,
            smoothstep(visual_pixel_footprint_m, 2.0 * visual_pixel_footprint_m, wavelength_m),
            evaluated,
        );
        if evaluated && (octave_visibility > 0.0 || EVALUATE_ALL_BRANCHES) {
            let octave_sample = spatiotemporal_noise_sample(
                noise_key(DECORATIVE_WATER_NOISE_FIELD, octave),
                position_m,
                wavelength_m,
                time_coordinate,
            );
            let amplitude_m = wavelength_m
                * sqrt(octave_variance / SPATIOTEMPORAL_SIMPLEX_SQUARED_GRADIENT);
            sample.gradient += amplitude_m * octave_visibility * octave_sample.gradient;
        }
        sample.slope_variance += octave_visibility * octave_visibility * octave_variance;
        wavelength_m *= 0.5;
        time_coordinate *= 2.0;
    }
    return sample;
}

// The water surface's slope a pixel resolves, from the wave modes and the
// decorative octaves, then the mean squares along x and y and the covariance
// of the slopes too fine for it. Those carry what Cox and Munk's mean squared
// slope leaves beyond the resolved slopes', and always at least the short
// waves' share. While the resolved modes hold no more slope variance than Cox
// and Munk's total less that share, the mean squared slope is theirs at every
// zoom and graphics tier. Compromises: beyond that, the surface is rougher
// than theirs by the modes' excess, alike on every tier; the unresolved
// slopes split between the axes along and across the wind as the positive
// part of the remainder does, so where resolved slopes overfill one axis
// while leaving the short waves' share, that axis keeps no unresolved slopes;
// and the resolved slopes' covariance between those axes is left out.
struct WaterSurfaceSlopes {
    gradient: vec2<f32>,
    unresolved_covariance: vec3<f32>,
};

fn visual_water_slopes_from_surface(
    position_m: vec2<f32>,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
    tide: SurfaceTideSample,
) -> WaterSurfaceSlopes {
    let waves = physical_water_wave_sample_from_surface(
        position_m,
        terrain_elevation_m,
        atmosphere,
        tide,
    );
    let wind_speed_m_s = length(waves.surface_wind_m_s);
    let total = cox_munk_slope_variance(wind_speed_m_s);
    let modes = waves.slope_covariance;
    let decorative = decorative_water_normal_sample(
        position_m,
        wind_speed_m_s,
        total.x + total.y - modes.x - modes.y,
    );
    let upwind = select(
        vec2<f32>(1.0, 0.0),
        waves.surface_wind_m_s / wind_speed_m_s,
        wind_speed_m_s > 0.0,
    );
    let squares = vec2<f32>(upwind.x * upwind.x, upwind.y * upwind.y);
    let cross_term = upwind.x * upwind.y;
    let resolved = vec2<f32>(
        dot(modes.xy, squares) + 2.0 * modes.z * cross_term,
        dot(modes.xy, squares.yx) - 2.0 * modes.z * cross_term,
    ) + 0.5 * decorative.slope_variance;
    // The unresolved slopes hold what the resolved slopes leave of the total,
    // shaped along and across the wind as that remainder is where positive;
    // isotropic octaves leave Cox and Munk's anisotropy to it. Where that
    // falls short of the short waves, the shortfall takes Cox and Munk's
    // proportions, so both axes keep slopes.
    let remainder = total - resolved;
    let shape = max(remainder, vec2<f32>(0.0));
    let left = max(remainder.x + remainder.y, 0.0);
    let unresolved = left * select(
        vec2<f32>(0.5),
        shape / (shape.x + shape.y),
        shape.x + shape.y > 0.0,
    ) + max(decorative.short_wave_slope_variance - left, 0.0) * total / (total.x + total.y);
    return WaterSurfaceSlopes(
        waves.gradient + decorative.gradient,
        vec3<f32>(
            dot(unresolved, squares),
            dot(unresolved, squares.yx),
            (unresolved.x - unresolved.y) * cross_term,
        ),
    );
}

fn visual_water_slopes(position_m: vec2<f32>) -> WaterSurfaceSlopes {
    let terrain = procedural_terrain_sample(position_m);
    return visual_water_slopes_from_surface(
        position_m,
        terrain.elevation_m,
        surface_atmosphere_sample_at(position_m),
        surface_tide_sample_at(position_m),
    );
}
