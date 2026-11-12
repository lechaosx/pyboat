struct OceanForcingSample {
    significant_height_m: f32,
    surface_wind_m_s: vec2<f32>,
    upper_wind_m_s: vec2<f32>,
    sea: WindSea,
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
    let sea = wind_sea(length(surface_wind_m_s));
    if water_depth_m == 0.0 {
        return OceanForcingSample(0.0, surface_wind_m_s, upper_wind_m_s, sea);
    }
    return OceanForcingSample(
        significant_wave_height_m(sea, length(upper_wind_m_s), water_depth_m),
        surface_wind_m_s,
        upper_wind_m_s,
        sea,
    );
}

// The fully developed sea of a surface wind, as `wind_sea` in
// crates/game/src/ocean.rs gives it: its peak wavenumber, its variance, and
// Mitsuyasu's spreading parameter at its peak.
struct WindSea {
    peak_wavenumber_per_m: f32,
    variance_m2: f32,
    peak_spread: f32,
};

fn wind_sea(surface_speed_m_s: f32) -> WindSea {
    if surface_speed_m_s <= 0.0 {
        return WindSea(1.0e30, 0.0, 0.0);
    }
    let constant = log(
        10.0 * 9.81 / (0.011 * 0.41 * 0.41 * surface_speed_m_s * surface_speed_m_s),
    );
    var log_height = 10.0;
    for (var iteration = 0u; iteration < 4u; iteration++) {
        log_height = constant + 2.0 * log(log_height);
    }
    let speed_m_s = surface_speed_m_s * (1.0 + log(1.95) / log_height);
    let peak_frequency = 0.877 * 9.81 / speed_m_s;
    return WindSea(
        peak_frequency * peak_frequency / 9.81,
        0.0081 * 9.81 * 9.81 / (5.0 * pow(peak_frequency, 4.0)),
        11.5 * pow(surface_speed_m_s * peak_frequency / 9.81, -2.5),
    );
}

// Significant wave height over water `depth_m` deep, of the wind `sea` of
// sustained resolved winds and the swell of the upper wind: wave energy
// responds to sustained resolved flow, not an instantaneous 256 m surface
// gust. Compromise: McCowan's (1894) breaker limit, 0.78 of the depth, caps
// it, a hard cap that a smooth saturation is to replace with the shore's
// waves.
fn significant_wave_height_m(sea: WindSea, upper_speed_m_s: f32, depth_m: f32) -> f32 {
    let wind_sea_height_m = 4.0 * sqrt(sea.variance_m2);
    // Swell from the upper wind stands in for swell travelling from distant storms.
    let swell_height_m = 0.006 * upper_speed_m_s * upper_speed_m_s;
    let unrestricted_height_m = length(vec2<f32>(wind_sea_height_m, swell_height_m));
    return min(unrestricted_height_m, 0.78 * depth_m);
}

// The resolved wave modes at a position, with the forcing that sets them.
struct WaterWaveSample {
    significant_height_m: f32,
    // The surface's height above still water, then the slope the pixel
    // resolves, then the mean squares of that slope along x and y and their
    // covariance over the modes' phases.
    displacement_m: f32,
    gradient: vec2<f32>,
    slope_covariance: vec3<f32>,
    surface_wind_m_s: vec2<f32>,
};

// The forcing of the waves the scene last sampled, which the sediment they
// stir at the same point reuses rather than evaluating the winds again.
var<private> sampled_wave_forcing: OceanForcingSample;

