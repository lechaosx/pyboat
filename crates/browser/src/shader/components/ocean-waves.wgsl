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

// The forcing of the waves the scene last sampled, which the sediment they
// stir at the same point reuses rather than evaluating the winds again.
var<private> sampled_wave_forcing: OceanForcingSample;

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
    vec2<f32>(1.00000, 1.00000), vec2<f32>(0.99585, 1.00000), vec2<f32>(0.99194, 1.00000),
    vec2<f32>(0.98828, 1.00000), vec2<f32>(0.98462, 1.00000), vec2<f32>(0.98120, 1.00000),
    vec2<f32>(0.97754, 1.00000), vec2<f32>(0.97363, 1.00000), vec2<f32>(0.96997, 1.00000),
    vec2<f32>(0.96606, 1.00000), vec2<f32>(0.96216, 0.99854), vec2<f32>(0.95801, 0.99414),
    vec2<f32>(0.95386, 0.99023), vec2<f32>(0.94946, 0.98657), vec2<f32>(0.94482, 0.98267),
    vec2<f32>(0.94018, 0.97876), vec2<f32>(0.93530, 0.97485), vec2<f32>(0.93017, 0.97070),
    vec2<f32>(0.92480, 0.96655), vec2<f32>(0.91919, 0.96216), vec2<f32>(0.91333, 0.95776),
    vec2<f32>(0.90698, 0.95288), vec2<f32>(0.90039, 0.94775), vec2<f32>(0.89331, 0.94238),
    vec2<f32>(0.88598, 0.93677), vec2<f32>(0.87793, 0.93042), vec2<f32>(0.86914, 0.92383),
    vec2<f32>(0.85986, 0.91675), vec2<f32>(0.84961, 0.90893), vec2<f32>(0.83838, 0.90039),
    vec2<f32>(0.82593, 0.89087), vec2<f32>(0.81201, 0.88037), vec2<f32>(0.79663, 0.86841),
    vec2<f32>(0.77905, 0.85498), vec2<f32>(0.75952, 0.83984), vec2<f32>(0.73681, 0.82226),
    vec2<f32>(0.71118, 0.80200), vec2<f32>(0.68139, 0.77832), vec2<f32>(0.64697, 0.75073),
    vec2<f32>(0.62817, 0.71826), vec2<f32>(0.60912, 0.67993), vec2<f32>(0.58959, 0.64208),
    vec2<f32>(0.56933, 0.62426), vec2<f32>(0.54858, 0.60571), vec2<f32>(0.52758, 0.58691),
    vec2<f32>(0.50585, 0.56738), vec2<f32>(0.48365, 0.54711), vec2<f32>(0.46119, 0.52660),
    vec2<f32>(0.43824, 0.50561), vec2<f32>(0.41505, 0.48414), vec2<f32>(0.39185, 0.46217),
    vec2<f32>(0.36841, 0.43995), vec2<f32>(0.34498, 0.41749), vec2<f32>(0.32178, 0.39478),
    vec2<f32>(0.29859, 0.37183), vec2<f32>(0.27564, 0.34913), vec2<f32>(0.25318, 0.32618),
    vec2<f32>(0.23120, 0.30347), vec2<f32>(0.20996, 0.28101), vec2<f32>(0.18946, 0.25904),
    vec2<f32>(0.16968, 0.23731), vec2<f32>(0.15088, 0.21631), vec2<f32>(0.13306, 0.19580),
    vec2<f32>(0.11646, 0.17603), vec2<f32>(0.10083, 0.15723), vec2<f32>(0.08667, 0.13941),
    vec2<f32>(0.07373, 0.12256), vec2<f32>(0.06201, 0.10694), vec2<f32>(0.05151, 0.09229),
    vec2<f32>(0.04224, 0.07910), vec2<f32>(0.03418, 0.06690), vec2<f32>(0.02734, 0.05591),
    vec2<f32>(0.02148, 0.04639), vec2<f32>(0.01660, 0.03784), vec2<f32>(0.01245, 0.03052),
    vec2<f32>(0.00928, 0.02417), vec2<f32>(0.00684, 0.01904), vec2<f32>(0.00488, 0.01465),
    vec2<f32>(0.00342, 0.01099), vec2<f32>(0.00244, 0.00806), vec2<f32>(0.00146, 0.00586),
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
// ln(1 - M), with M = 0.8297 the share of time its waves move fine sediment.
const FINE_SEDIMENT_QUIET_SPELLS: f32 = 2.372;

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