// The waves `ocean_wave_state` in crates/game/src/ocean.rs gives, each mode
// filtered by the pixel's footprint, exp(-(k fp)² / 2), and a band skipped
// once its longest mode falls under about two pixels, k fp ≥ 3, where the
// filter leaves 1%. Compromise: the mean squared slope leaves out Tayfun's
// steepening of crests, 1-2% of the modes' slope variance.
fn physical_water_wave_sample_from_surface(
    position_m: vec2<f32>,
    terrain_elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
    tide: SurfaceTideSample,
) -> WaterWaveSample {
    var sample = WaterWaveSample(0.0, 0.0, vec2<f32>(0.0), vec3<f32>(0.0), vec2<f32>(0.0));
    let forcing = ocean_forcing_from_surface(
        position_m,
        terrain_elevation_m,
        atmosphere,
        tide,
    );
    sampled_wave_forcing = forcing;
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
    let upper_speed = length(forcing.upper_wind_m_s);
    let upper_direction = forcing.upper_wind_m_s / max(upper_speed, 0.0001);
    let sea = forcing.sea;
    let swell_height_m = 0.006 * upper_speed * upper_speed;
    let depth_scale = significant_height_m
        / max(length(vec2<f32>(4.0 * sqrt(sea.variance_m2), swell_height_m)), 1.0e-6);
    let relative_m = camera_relative_position_m(position_m);
    let footprint_m = visual_pixel_footprint_m;

    var displacement_m = 0.0;
    var quadrature_m = 0.0;
    var gradient = vec2<f32>(0.0);
    var quadrature_gradient = vec2<f32>(0.0);
    // Tayfun's mean wavenumber weights each band's centre by its variance
    // over the whole window, so it holds as the footprint drops bands.
    var weighted_wavenumber = 0.0;
    var window_variance = 0.0;
    let spacing = TAU / f32(WAVE_DIRECTIONS);
    let window_width = 0.5 * f32(WAVE_VISITED_DIRECTIONS) * spacing;
    let wind_angle = atan2(surface_wind.y, surface_wind.x);
    let window = WAVE_GAMEPLAY_WINDOW * sea.peak_wavenumber_per_m;
    let first_band = i32(floor(
        WAVE_BANDS_PER_OCTAVE * log2(window.x / WAVE_LOWEST_WAVENUMBER_PER_M),
    ));
    for (var visited = 0u; visited < WAVE_VISITED_BANDS; visited++) {
        let band = first_band + i32(visited);
        let edges = WAVE_LOWEST_WAVENUMBER_PER_M
            * exp2((f32(band) + vec2<f32>(0.0, 1.0)) / WAVE_BANDS_PER_OCTAVE);
        let lower = max(edges.x, window.x);
        let upper = min(edges.y, window.y);
        let in_window = band >= 0 && band < WAVE_BAND_COUNT && upper > lower;
        let ratio = vec2<f32>(lower, upper) / sea.peak_wavenumber_per_m;
        let band_variance = select(
            0.0,
            sea.variance_m2
                * (exp(-1.25 / (ratio.y * ratio.y)) - exp(-1.25 / (ratio.x * ratio.x))),
            in_window,
        );
        weighted_wavenumber += band_variance * sqrt(lower * upper);
        window_variance += band_variance;
        let skipped = !in_window || edges.x * footprint_m >= 3.0;
        if skipped && !EVALUATE_ALL_BRANCHES {
            continue;
        }
        let table_band = u32(clamp(band, 0, WAVE_BAND_COUNT - 1));
        let variance = select(band_variance, 0.0, skipped);
        let frequency_ratio = sqrt(sqrt(ratio.x * ratio.y));
        let spread = sea.peak_spread
            * pow(frequency_ratio, select(-2.5, 5.0, frequency_ratio < 1.0));
        let rotation = scene.ocean_wave_band_rotations[table_band / 4u][table_band % 4u];
        let direction_position = (wind_angle - rotation) / spacing;
        let nearest = floor(direction_position);
        let offset = direction_position - nearest;
        var band_displacement = 0.0;
        var band_quadrature = 0.0;
        var band_gradient = vec2<f32>(0.0);
        var band_quadrature_gradient = vec2<f32>(0.0);
        var band_covariance = vec3<f32>(0.0);
        var band_weight = 0.0;
        for (var direction = 0u; direction < WAVE_VISITED_DIRECTIONS; direction++) {
            let step = f32(direction) + 1.0 - 0.5 * f32(WAVE_VISITED_DIRECTIONS);
            let angle = (step - offset) * spacing;
            let root_weight = pow(cos(0.5 * angle), spread)
                * cos(PI * max(abs(angle) / window_width - 0.5, 0.0));
            let table_direction = u32(i32(nearest) + i32(step)) & (WAVE_DIRECTIONS - 1u);
            let mode = scene.ocean_wave_table[table_band * WAVE_DIRECTIONS + table_direction];
            let filtered = mode.z * footprint_m;
            let amplitude = root_weight * exp(-0.5 * filtered * filtered);
            let phase = dot(mode.xy, relative_m) + mode.w;
            let sine = sin(phase);
            let cosine = cos(phase);
            band_displacement += amplitude * cosine;
            band_quadrature += amplitude * sine;
            band_gradient -= amplitude * sine * mode.xy;
            band_quadrature_gradient += amplitude * cosine * mode.xy;
            band_covariance += amplitude * amplitude
                * vec3<f32>(mode.xy * mode.xy, mode.x * mode.y);
            band_weight += root_weight * root_weight;
        }
        let squared_scale = 2.0 * variance / max(band_weight, 1.0e-20);
        let scale = sqrt(squared_scale) * depth_scale;
        displacement_m += scale * band_displacement;
        quadrature_m += scale * band_quadrature;
        gradient += scale * band_gradient;
        quadrature_gradient += scale * band_quadrature_gradient;
        sample.slope_covariance += 0.5 * squared_scale * depth_scale * depth_scale
            * band_covariance;
    }
    let mean_wavenumber = weighted_wavenumber / max(window_variance, 1.0e-20);
    let linear_m = displacement_m;
    displacement_m += 0.5 * mean_wavenumber
        * (linear_m * linear_m - quadrature_m * quadrature_m);
    gradient += mean_wavenumber * (linear_m * gradient - quadrature_m * quadrature_gradient);

    var swell_weights: array<f32, OCEAN_SWELL_MODE_COUNT>;
    var swell_weight_squares = 0.0;
    for (var mode_index = 0u; mode_index < OCEAN_SWELL_MODE_COUNT; mode_index++) {
        let mode = scene.ocean_swell_modes[mode_index];
        let weight = 0.20 + pow(max(dot(mode.xy / mode.z, upper_direction), 0.0), 4.0);
        swell_weights[mode_index] = weight;
        swell_weight_squares += weight * weight;
    }
    let swell_scale = depth_scale * swell_height_m
        / (4.0 * sqrt(0.5 * swell_weight_squares));
    for (var mode_index = 0u; mode_index < OCEAN_SWELL_MODE_COUNT; mode_index++) {
        let mode = scene.ocean_swell_modes[mode_index];
        let filtered = mode.z * footprint_m;
        let amplitude_m = swell_scale * swell_weights[mode_index]
            * exp(-0.5 * filtered * filtered);
        let phase = dot(mode.xy, relative_m) + mode.w;
        displacement_m += amplitude_m * cos(phase);
        gradient -= amplitude_m * sin(phase) * mode.xy;
        sample.slope_covariance += 0.5 * amplitude_m * amplitude_m
            * vec3<f32>(mode.xy * mode.xy, mode.x * mode.y);
    }
    sample.displacement_m = displacement_m;
    sample.gradient = gradient;
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

// The wave spectrum's peak wavelength: the wind sea's. Compromise: the peak
// is no shorter than the decorative octaves reach, half an octave above the
// longest, so the octave layout stands in for gravity waves that outlast a
// calm of the local wind, where a real spectrum falls off above its peak.
fn wave_peak_wavelength_m(sea: WindSea) -> f32 {
    return max(
        TAU / sea.peak_wavenumber_per_m,
        1.4142135 * DECORATIVE_WATER_WAVELENGTH_M,
    );
}

// The period of waves of `wavelength_m` in deep water, √(2πλ/g).
fn wave_period_s(wavelength_m: f32) -> f32 {
    return sqrt(TAU * wavelength_m / 9.81);
}

// The peak orbital speed at a bed `depth_m` down under waves of significant
// height `height_m` and deep-water peak wavelength `wavelength_m`: linear
// wave theory's π H / (T sinh kh), with kh from the peak's deep-water k₀h by
// Eckart's (1952) explicit approximation of the dispersion relation,
// k₀h / √tanh(k₀h), within 5%.
fn wave_orbital_speed_at_bed_m_s(height_m: f32, wavelength_m: f32, depth_m: f32) -> f32 {
    let deep_water_depth = TAU * max(depth_m, 0.01) / wavelength_m;
    let wavenumber_depth = deep_water_depth / sqrt(tanh(deep_water_depth));
    return PI * height_m / (wave_period_s(wavelength_m) * sinh(wavenumber_depth));
}

// The orbital speed at the bed that moves quartz sand of 0.25 mm under waves
// of `period_s`: Komar and Miller's (1973) threshold for grains below 0.5 mm,
// [0.118 g (s - 1)]^(2/3) D^(1/3) T^(1/3) with s = 2.65, as Soulsby (1997)
// gives it; 0.176 m/s at 6 s.
fn sand_threshold_orbital_speed_m_s(period_s: f32) -> f32 {
    return 0.096977 * pow(period_s, 1.0 / 3.0);
}

// The orbital speed at the bed that lifts fine sediment: under a wave
// friction factor of 0.02 it exerts the 0.06-0.1 Pa critical stress of
// fine-sediment resuspension models (Chung et al. 2009, Water Resour. Res. 45;
// Green and Coco 2014, Rev. Geophys. 52).
const FINE_SEDIMENT_THRESHOLD_ORBITAL_SPEED_M_S: f32 = 0.1;
// The wave friction factor that turns the orbital speed at the bed into its
// stress there, as that threshold takes it.
const WAVE_FRICTION_FACTOR: f32 = 0.02;
const SEAWATER_DENSITY_KG_M3: f32 = 1025.0;

// The wind climate's shape k and scale c: a Weibull distribution of the
// resolved surface wind, fitted to the game's wind sampled over places and
// seasons, so a wind U is exceeded exp(-(U/c)^k) of the time.
const WIND_CLIMATE_WEIBULL = vec2<f32>(1.7, 9.0);

// The share of time the waves at a bed `depth_m` below mean sea level move
// sand, then fine sediment: the share of the wind climate's winds whose wind
// sea moves each past its threshold orbital speed, at depths 2^(i/8 - 2) m,
// from 0.25 to 256 m. Compromises: one climate stands for every place, as the
// waves have no fetch; the upper wind's swell, which adds up to 0.03, is left
// out; and the tide does not change the depth. Its test,
// `the_seabed_mobility_table_follows_the_wave_climate`, prints it afresh from
// the functions above.
const SEABED_MOBILITY = array<vec2<f32>, 81>(
    vec2<f32>(0.95532, 0.96875), vec2<f32>(0.95361, 0.96777), vec2<f32>(0.95190, 0.96655),
    vec2<f32>(0.95019, 0.96509), vec2<f32>(0.94824, 0.96387), vec2<f32>(0.94629, 0.96240),
    vec2<f32>(0.94434, 0.96118), vec2<f32>(0.94214, 0.95947), vec2<f32>(0.93994, 0.95801),
    vec2<f32>(0.93774, 0.95630), vec2<f32>(0.93530, 0.95483), vec2<f32>(0.93286, 0.95288),
    vec2<f32>(0.93017, 0.95117), vec2<f32>(0.92749, 0.94922), vec2<f32>(0.92456, 0.94726),
    vec2<f32>(0.92163, 0.94507), vec2<f32>(0.91846, 0.94287), vec2<f32>(0.91528, 0.94043),
    vec2<f32>(0.91186, 0.93799), vec2<f32>(0.90820, 0.93530), vec2<f32>(0.90430, 0.93262),
    vec2<f32>(0.90014, 0.92969), vec2<f32>(0.89575, 0.92651), vec2<f32>(0.89087, 0.92309),
    vec2<f32>(0.88598, 0.91943), vec2<f32>(0.88037, 0.91553), vec2<f32>(0.87451, 0.91113),
    vec2<f32>(0.86792, 0.90649), vec2<f32>(0.86084, 0.90137), vec2<f32>(0.85303, 0.89551),
    vec2<f32>(0.84424, 0.88916), vec2<f32>(0.83447, 0.88208), vec2<f32>(0.82324, 0.87402),
    vec2<f32>(0.81079, 0.86474), vec2<f32>(0.79638, 0.85400), vec2<f32>(0.77978, 0.84155),
    vec2<f32>(0.76049, 0.82715), vec2<f32>(0.73828, 0.81030), vec2<f32>(0.71191, 0.79004),
    vec2<f32>(0.68090, 0.76587), vec2<f32>(0.65869, 0.73706), vec2<f32>(0.64135, 0.70239),
    vec2<f32>(0.62329, 0.66674), vec2<f32>(0.60473, 0.65014), vec2<f32>(0.58569, 0.63305),
    vec2<f32>(0.56591, 0.61547), vec2<f32>(0.54540, 0.59716), vec2<f32>(0.52465, 0.57812),
    vec2<f32>(0.50317, 0.55883), vec2<f32>(0.48145, 0.53881), vec2<f32>(0.45924, 0.51806),
    vec2<f32>(0.43653, 0.49708), vec2<f32>(0.41358, 0.47559), vec2<f32>(0.39063, 0.45386),
    vec2<f32>(0.36744, 0.43165), vec2<f32>(0.34400, 0.40919), vec2<f32>(0.32105, 0.38672),
    vec2<f32>(0.29786, 0.36402), vec2<f32>(0.27515, 0.34107), vec2<f32>(0.25293, 0.31836),
    vec2<f32>(0.23096, 0.29590), vec2<f32>(0.20972, 0.27369), vec2<f32>(0.18946, 0.25171),
    vec2<f32>(0.16968, 0.23047), vec2<f32>(0.15088, 0.20948), vec2<f32>(0.13330, 0.18946),
    vec2<f32>(0.11670, 0.17017), vec2<f32>(0.10108, 0.15161), vec2<f32>(0.08692, 0.13428),
    vec2<f32>(0.07398, 0.11768), vec2<f32>(0.06226, 0.10254), vec2<f32>(0.05176, 0.08838),
    vec2<f32>(0.04248, 0.07544), vec2<f32>(0.03442, 0.06372), vec2<f32>(0.02759, 0.05322),
    vec2<f32>(0.02173, 0.04395), vec2<f32>(0.01685, 0.03565), vec2<f32>(0.01270, 0.02881),
    vec2<f32>(0.00952, 0.02271), vec2<f32>(0.00708, 0.01782), vec2<f32>(0.00513, 0.01367),
);

fn seabed_mobility(depth_m: f32) -> vec2<f32> {
    let index = clamp(8.0 * log2(max(depth_m, 0.25)) + 16.0, 0.0, 80.0);
    let lower = min(u32(index), 79u);
    return mix(SEABED_MOBILITY[lower], SEABED_MOBILITY[lower + 1u], index - f32(lower));
}

// The times in a row the wind climate must leave a bed still for fine
// sediment to settle in its sand and consolidate there. Calibrated so a bed
// all sand 5 m deep holds 1.5 g/m³ under the climate's median wind, the
// median background of the fringing reefs of Magnetic Island, within their
// 1-2 mg/L (Waterhouse et al. 2021) and Cleveland Bay's background of under
// 5 mg/L (Larcombe et al. 1995, Coral Reefs 14), as Jones et al. (2021, Front.
// Mar. Sci. 8, 727206) cite them: there the median wind's waves lift
// all the fine sediment it holds, 100 g/m³ over 5 m, so N = ln(1.5 / 100) /
// ln(1 - M), with M = 0.8468 the share of time its waves move fine sediment.
const FINE_SEDIMENT_QUIET_SPELLS: f32 = 2.238;

// The share of a bed `bed_depth_m` below mean sea level, of `sand_cover`,
// that holds fine sediment: the share of time its waves leave fine sediment
// still, M, held for `FINE_SEDIMENT_QUIET_SPELLS` spells in a row, (1 - M)^N.
// Compromise: fine sediment settles only among the sand, for want of rivers
// and other sources of mud.
fn fine_sediment_availability(sand_cover: f32, bed_depth_m: f32) -> f32 {
    return sand_cover * pow(1.0 - seabed_mobility(bed_depth_m).y, FINE_SEDIMENT_QUIET_SPELLS);
}

// The rise in fine sediment held in suspension per unit of the excess of the
// squared orbital speed at the bed over the threshold's: Schoellhamer (1995,
// Estuar. Coast. Shelf Sci. 40) finds orbital speeds of 15-20 cm/s
// resuspending Old Tampa Bay's bed 1.5 and 4 m down, and Sheng et al. (1995)
// its water rising from about 10 to over 40 mg/L in storms, as Green and Coco
// (2014, Rev. Geophys. 52) cite them: 10-24 g/m³ over that excess of 1.25-3.
const FINE_SEDIMENT_RESUSPENSION_G_M3: f32 = 15.0;

// The fine sediment a bed of it alone can give up to waves stressing it by
// `stress_pa`, in g/m²: Sanford and Maa's (2001, Mar. Geol. 179) depth-limited
// erosion, which stops where the bed's critical stress, rising with the mass
// eroded, meets the waves'. Its profile is the York River's,
// m = 1.43 (τ - 0.01)^1.97 kg/m² (Dickhudt et al. 2009, Mar. Geol. 267, as Ha
// and Ha 2021, Front. Mar. Sci. 8, tabulate it), up to the 0.5 kg/m² it gives
// at 0.6 Pa, the top of the stresses measured. Compromise: that ceiling is
// where the measurements stop, not what a bed holds, so above 0.6 Pa, which a
// moderate wind's waves reach over beds to about 6 m deep, a storm raises the
// load no further, where real coasts rise to tens of mg/L in wind events.
// Extrapolated past 0.6 Pa, the fit holds storm water over sandy beds 10-20 m
// deep at about ten times the tens to low hundreds of mg/L measured (Sheng et
// al. 1995; Ferré et al. 2005, Cont. Shelf Res. 25). Revisit once beds of fine
// sediment alone exist.
fn erodible_fine_sediment_g_m2(stress_pa: f32) -> f32 {
    return 1000.0 * min(1.43 * pow(max(stress_pa - 0.01, 0.0), 1.97), 0.5);
}

// Fine sediment held in suspension over a bed `depth_m` deep, of which
// `availability` holds fine sediment, under waves moving the bed at
// `orbital_speed_m_s`: in proportion to the excess of the squared speed over
// the threshold's where erosion by an excess-stress law balances settling,
// up to what the bed gives up to the waves' stress, τ = ρ f_w u² / 2, mixed
// through the column. Compromises: the load follows the present waves
// without settling lag; and the column is mixed from bed to surface.
fn stirred_fine_sediment_g_m3(availability: f32, orbital_speed_m_s: f32, depth_m: f32) -> f32 {
    let squared_speed = orbital_speed_m_s * orbital_speed_m_s;
    let threshold = FINE_SEDIMENT_THRESHOLD_ORBITAL_SPEED_M_S;
    let excess = max(squared_speed / (threshold * threshold) - 1.0, 0.0);
    let stress_pa = 0.5 * SEAWATER_DENSITY_KG_M3 * WAVE_FRICTION_FACTOR * squared_speed;
    return availability * min(
        FINE_SEDIMENT_RESUSPENSION_G_M3 * excess,
        erodible_fine_sediment_g_m2(stress_pa) / max(depth_m, 0.01),
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
// direction. Below about 5.3 m/s the peak's floor gives every octave a full
// share, an 18 m octave included.
fn decorative_water_normal_sample(
    position_m: vec2<f32>,
    wind_speed_m_s: f32,
    peak_wavelength_m: f32,
    slope_budget: f32,
) -> DecorativeWaterSample {
    var sample = DecorativeWaterSample(vec2<f32>(0.0), 0.0, 0.0);
    let shown = material_feature_visibility().w > 0.0;
    var wavelength_m = DECORATIVE_WATER_WAVELENGTH_M;
    // The octaves reach half an octave above the first and below the last,
    // which is eight times shorter.
    let octave_range_m = vec2<f32>(1.4142135, 0.0883883) * wavelength_m;
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
    // The wave sample leaves its forcing in `sampled_wave_forcing`.
    let decorative = decorative_water_normal_sample(
        position_m,
        wind_speed_m_s,
        wave_peak_wavelength_m(sampled_wave_forcing.sea),
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
