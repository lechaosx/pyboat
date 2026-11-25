use std::cell::RefCell;
use std::rc::Rc;

use physics::glamx::{DVec2, Vec2 as Vector};
use simulation::{HullParameters, WorldGeometry, hull_outline_m};

use crate::{OceanQuery, OceanState, TICK_SECONDS, hash_unit};

const GRAVITY_M_S2: f64 = 9.81;

// The wave table: bands of wavenumber, `OCEAN_WAVE_BANDS_PER_OCTAVE` to an
// octave from the longest wavelength, `OCEAN_WAVE_LONGEST_M`, down to 0.5 m,
// each with `OCEAN_WAVE_DIRECTIONS` directions spaced evenly from a seeded
// rotation. Compromise: where the gameplay window passes the table's ends,
// below about 1.25 m/s and above about 40 m/s, the modes lose that part of
// the window's variance while Hs stays the whole spectrum's; both edges are
// continuous, and at 1.25 m/s Hs is about 4 cm. Keep these and
// `GAMEPLAY_WINDOW` synchronized with the WAVE_ constants in
// scene-state.wgsl.
pub const OCEAN_WAVE_BANDS_PER_OCTAVE: usize = 2;
pub const OCEAN_WAVE_LONGEST_M: f64 = 4096.0;
pub const OCEAN_WAVE_BAND_COUNT: usize = 13 * OCEAN_WAVE_BANDS_PER_OCTAVE;
pub const OCEAN_WAVE_DIRECTIONS: usize = 16;
// A point visits the bands that meet its gameplay window, at most
// `VISITED_BANDS`, and in each the `VISITED_DIRECTIONS` table directions
// nearest its wind.
const VISITED_BANDS: usize = 8;
const VISITED_DIRECTIONS: usize = 6;
// The wind sea's wave modes span 0.36 to 3 times its peak wavenumber. Below,
// Pierson and Moskowitz's spectrum falls under 0.3% of its peak and holds
// 0.007% of its variance. Compromise: above, the 13% of the variance
// beyond three times the peak is left to the decorative octaves' slopes and
// has no height, load or velocity for boats: they cannot surf waves slower
// than about 0.69 of the wind, the only ones a dinghy is fast enough to
// catch, and the window keeps about 35% of the sea's surface Stokes drift, about 0.6%
// of the wind's speed instead of 1.7%.
const GAMEPLAY_WINDOW: [f64; 2] = [0.36, 3.0];

/// A wave mode's spatial phase is `wave_number_x_per_m * x + wave_number_y_per_m * y`
/// with `x` in `[0, width)`; `phase_rad` adds the mode's phase at the queried clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OceanWaveMode {
    pub wave_number_x_per_m: f64,
    pub wave_number_y_per_m: f64,
    pub wave_number_per_m: f64,
    pub angular_frequency_rad_s: f64,
    pub phase_rad: f64,
}

/// The wave modes at a clock: each band's direction rotation and its modes in
/// direction order, and the background swell's clock.
#[derive(Clone, Debug, PartialEq)]
pub struct OceanWaveTable {
    pub band_rotation_rad: [f64; OCEAN_WAVE_BAND_COUNT],
    pub modes: [[OceanWaveMode; OCEAN_WAVE_DIRECTIONS]; OCEAN_WAVE_BAND_COUNT],
    pub swell_clock: SwellClock,
}

/// The fully developed sea of a surface wind: Pierson and Moskowitz's (1964)
/// spectrum, α g² ω⁻⁵ exp(-5/4 (ω_p/ω)⁴) with α = 0.0081 and ω_p = 0.877 g /
/// U₁₉.₅, of variance α g² / (5 ω_p⁴), so Hs = 0.21 U₁₉.₅² / g; and
/// Mitsuyasu et al.'s (1975) spreading parameter at its peak,
/// 11.5 (U₁₀ ω_p / g)^-2.5. Compromise: the sea is fully developed wherever
/// the wind blows, as if it had blown long and far, so the strongest winds
/// raise seas above the highest measured until fetch and duration limit
/// them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindSea {
    pub peak_wavenumber_per_m: f64,
    pub variance_m2: f64,
    pub peak_spread: f64,
}

impl WindSea {
    pub fn significant_height_m(&self) -> f64 {
        4.0 * libm::sqrt(self.variance_m2)
    }
}

/// The wind sea of a sustained surface wind taken 10 m up, which gives the
/// wind at 19.5 m by a neutral log profile with Charnock's (1955) roughness,
/// z₀ = 0.011 u*²/g.
pub fn wind_sea(surface_wind_speed_m_s: f64) -> WindSea {
    if surface_wind_speed_m_s <= 0.0 {
        return WindSea {
            peak_wavenumber_per_m: f64::INFINITY,
            variance_m2: 0.0,
            peak_spread: 0.0,
        };
    }
    // With u* = κ U₁₀ / L for L = ln(10 m / z₀) and κ = 0.41, L solves
    // L = c + 2 ln L, which a fixed-point iteration settles in four steps
    // from L = 10, as the derivative 2 / L is about 0.2.
    let constant = libm::log(
        10.0 * GRAVITY_M_S2
            / (0.011 * 0.41 * 0.41 * surface_wind_speed_m_s * surface_wind_speed_m_s),
    );
    let mut log_height = 10.0;
    for _ in 0..4 {
        log_height = constant + 2.0 * libm::log(log_height);
    }
    let speed_m_s = surface_wind_speed_m_s * (1.0 + libm::log(1.95) / log_height);
    let peak_frequency = 0.877 * GRAVITY_M_S2 / speed_m_s;
    WindSea {
        peak_wavenumber_per_m: peak_frequency * peak_frequency / GRAVITY_M_S2,
        variance_m2: 0.0081 * GRAVITY_M_S2 * GRAVITY_M_S2 / (5.0 * libm::pow(peak_frequency, 4.0)),
        peak_spread: 11.5 * libm::pow(surface_wind_speed_m_s * peak_frequency / GRAVITY_M_S2, -2.5),
    }
}

/// A table mode and its amplitude in the wind sea at a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveComponent {
    pub band: usize,
    pub mode: OceanWaveMode,
    pub amplitude_m: f64,
}

/// The wind sea's components under a surface wind, at the clock's start.
pub fn wind_sea_components(
    world_geometry: WorldGeometry,
    match_seed: u64,
    surface_wind_velocity_m_s: Vector,
) -> Vec<WaveComponent> {
    let table = ocean_wave_table(world_geometry, match_seed, 0, 0.0);
    let mut components = Vec::new();
    for_each_component(
        &table.band_rotation_rad,
        &SeaPartition::wind_sea(surface_wind_velocity_m_s),
        |band, direction, amplitude_m, _| {
            components.push(WaveComponent {
                band,
                mode: table.modes[band][direction],
                amplitude_m,
            });
        },
    );
    components
}

/// How a sea's variance spreads over wavenumber.
#[derive(Clone, Copy)]
enum SpectralShape {
    /// Pierson and Moskowitz's fully developed sea.
    PiersonMoskowitz,
    /// The swell part of Ochi and Hubble's (1976, Proc. 15th ICCE, ch. 18)
    /// six-parameter spectrum at its most probable shape, λ = 3 (their
    /// Table 2): S(ω) ∝ ω^-(4λ+1) exp(-(4λ+1)/4 (ω_m/ω)⁴).
    OchiHubbleSwell,
}

impl SpectralShape {
    /// The share of the variance at deep-water wavenumbers below `wavenumber`
    /// of a sea peaking at `peak_wavenumber`, where (ω_p/ω)⁴ = (k_p/k)².
    fn share_below(self, wavenumber: f64, peak_wavenumber: f64) -> f64 {
        let ratio = (peak_wavenumber / wavenumber).powi(2);
        match self {
            Self::PiersonMoskowitz => libm::exp(-1.25 * ratio),
            // The regularized upper incomplete gamma function Q(3, x).
            Self::OchiHubbleSwell => {
                let x = 3.25 * ratio;
                libm::exp(-x) * (1.0 + x + 0.5 * x * x)
            }
        }
    }
}

/// A sea as the wave modes sample it: its spectrum, the multiples of its peak
/// wavenumber its modes span, and how many bands and table directions about
/// its direction a point visits.
#[derive(Clone, Copy)]
struct SeaPartition {
    shape: SpectralShape,
    variance_m2: f64,
    peak_wavenumber_per_m: f64,
    peak_spread: f64,
    direction_rad: f64,
    window: [f64; 2],
    visited_bands: usize,
    visited_directions: usize,
}

impl SeaPartition {
    fn wind_sea(surface_wind_velocity_m_s: Vector) -> Self {
        let sea = wind_sea(f64::from(surface_wind_velocity_m_s.length()));
        Self {
            shape: SpectralShape::PiersonMoskowitz,
            variance_m2: sea.variance_m2,
            peak_wavenumber_per_m: sea.peak_wavenumber_per_m,
            peak_spread: sea.peak_spread,
            direction_rad: libm::atan2(
                f64::from(surface_wind_velocity_m_s.y),
                f64::from(surface_wind_velocity_m_s.x),
            ),
            window: GAMEPLAY_WINDOW,
            visited_bands: VISITED_BANDS,
            visited_directions: VISITED_DIRECTIONS,
        }
    }

    fn swell(swell: Swell) -> Self {
        Self {
            shape: SpectralShape::OchiHubbleSwell,
            variance_m2: swell.significant_height_m * swell.significant_height_m / 16.0,
            peak_wavenumber_per_m: swell.peak_wavenumber_per_m,
            peak_spread: SWELL_PEAK_SPREAD,
            direction_rad: swell.direction_rad,
            window: SWELL_WINDOW,
            visited_bands: SWELL_VISITED_BANDS,
            visited_directions: SWELL_VISITED_DIRECTIONS,
        }
    }
}

/// Calls `visit` with the band, table direction and amplitude of each mode of
/// `sea` a point visits, and the band's centre wavenumber within the window.
/// Each band holds the spectrum's variance over its wavenumbers inside the
/// sea's window, so bands enter and leave the window empty as the peak moves.
/// Within a band, amplitudes follow Mitsuyasu's cos^2s(θ/2) at the band's
/// frequency, s = s_p (f/f_p)^5 below the peak and s_p (f/f_p)^-2.5 above,
/// normalized so the band keeps its variance.
/// Compromises: bands are half an octave wide throughout, as one table serves
/// every peak; and a band visits only its `visited_directions` directions
/// nearest the sea's, under a Tukey window flat over their inner half that
/// takes each to zero before the visited set changes, which narrows the broad
/// spreads away from the peak. Returns the window's variance and its sum of
/// each band's centre wavenumber times the band's variance.
fn for_each_component(
    band_rotation_rad: &[f64; OCEAN_WAVE_BAND_COUNT],
    sea: &SeaPartition,
    mut visit: impl FnMut(usize, usize, f64, f64),
) -> (f64, f64) {
    let (mut window_variance, mut weighted_wavenumber) = (0.0, 0.0);
    if sea.variance_m2 <= 0.0 {
        return (window_variance, weighted_wavenumber);
    }
    let peak = sea.peak_wavenumber_per_m;
    let lowest_wavenumber = std::f64::consts::TAU / OCEAN_WAVE_LONGEST_M;
    let spacing = std::f64::consts::TAU / OCEAN_WAVE_DIRECTIONS as f64;
    let window_width = 0.5 * sea.visited_directions as f64 * spacing;
    let window = sea.window.map(|ratio| ratio * peak);
    let first_band =
        libm::floor(OCEAN_WAVE_BANDS_PER_OCTAVE as f64 * libm::log2(window[0] / lowest_wavenumber))
            as i64;
    for band in first_band..first_band + sea.visited_bands as i64 {
        let Ok(band) = usize::try_from(band) else {
            continue;
        };
        if band >= OCEAN_WAVE_BAND_COUNT {
            break;
        }
        let edges = [band, band + 1].map(|edge| {
            lowest_wavenumber * libm::exp2(edge as f64 / OCEAN_WAVE_BANDS_PER_OCTAVE as f64)
        });
        let lower = edges[0].max(window[0]);
        let upper = edges[1].min(window[1]);
        if upper <= lower {
            continue;
        }
        let variance = sea.variance_m2
            * (sea.shape.share_below(upper, peak) - sea.shape.share_below(lower, peak));
        let centre = libm::sqrt(lower * upper);
        weighted_wavenumber += variance * centre;
        window_variance += variance;
        let frequency_ratio = libm::sqrt(centre / peak);
        let spread = sea.peak_spread
            * libm::pow(
                frequency_ratio,
                if frequency_ratio < 1.0 { 5.0 } else { -2.5 },
            );
        // The visited directions straddle the sea's: an even count the two
        // table directions either side of it, an odd one the nearest.
        let position = (sea.direction_rad - band_rotation_rad[band]) / spacing;
        let nearest = libm::floor(position + 0.5 * (sea.visited_directions % 2) as f64);
        let offset = position - nearest;
        let first_step = -((sea.visited_directions as i64 - 1) / 2);
        let mut roots = [0.0; VISITED_DIRECTIONS];
        for (direction, root) in roots.iter_mut().enumerate().take(sea.visited_directions) {
            let angle = ((direction as i64 + first_step) as f64 - offset) * spacing;
            *root = libm::pow(libm::cos(0.5 * angle), spread)
                * libm::cos(
                    std::f64::consts::PI * (libm::fabs(angle) / window_width - 0.5).max(0.0),
                );
        }
        let weight: f64 = roots.iter().map(|root| root * root).sum();
        let scale = libm::sqrt(2.0 * variance / weight);
        for (direction, root) in roots.into_iter().enumerate().take(sea.visited_directions) {
            let table_direction = (nearest as i64 + direction as i64 + first_step)
                .rem_euclid(OCEAN_WAVE_DIRECTIONS as i64)
                as usize;
            visit(band, table_direction, scale * root, centre);
        }
    }
    (window_variance, weighted_wavenumber)
}

/// The slope variance along and across a surface wind of its wind sea's
/// modes, in deep water and unfiltered by any footprint.
pub fn wind_sea_slope_variance(
    world_geometry: WorldGeometry,
    match_seed: u64,
    surface_wind_velocity_m_s: Vector,
) -> [f64; 2] {
    let table = ocean_wave_table_at_start(world_geometry, match_seed);
    let direction = surface_wind_velocity_m_s.normalize_or_zero();
    let [x, y] = [f64::from(direction.x), f64::from(direction.y)];
    let mut slope_variance = [0.0; 2];
    for_each_component(
        &table.band_rotation_rad,
        &SeaPartition::wind_sea(surface_wind_velocity_m_s),
        |band, table_direction, amplitude_m, _| {
            let mode = table.modes[band][table_direction];
            let variance = 0.5 * amplitude_m * amplitude_m;
            slope_variance[0] +=
                variance * (mode.wave_number_x_per_m * x + mode.wave_number_y_per_m * y).powi(2);
            slope_variance[1] +=
                variance * (mode.wave_number_y_per_m * x - mode.wave_number_x_per_m * y).powi(2);
        },
    );
    slope_variance
}

/// What raises the sea at a point: the sustained surface wind, the swell, and
/// the water's depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OceanWaveForcing {
    pub surface_wind_velocity_m_s: Vector,
    pub swell: Swell,
    pub water_depth_m: f32,
}

/// A swell: its significant height, its peak wavenumber, and the direction it
/// travels toward, anticlockwise from east.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swell {
    pub significant_height_m: f64,
    pub peak_wavenumber_per_m: f64,
    pub direction_rad: f64,
}

impl Swell {
    pub const NONE: Self = Self {
        significant_height_m: 0.0,
        peak_wavenumber_per_m: 1.0,
        direction_rad: 0.0,
    };
}

// The swell that storms beyond the world's scale send a sea, by absolute
// latitude at 0°, 10°, … 90°: the planet is symmetric, so its hemispheres
// share one climate, the southern half a year from the northern. The knots
// pool both hemispheres of NOAA NCEP's operational global WAVEWATCH III
// (swell partition: height `shgt`, peak period `sper`, direction `sdir`;
// PacIOOS ERDDAP dataset `ww3_global`), sampled every 121 h over 2018-2024,
// within 3° of each knot, with each Southern Hemisphere season shifted half
// a year. Gulev et al.'s (2003, J. Geophys. Res. 108, 3236) ship-observed
// swell heights, 1.7-2.7 m by latitude band, agree in pattern and run higher
// in the tropics; their visually observed periods run low.
// The root mean square of the swell's significant height.
const SWELL_HEIGHT_M: [f32; 10] = [1.16, 1.19, 1.47, 1.93, 2.32, 2.41, 2.20, 1.41, 1.41, 1.41];
// The relative amplitude of its square's annual cycle, rougher in winter.
const SWELL_HEIGHT_SEASONAL_AMPLITUDE: [f32; 10] =
    [0.0, 0.22, 0.43, 0.55, 0.45, 0.42, 0.34, 0.34, 0.34, 0.34];
// Its mean peak period and that period's annual amplitude, longer in winter.
const SWELL_PERIOD_S: [f32; 10] = [
    10.75, 11.23, 11.44, 11.34, 11.32, 11.25, 10.77, 9.03, 9.03, 9.03,
];
const SWELL_PERIOD_SEASONAL_AMPLITUDE_S: [f32; 10] =
    [0.0, 0.78, 0.69, 0.84, 0.85, 0.76, 0.79, 0.82, 0.82, 0.82];
// The direction it travels toward, in degrees from east toward the equator:
// the direction of its energy-weighted mean propagation, westward along the
// equator, equatorward in the subtropics, eastward in the storm belts near
// 50° and poleward beyond. Compromises: the annual cycles' amplitudes are 0
// and the direction westward at the equator, where the measured 0.07, 0.08 s
// and 4° south of west would step between the hemispheres; the direction
// keeps no annual cycle, where between 5° and 25° the same series' northward
// propagation swings over the year by an annual cosine of 0.38-0.53, more
// than its mean resultant of 0.20-0.33, as the far hemisphere's winter
// swell crosses into summer; and beyond 70°, where no open sea was measured,
// the knots hold 70°'s values and the direction turns to east at the pole,
// through which the world mirrors.
const SWELL_DIRECTION_DEGREES: [f32; 10] =
    [180.0, 99.0, 84.0, 63.0, 31.0, 4.0, -31.0, -63.0, -31.5, 0.0];
// The swell wanders at a point, once the annual cycle is removed, in its
// steepness Hs / Lp and its peak period, of each value's ratio to its
// point's calendar month's mean in the same series; its height is the
// steepness times its wavelength, so long swell is high swell and short
// swell low, as measured.
// The spread of the log-normal of the steepness's ratio: the σ that gives
// the swell within 3° of the knot, over the seasons and the wander, the
// measured 99.9th percentile of absolute steepness there, 0.022-0.027, the
// steep end where swell tilts boats. Compromises: a log-normal cannot hold
// the measured shape, so the swell's 99th percentile falls 5-14% short of
// the measured at every knot; and its mean steepness stands up to 16% over
// the measured at 40-70°, as the knots keep the measured mean square height
// and mean period rather than mean steepness.
const SWELL_STEEPNESS_LOG_SPREAD: [f32; 10] =
    [0.43, 0.47, 0.41, 0.36, 0.32, 0.30, 0.30, 0.33, 0.33, 0.33];
// The correlation of the logarithms of the steepness's and the period's
// ratios: steeper swell is shorter.
const SWELL_STEEPNESS_PERIOD_CORRELATION: [f32; 10] = [
    -0.75, -0.68, -0.61, -0.47, -0.35, -0.35, -0.36, -0.41, -0.41, -0.41,
];
// The spread of the period's ratio: from its coefficient of variation CV,
// σ = √ln(1 + CV²), the log-normal of that CV.
const SWELL_PERIOD_LOG_SPREAD: [f32; 10] =
    [0.23, 0.22, 0.21, 0.19, 0.19, 0.20, 0.21, 0.24, 0.24, 0.24];
// The circular standard deviation of its direction at a point, √(-2 ln R),
// from each point's resultant length R within a calendar month, in degrees.
const SWELL_DIRECTION_SPREAD_DEGREES: [f32; 10] =
    [48.0, 49.0, 49.0, 52.0, 57.0, 67.0, 64.0, 54.0, 54.0, 54.0];
// The season phase at which a hemisphere's swell peaks: the fitted annual
// cycles peak within a few weeks of mid-January in the north, the season
// phase counting from the December solstice.
const SWELL_WINTER_SEASON_PHASE: f64 = 0.05;
// The swell's steepness, period and direction each wander with one slow signal,
// a sum of `SWELL_SIGNAL_TERMS` cosines of seeded phases and of frequencies
// drawn from a Gaussian of standard deviation √2 / `SWELL_COHERENCE_S`,
// whose autocorrelation is exp(-(τ / `SWELL_COHERENCE_S`)²) in expectation.
// At a point the measured autocorrelation of the swell's log height falls to
// 1/e in 21 h and its period's and direction's in 18 h (3-hourly series at 75
// open-ocean points over June and July 2019); `SWELL_COHERENCE_S` is their
// 20 h. The steepness takes the measured correlation with the period from
// the period's signal and the rest from its own. Compromises: the signals
// are the same everywhere, so swell events reach every longitude at once;
// the direction wanders independently; and the measured autocorrelation
// falls faster in the first 6 h, to 0.70, which the smooth signal leaves
// out.
const SWELL_COHERENCE_S: f64 = 20.0 * 3_600.0;
const SWELL_SIGNAL_TERMS: usize = 8;
// Goda's maximum spreading parameter for swell that has travelled far, in
// Mitsuyasu's cos^2s(θ/2) about its direction (Goda, Takayama and Suzuki
// 1978, Proc. 16th ICCE, ch. 35, after Goda and Suzuki 1975): 9° rms at the
// peak.
const SWELL_PEAK_SPREAD: f64 = 75.0;
// The swell's wave modes span 0.7 to 2.1 times its peak wavenumber and hold
// 92% of its variance. That window is 3.17 half-octave bands wide, so it
// touches at most `SWELL_VISITED_BANDS` bands, each visiting
// `SWELL_VISITED_DIRECTIONS` directions. Compromise for cost: the 8% outside
// has no height, where 0.62-2.73 would hold 98% in up to six bands.
const SWELL_WINDOW: [f64; 2] = [0.7, 2.1];
const SWELL_VISITED_BANDS: usize = 5;
const SWELL_VISITED_DIRECTIONS: usize = 3;
// `for_each_component` holds a band's direction weights in an array of
// `VISITED_DIRECTIONS`.
const _: () = assert!(SWELL_VISITED_DIRECTIONS <= VISITED_DIRECTIONS);

/// The swell climate's knots for the renderer, three rows to a knot at 0°,
/// 10°, … 90°: height, its annual amplitude, period and its annual
/// amplitude; then direction, the spreads of log steepness and log period,
/// and the direction's spread in degrees; then the correlation of log
/// steepness with log period.
pub fn swell_climate_knots() -> [[f32; 4]; 30] {
    std::array::from_fn(|row| {
        let knot = row / 3;
        if row % 3 == 2 {
            [SWELL_STEEPNESS_PERIOD_CORRELATION[knot], 0.0, 0.0, 0.0]
        } else if row % 3 == 0 {
            [
                SWELL_HEIGHT_M[knot],
                SWELL_HEIGHT_SEASONAL_AMPLITUDE[knot],
                SWELL_PERIOD_S[knot],
                SWELL_PERIOD_SEASONAL_AMPLITUDE_S[knot],
            ]
        } else {
            [
                SWELL_DIRECTION_DEGREES[knot],
                SWELL_STEEPNESS_LOG_SPREAD[knot],
                SWELL_PERIOD_LOG_SPREAD[knot],
                SWELL_DIRECTION_SPREAD_DEGREES[knot],
            ]
        }
    })
}

/// The clock's part of the background swell: the annual cycle at the
/// northern hemisphere's latitudes, cos 2π(season - winter), and the slow
/// signals its steepness, period and direction wander with, each of unit
/// variance; the steepness's is the part apart from the period's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwellClock {
    pub season: f64,
    pub steepness: f64,
    pub period: f64,
    pub direction: f64,
}

fn swell_clock(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
) -> SwellClock {
    let world_time_s = (world_tick as f64 + f64::from(world_tick_fraction)) * TICK_SECONDS;
    let season_phase = f64::from(
        crate::world_celestial_conditions(world_geometry, world_time_s, Vector::ZERO).season_phase,
    );
    let gaussian_scale = std::f64::consts::SQRT_2 / SWELL_COHERENCE_S;
    let signal = |index: u64| {
        let mut sum = 0.0;
        for term in 0..SWELL_SIGNAL_TERMS as u64 {
            let key = match_seed
                ^ 0x2545_f491_4f6c_dd1d
                ^ (index * SWELL_SIGNAL_TERMS as u64 + term).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            // |N(0, 1)| by Box and Muller.
            let gaussian = libm::sqrt(-2.0 * libm::log(hash_unit(key ^ 0x94d0_49bb_1331_11eb)))
                * libm::fabs(libm::cos(
                    std::f64::consts::TAU * hash_unit(key ^ 0xd6e8_feb8_6659_fd93),
                ));
            let phase_per_tick = gaussian * gaussian_scale * TICK_SECONDS;
            sum += libm::cos(
                std::f64::consts::TAU * hash_unit(key ^ 0x8ebc_6af0_9c88_c6e3)
                    + tick_phase_radians(world_tick, phase_per_tick)
                    + f64::from(world_tick_fraction) * phase_per_tick,
            );
        }
        libm::sqrt(2.0 / SWELL_SIGNAL_TERMS as f64) * sum
    };
    SwellClock {
        season: libm::cos(std::f64::consts::TAU * (season_phase - SWELL_WINTER_SEASON_PHASE)),
        steepness: signal(0),
        period: signal(1),
        direction: signal(2),
    }
}

/// The background swell at a point and clock: the swell that storms beyond
/// the world's scale send a sea at its latitude in its season, wandering with
/// slow signals in a log-normal steepness and period that keep the climate's
/// mean square height and mean period. Compromise: every water point takes the open sea's
/// swell, island lees, bays and lagoons too, until an exposure law shelters
/// them.
pub fn background_swell(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    position_m: Vector,
) -> Swell {
    let clock =
        ocean_wave_table(world_geometry, match_seed, world_tick, world_tick_fraction).swell_clock;
    let y_m = f64::from(position_m.y);
    let latitude = world_geometry.latitude_degrees(y_m) as f32;
    let knot = |knots| f64::from(crate::latitude_curve_value(knots, latitude));
    let hemisphere = if latitude >= 0.0 { 1.0 } else { -1.0 };
    let season = hemisphere * clock.season;
    let steepness_spread = knot(SWELL_STEEPNESS_LOG_SPREAD);
    let period_spread = knot(SWELL_PERIOD_LOG_SPREAD);
    let correlation = knot(SWELL_STEEPNESS_PERIOD_CORRELATION);
    // ln Hs = ln steepness + 2 ln Tp + constant is normal, of the period's
    // signal and the steepness's own, with `variance`; subtracting it keeps
    // the climate's mean of Hs², E[exp(2X)] = exp(2 Var X) for a centred
    // normal X. Compromise: that holds for Gaussian signals; the signals'
    // eight cosines leave the mean of Hs² about 0.1-0.3% low.
    let period_coefficient = 2.0 * period_spread + correlation * steepness_spread;
    let steepness_coefficient = steepness_spread * libm::sqrt(1.0 - correlation * correlation);
    let variance =
        period_coefficient * period_coefficient + steepness_coefficient * steepness_coefficient;
    let significant_height_m = knot(SWELL_HEIGHT_M)
        * libm::sqrt(1.0 + knot(SWELL_HEIGHT_SEASONAL_AMPLITUDE) * season)
        * libm::exp(
            period_coefficient * clock.period + steepness_coefficient * clock.steepness - variance,
        );
    let period_s = (knot(SWELL_PERIOD_S) + knot(SWELL_PERIOD_SEASONAL_AMPLITUDE_S) * season)
        * libm::exp(period_spread * clock.period - 0.5 * period_spread * period_spread);
    // Toward the equator is south in the north, and flips beyond a pole,
    // where latitude falls northward.
    let equatorward = -hemisphere * world_geometry.latitude_gradient_degrees_per_m(y_m).signum();
    let local = knot(SWELL_DIRECTION_DEGREES).to_radians();
    let peak_frequency = std::f64::consts::TAU / period_s;
    Swell {
        significant_height_m,
        peak_wavenumber_per_m: peak_frequency * peak_frequency / GRAVITY_M_S2,
        direction_rad: libm::atan2(equatorward * libm::sin(local), libm::cos(local))
            + knot(SWELL_DIRECTION_SPREAD_DEGREES).to_radians() * clock.direction,
    }
}

pub(crate) fn procedural_wave_state(
    query: OceanQuery,
) -> (OceanState, Option<SurfaceFit>, SeaComponents) {
    ocean_wave_state_and_hull_fit(
        query.world_geometry,
        query.match_seed,
        query.world_tick,
        query.world_tick_fraction,
        query.position_m,
        query.wave_forcing,
        query.waterplane,
    )
}

/// The sea's surface at a point and clock under `forcing`: the wind sea's and
/// the swell's modes, each component taking the larger of the two spectra so
/// that neither counts twice where they meet, with Tayfun's (1980)
/// narrow-band second order, η₂ = (k_m/2)(η² - η̂²) at their mean
/// wavenumber, where η̂ is the sum's quadrature. Compromises: Tayfun's term
/// treats a sea of wind sea and swell as one narrow band, at the mean
/// wavenumber of both windows' variance; and McCowan's (1894) breaker limit,
/// 0.78 of the depth, caps the sea, a hard cap that a smooth saturation is to
/// replace with the shore's waves.
pub fn ocean_wave_state(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    position_m: Vector,
    forcing: OceanWaveForcing,
) -> OceanState {
    ocean_wave_state_and_hull_fit(
        world_geometry,
        match_seed,
        world_tick,
        world_tick_fraction,
        position_m,
        forcing,
        None,
    )
    .0
}

/// `ocean_wave_state`, and with a waterplane the whole sea's unfiltered
/// `hull_wave_fit` over it, from one pass over the modes; and the sea's
/// components.
pub(crate) fn ocean_wave_state_and_hull_fit(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    position_m: Vector,
    forcing: OceanWaveForcing,
    waterplane: Option<Waterplane>,
) -> (OceanState, Option<SurfaceFit>, SeaComponents) {
    let hull = waterplane.map(|waterplane| HullTransfer::new(waterplane, 0.0));
    let components = sea_components(world_geometry, match_seed, forcing);
    let sums = sea_sums(
        world_geometry,
        match_seed,
        world_tick,
        world_tick_fraction,
        position_m,
        &components,
        |band, mode| {
            [
                Some(Transfer {
                    height: 1.0,
                    slope_wave_number: DVec2::new(
                        mode.wave_number_x_per_m,
                        mode.wave_number_y_per_m,
                    ),
                    yaw_m2: 0.0,
                }),
                hull.as_ref().and_then(|hull| hull.transfer(band, mode)),
            ]
        },
    );
    let sea = whole_sea(&sums[0]);
    let unrestricted_height_m = components.sea_height_m();
    let depth_scale = depth_scale(unrestricted_height_m, forcing.water_depth_m);
    let significant_height_m = depth_scale * unrestricted_height_m;
    let fit = second_order(&sea, depth_scale, components.sea_window);
    let orbital_velocity_m_s = depth_scale * sea.orbital_velocity_m_s;

    let surface_displacement_m = fit.height_m as f32;
    let significant_wave_height_m = significant_height_m as f32;
    (
        OceanState {
            current_velocity_m_s: Vector::ZERO,
            wave_orbital_velocity_m_s: orbital_velocity_m_s.as_vec2(),
            significant_wave_height_m,
            surface_displacement_m,
            surface_slope: fit.slope.as_vec2(),
            rogue_crest_excess_m: (surface_displacement_m - significant_wave_height_m).max(0.0),
        },
        hull.map(|_| second_order(&whole_sea(&sums[1]), depth_scale, components.sea_window)),
        components,
    )
}

/// A hull's waterplane as the sea's surface is fitted over it: a rectangle
/// along the hull's heading, centred on the hull's reference point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Waterplane {
    pub heading_rad: f64,
    pub length_m: f64,
    pub beam_m: f64,
}

impl Waterplane {
    /// The rectangle with the second moments per area, along and across the
    /// hull about its reference point, of the hull's outline, so a hull
    /// follows long waves' height and slope as its own waterplane would to
    /// second order in wavenumber. Compromises: the deck outline stands in
    /// for the flared hull's narrower waterplane; a rectangle stands in for
    /// its pointed shape where waves are short enough to tell them apart; and
    /// the rectangle is centred on the reference point, which for the dinghy
    /// lies 0.22 m ahead of its outline's centroid.
    pub fn of_hull(hull: HullParameters, heading_rad: f64) -> Self {
        let outline = hull_outline_m(hull);
        let (mut twice_area, mut along, mut across) = (0.0, 0.0, 0.0);
        for (index, first) in outline.iter().enumerate() {
            let second = outline[(index + 1) % outline.len()];
            let cross = first.perp_dot(second);
            twice_area += cross;
            along += cross * (first.x * first.x + first.x * second.x + second.x * second.x);
            across += cross * (first.y * first.y + first.y * second.y + second.y * second.y);
        }
        // A rectangle's ∫x² dA / A is L²/12, and a polygon's is
        // Σ cross (x₁² + x₁x₂ + x₂²) / (6 Σ cross).
        Self {
            heading_rad,
            length_m: libm::sqrt(2.0 * along / twice_area),
            beam_m: libm::sqrt(2.0 * across / twice_area),
        }
    }
}

/// The surface fitted over a waterplane: its height at the waterplane's
/// centre and its slope, both from Tayfun's second order of the fitted first
/// order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceFit {
    pub height_m: f64,
    pub slope: DVec2,
    pub orbital_velocity_m_s: DVec2,
    pub drift_velocity_m_s: DVec2,
    pub water_acceleration_m_s2: DVec2,
    pub yaw_moment_m2_s2: f64,
}

/// The wind sea alone, as the drawn surface shows it, and the whole sea
/// fitted over a waterplane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HullSurface {
    pub wind_sea: SurfaceFit,
    pub sea: SurfaceFit,
}

/// The sea's surface under `forcing` fitted over `waterplane` at a clock, as
/// a hull that follows it without motion of its own floats: the least-squares
/// plane over the waterplane, which for a wall-sided hull in the undisturbed
/// wave is where its waterplane's restoring force and moments balance the
/// wave's pressure, the Froude-Krylov heave force and moments (Journée and
/// Massie 2001, Offshore Hydromechanics, ch. 6). A mode of amplitude a and
/// phase φ at the centre, with u = k∥L/2 and v = k⊥B/2 along and across the
/// heading, raises it a S(u) S(v) cos φ, with S(u) = sin u / u, and tilts it
/// by -a sin φ (k∥ P(u) S(v), k⊥ S(u) P(v)) along and across, with
/// P(u) = 3 (sin u - u cos u) / u³. Each mode is also filtered by a
/// `footprint_m` pixel's footprint as the drawn surface filters it,
/// exp(-(k fp)² / 2), skipping its band once the band's longest wave falls
/// under about two pixels, k fp ≥ 3. Compromises: the hull follows the sea
/// quasi-statically, leaving out its heave resonance, estimated at 0.7-0.8 s
/// from its waterplane and an added mass like its own, which raises its
/// response to waves twice its length by about 10-15%; it leaves out
/// the pressure's decay to its draft (the Smith effect), which takes about
/// 14% from those waves; and Tayfun's term is that of the fitted first
/// order, not the second order fitted over the hull.
#[allow(clippy::too_many_arguments)]
pub fn hull_wave_fit(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    position_m: Vector,
    forcing: OceanWaveForcing,
    waterplane: Waterplane,
    footprint_m: f64,
) -> HullSurface {
    hull_wave_fit_of_sea(
        world_geometry,
        match_seed,
        world_tick,
        world_tick_fraction,
        position_m,
        &sea_components(world_geometry, match_seed, forcing),
        waterplane,
        footprint_m,
    )
}

/// `hull_wave_fit` of the sea whose components `sea` holds.
#[allow(clippy::too_many_arguments)]
pub fn hull_wave_fit_of_sea(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    position_m: Vector,
    sea: &SeaComponents,
    waterplane: Waterplane,
    footprint_m: f64,
) -> HullSurface {
    let hull = HullTransfer::new(waterplane, footprint_m);
    let [sums, _] = sea_sums(
        world_geometry,
        match_seed,
        world_tick,
        world_tick_fraction,
        position_m,
        sea,
        |band, mode| [hull.transfer(band, mode), None],
    );
    HullSurface {
        wind_sea: second_order(
            &sums.wind,
            depth_scale(sea.wind_sea_height_m(), sea.water_depth_m),
            sea.wind_window,
        ),
        sea: second_order(
            &whole_sea(&sums),
            depth_scale(sea.sea_height_m(), sea.water_depth_m),
            sea.sea_window,
        ),
    }
}

/// The horizontal displacement from its mean place of the surface water at a
/// point and clock in the sea whose components `sea` holds: each mode's
/// -a sin φ k̂, whose rate of change is the orbital velocity that carries
/// boats, limited by depth as the sea's height is.
pub fn sea_orbital_displacement_m(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    position_m: Vector,
    sea: &SeaComponents,
) -> DVec2 {
    let [sums, _] = sea_sums(
        world_geometry,
        match_seed,
        world_tick,
        world_tick_fraction,
        position_m,
        sea,
        |_, mode| {
            [
                Some(Transfer {
                    height: 1.0,
                    slope_wave_number: DVec2::new(
                        mode.wave_number_x_per_m,
                        mode.wave_number_y_per_m,
                    ),
                    yaw_m2: 0.0,
                }),
                None,
            ]
        },
    );
    depth_scale(sea.sea_height_m(), sea.water_depth_m) * whole_sea(&sums).orbital_displacement_m
}

/// How a mode enters a fit over a waterplane through a pixel footprint.
struct HullTransfer {
    waterplane: Waterplane,
    footprint_m: f64,
    along: DVec2,
    across: DVec2,
}

impl HullTransfer {
    fn new(waterplane: Waterplane, footprint_m: f64) -> Self {
        let (across_x, along_x) = libm::sincos(waterplane.heading_rad);
        Self {
            waterplane,
            footprint_m,
            along: DVec2::new(along_x, across_x),
            across: DVec2::new(-across_x, along_x),
        }
    }

    /// How the mode enters the fit, or `None` where the footprint skips its
    /// band.
    fn transfer(&self, band: usize, mode: &OceanWaveMode) -> Option<Transfer> {
        let lowest_wavenumber = std::f64::consts::TAU / OCEAN_WAVE_LONGEST_M;
        let band_wavenumber =
            lowest_wavenumber * libm::exp2(band as f64 / OCEAN_WAVE_BANDS_PER_OCTAVE as f64);
        if band_wavenumber * self.footprint_m >= 3.0 {
            return None;
        }
        let filtered = mode.wave_number_per_m * self.footprint_m;
        let filter = libm::exp(-0.5 * filtered * filtered);
        let wave_number = DVec2::new(mode.wave_number_x_per_m, mode.wave_number_y_per_m);
        let wave_number_along = wave_number.dot(self.along);
        let wave_number_across = wave_number.dot(self.across);
        let (height_along, slope_along) =
            waterplane_transfers(0.5 * wave_number_along * self.waterplane.length_m);
        let (height_across, slope_across) =
            waterplane_transfers(0.5 * wave_number_across * self.waterplane.beam_m);
        let (length_m, beam_m) = (self.waterplane.length_m, self.waterplane.beam_m);
        Some(Transfer {
            height: filter * height_along * height_across,
            slope_wave_number: filter
                * (wave_number_along * slope_along * height_across * self.along
                    + wave_number_across * height_along * slope_across * self.across),
            yaw_m2: filter
                * wave_number_along
                * wave_number_across
                * (length_m * length_m / 12.0 * slope_along * height_across
                    - beam_m * beam_m / 12.0 * height_along * slope_across),
        })
    }
}

/// How a mode enters a sum over a waterplane: `height` is the waterplane's
/// mean of its height, and so of its orbital velocity and of its pressure
/// gradient's load; `slope_wave_number` stands in for its wavenumber in the
/// fitted plane's slope; and `yaw_m2`, times its height at the centre, is
/// the waterplane's mean of y ∂η/∂x - x ∂η/∂y along and across the heading,
/// the yaw of -∇η, per mode a k∥k⊥ (L²/12 P(u) S(v) - B²/12 S(u) P(v)) cos φ.
struct Transfer {
    height: f64,
    slope_wave_number: DVec2,
    yaw_m2: f64,
}

/// A waterplane's height and slope transfers along one axis at half its
/// wavenumber times its extent, `u`: sin u / u and 3 (sin u - u cos u) / u³,
/// from their series where those cancel.
fn waterplane_transfers(u: f64) -> (f64, f64) {
    if libm::fabs(u) < 1.0e-2 {
        return (1.0 - u * u / 6.0, 1.0 - u * u / 10.0);
    }
    let (sine, cosine) = libm::sincos(u);
    (sine / u, 3.0 * (sine - u * cosine) / (u * u * u))
}

/// The share of a sea's unrestricted significant height that McCowan's
/// breaker limit leaves over water `water_depth_m` deep.
fn depth_scale(unrestricted_height_m: f64, water_depth_m: f32) -> f64 {
    if unrestricted_height_m > 0.0 {
        unrestricted_height_m.min(0.78 * f64::from(water_depth_m)) / unrestricted_height_m
    } else {
        0.0
    }
}

/// Tayfun's second order of a sum scaled by `depth_scale`, at the mean
/// wavenumber of its window's `(variance, wavenumber-weighted variance)`.
fn second_order(sum: &SurfaceSum, depth_scale: f64, window: (f64, f64)) -> SurfaceFit {
    let mean_wavenumber = if window.0 > 0.0 {
        window.1 / window.0
    } else {
        0.0
    };
    let linear_m = depth_scale * sum.displacement_m;
    let quadrature_m = depth_scale * sum.quadrature_m;
    SurfaceFit {
        height_m: linear_m
            + 0.5 * mean_wavenumber * (linear_m * linear_m - quadrature_m * quadrature_m),
        slope: depth_scale * sum.slope
            + mean_wavenumber
                * depth_scale
                * (linear_m * sum.slope - quadrature_m * sum.quadrature_slope),
        orbital_velocity_m_s: depth_scale * sum.orbital_velocity_m_s,
        drift_velocity_m_s: depth_scale * depth_scale * sum.drift_velocity_m_s,
        water_acceleration_m_s2: depth_scale * sum.water_acceleration_m_s2,
        yaw_moment_m2_s2: depth_scale * sum.yaw_moment_m2_s2,
    }
}

/// The wind sea's sum and the swell's excess over it, by one transfer.
#[derive(Default)]
struct PartitionSums {
    wind: SurfaceSum,
    swell_excess: SurfaceSum,
}

/// A table mode's band and direction, its amplitude, and the amplitude of
/// the component it adds to: the wind sea's, for the swell's excess.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SeaComponent {
    band: u8,
    direction: u8,
    amplitude_m: f64,
    base_amplitude_m: f64,
}

impl SeaComponent {
    const NONE: Self = Self {
        band: 0,
        direction: 0,
        amplitude_m: 0.0,
        base_amplitude_m: 0.0,
    };
}

/// The sea a forcing raises, apart from the clock and the position: the
/// wind sea's components, the swell's excess over them, the partitions'
/// variances and the variance they share, the wind sea's window's and the
/// whole sea's windows' variance and wavenumber-weighted variance, and the
/// water's depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeaComponents {
    wind: [SeaComponent; VISITED_BANDS * VISITED_DIRECTIONS],
    wind_count: usize,
    swell_excess: [SeaComponent; SWELL_VISITED_BANDS * SWELL_VISITED_DIRECTIONS],
    swell_count: usize,
    wind_sea_variance_m2: f64,
    swell_variance_m2: f64,
    shared_variance_m2: f64,
    wind_window: (f64, f64),
    sea_window: (f64, f64),
    water_depth_m: f32,
}

impl SeaComponents {
    /// No sea, as over dry ground.
    pub const NONE: Self = Self {
        wind: [SeaComponent::NONE; VISITED_BANDS * VISITED_DIRECTIONS],
        wind_count: 0,
        swell_excess: [SeaComponent::NONE; SWELL_VISITED_BANDS * SWELL_VISITED_DIRECTIONS],
        swell_count: 0,
        wind_sea_variance_m2: 0.0,
        swell_variance_m2: 0.0,
        shared_variance_m2: 0.0,
        wind_window: (0.0, 0.0),
        sea_window: (0.0, 0.0),
        water_depth_m: 0.0,
    };

    fn sea_height_m(&self) -> f64 {
        4.0 * libm::sqrt(
            (self.wind_sea_variance_m2 + self.swell_variance_m2 - self.shared_variance_m2).max(0.0),
        )
    }

    fn wind_sea_height_m(&self) -> f64 {
        4.0 * libm::sqrt(self.wind_sea_variance_m2)
    }
}

/// The sea `forcing` raises in a world and seed, at every clock and
/// position: a component both seas share takes the larger amplitude, the
/// swell adding only its excess there, and the variance it shares counts
/// once.
pub fn sea_components(
    world_geometry: WorldGeometry,
    match_seed: u64,
    forcing: OceanWaveForcing,
) -> SeaComponents {
    let band_rotation_rad = ocean_wave_table_at_start(world_geometry, match_seed).band_rotation_rad;
    let wind_sea = SeaPartition::wind_sea(forcing.surface_wind_velocity_m_s);
    let swell = SeaPartition::swell(forcing.swell);
    let mut components = SeaComponents {
        wind_sea_variance_m2: wind_sea.variance_m2,
        swell_variance_m2: swell.variance_m2,
        water_depth_m: forcing.water_depth_m,
        ..SeaComponents::NONE
    };
    let mut wind_amplitudes = [[0.0; OCEAN_WAVE_DIRECTIONS]; OCEAN_WAVE_BAND_COUNT];
    components.wind_window = for_each_component(
        &band_rotation_rad,
        &wind_sea,
        |band, direction, amplitude_m, _| {
            wind_amplitudes[band][direction] = amplitude_m;
            components.wind[components.wind_count] = SeaComponent {
                band: band as u8,
                direction: direction as u8,
                amplitude_m,
                base_amplitude_m: 0.0,
            };
            components.wind_count += 1;
        },
    );
    components.sea_window = components.wind_window;
    for_each_component(
        &band_rotation_rad,
        &swell,
        |band, direction, amplitude_m, wavenumber| {
            let wind_amplitude_m = wind_amplitudes[band][direction];
            let excess_variance =
                0.5 * (amplitude_m.max(wind_amplitude_m).powi(2) - wind_amplitude_m.powi(2));
            components.swell_excess[components.swell_count] = SeaComponent {
                band: band as u8,
                direction: direction as u8,
                amplitude_m: (amplitude_m - wind_amplitude_m).max(0.0),
                base_amplitude_m: wind_amplitude_m,
            };
            components.swell_count += 1;
            components.shared_variance_m2 += 0.5 * amplitude_m.min(wind_amplitude_m).powi(2);
            components.sea_window.0 += excess_variance;
            components.sea_window.1 += excess_variance * wavenumber;
        },
    );
    components
}

/// The sums of `sea`'s components at a point and clock by each of
/// `transfer`'s two transfers, through which each mode enters by its share
/// of height and the wavenumber that turns its height into slope, or not at
/// all where it gives `None`. Both share the modes' phases.
fn sea_sums(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    position_m: Vector,
    sea: &SeaComponents,
    transfer: impl Fn(usize, &OceanWaveMode) -> [Option<Transfer>; 2],
) -> [PartitionSums; 2] {
    let table = ocean_wave_table(world_geometry, match_seed, world_tick, world_tick_fraction);
    let width_m = f64::from(world_geometry.width_m() as f32);
    let x_m = f64::from(position_m.x).rem_euclid(width_m);
    let y_m = f64::from(position_m.y);
    let mut sums: [PartitionSums; 2] = Default::default();
    for (components, swell) in [
        (&sea.wind[..sea.wind_count], false),
        (&sea.swell_excess[..sea.swell_count], true),
    ] {
        for component in components {
            let band = usize::from(component.band);
            let mode = &table.modes[band][usize::from(component.direction)];
            let transfers = transfer(band, mode);
            if transfers.iter().any(Option::is_some) {
                let phase = mode.phase_at(x_m, y_m);
                for (sums, transfer) in sums.iter_mut().zip(transfers) {
                    if let Some(transfer) = transfer {
                        let sum = if swell {
                            &mut sums.swell_excess
                        } else {
                            &mut sums.wind
                        };
                        sum.add(mode, component, phase, &transfer);
                    }
                }
            }
        }
    }
    sums
}

/// The whole sea's sum.
fn whole_sea(sums: &PartitionSums) -> SurfaceSum {
    let PartitionSums { wind, swell_excess } = sums;
    SurfaceSum {
        displacement_m: wind.displacement_m + swell_excess.displacement_m,
        quadrature_m: wind.quadrature_m + swell_excess.quadrature_m,
        slope: wind.slope + swell_excess.slope,
        quadrature_slope: wind.quadrature_slope + swell_excess.quadrature_slope,
        orbital_displacement_m: wind.orbital_displacement_m + swell_excess.orbital_displacement_m,
        orbital_velocity_m_s: wind.orbital_velocity_m_s + swell_excess.orbital_velocity_m_s,
        water_acceleration_m_s2: wind.water_acceleration_m_s2
            + swell_excess.water_acceleration_m_s2,
        yaw_moment_m2_s2: wind.yaw_moment_m2_s2 + swell_excess.yaw_moment_m2_s2,
        drift_velocity_m_s: wind.drift_velocity_m_s + swell_excess.drift_velocity_m_s,
    }
}

/// Sums of deep-water modes at a point or over a waterplane: height, its
/// quadrature, their slopes, the surface's horizontal orbital displacement
/// and velocity, the
/// water's acceleration under the surface's pressure gradient, -g ∇η, which
/// in deep water is that velocity's rate of change, and the yaw moment that
/// gradient exerts per unit of displaced mass; and the mean drift of a hull
/// that rides the surface, from its heave through the orbital velocity's
/// vertical shear. Compromise: the load takes the linear wave's local
/// acceleration, leaving out the second-order convective acceleration, whose
/// mean is zero.
#[derive(Default)]
struct SurfaceSum {
    displacement_m: f64,
    quadrature_m: f64,
    slope: DVec2,
    quadrature_slope: DVec2,
    orbital_displacement_m: DVec2,
    orbital_velocity_m_s: DVec2,
    water_acceleration_m_s2: DVec2,
    yaw_moment_m2_s2: f64,
    drift_velocity_m_s: DVec2,
}

impl SurfaceSum {
    /// Adds a component's mode at `(sin, cos)` of its phase by `transfer`.
    ///
    /// A hull at depth z under the heaving surface meets the orbital
    /// velocity's vertical shear, k u in deep water, so its velocity gains
    /// k η u; a mode's mean of that is ½ a² ω k along its travel, half of
    /// Stokes's (1847) drift a² ω k, and the hull's own motion through the
    /// waves supplies the other half. The drift takes the component's
    /// amplitude together with the one it adds to, as its mode's whole
    /// amplitude. Compromises: the drift keeps that product's mean alone,
    /// leaving out its oscillation at twice the wave's frequency and the
    /// products of different modes, which extrapolating short waves' shear
    /// through a long swell's heave would make unphysically large; and the
    /// hull takes the shear and the orbital velocity at the surface, without
    /// their decay to its immersion, as the heave leaves out the pressure's
    /// decay to its draft.
    fn add(
        &mut self,
        mode: &OceanWaveMode,
        component: &SeaComponent,
        (sine, cosine): (f64, f64),
        transfer: &Transfer,
    ) {
        let amplitude_m = component.amplitude_m;
        let wave_number = DVec2::new(mode.wave_number_x_per_m, mode.wave_number_y_per_m);
        let height_m = transfer.height * amplitude_m;
        self.displacement_m += height_m * cosine;
        self.quadrature_m += height_m * sine;
        self.slope -= amplitude_m * sine * transfer.slope_wave_number;
        self.quadrature_slope += amplitude_m * cosine * transfer.slope_wave_number;
        self.orbital_displacement_m -= height_m * sine / mode.wave_number_per_m * wave_number;
        self.orbital_velocity_m_s +=
            height_m * mode.angular_frequency_rad_s * cosine / mode.wave_number_per_m * wave_number;
        self.water_acceleration_m_s2 += GRAVITY_M_S2 * height_m * sine * wave_number;
        self.yaw_moment_m2_s2 += GRAVITY_M_S2 * amplitude_m * cosine * transfer.yaw_m2;
        let whole_amplitude_m = component.base_amplitude_m + amplitude_m;
        self.drift_velocity_m_s += 0.5
            * transfer.height
            * transfer.height
            * (whole_amplitude_m * whole_amplitude_m
                - component.base_amplitude_m * component.base_amplitude_m)
            * mode.angular_frequency_rad_s
            * wave_number;
    }
}

/// The wave table at a clock. Each band's modes take seeded
/// wavenumbers within the band, with eastward wavenumbers fitted to the
/// world's width so waves join across longitude. Of the modes only the
/// phases depend on the clock, so the rest is built once per world and seed.
pub fn ocean_wave_table(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
) -> Rc<OceanWaveTable> {
    let inputs = (
        world_geometry,
        match_seed,
        world_tick,
        world_tick_fraction.to_bits(),
    );
    LATEST_WAVE_TABLE.with_borrow_mut(|latest| {
        if let Some((latest_inputs, table)) = latest
            && *latest_inputs == inputs
        {
            return table.clone();
        }
        let start = ocean_wave_table_at_start(world_geometry, match_seed);
        let table = Rc::new(OceanWaveTable {
            band_rotation_rad: start.band_rotation_rad,
            modes: start
                .modes
                .map(|band| band.map(|mode| mode.at_clock(world_tick, world_tick_fraction))),
            swell_clock: swell_clock(world_geometry, match_seed, world_tick, world_tick_fraction),
        });
        *latest = Some((inputs, table.clone()));
        table
    })
}

/// The wave modes at the clock's start, with each band's direction rotation.
struct WaveModesAtStart {
    band_rotation_rad: [f64; OCEAN_WAVE_BAND_COUNT],
    modes: [[OceanWaveMode; OCEAN_WAVE_DIRECTIONS]; OCEAN_WAVE_BAND_COUNT],
}

fn ocean_wave_table_at_start(
    world_geometry: WorldGeometry,
    match_seed: u64,
) -> Rc<WaveModesAtStart> {
    WAVE_TABLE_AT_START.with_borrow_mut(|latest| {
        if let Some((inputs, table)) = latest
            && *inputs == (world_geometry, match_seed)
        {
            return table.clone();
        }
        let lowest_wavenumber = std::f64::consts::TAU / OCEAN_WAVE_LONGEST_M;
        let spacing = std::f64::consts::TAU / OCEAN_WAVE_DIRECTIONS as f64;
        let band_rotation_rad: [f64; OCEAN_WAVE_BAND_COUNT] = std::array::from_fn(|band| {
            spacing
                * hash_unit(
                    match_seed
                        ^ 0x5be0_cd19_137e_2179
                        ^ (band as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
                )
        });
        let modes = std::array::from_fn(|band| {
            std::array::from_fn(|direction| {
                let mode_key = match_seed
                    ^ 0x1f83_d9ab_fb41_bd6b
                    ^ ((band * OCEAN_WAVE_DIRECTIONS + direction) as u64)
                        .wrapping_mul(0x9e37_79b9_7f4a_7c15);
                let wave_number = lowest_wavenumber
                    * libm::exp2(
                        (band as f64 + hash_unit(mode_key ^ 0xe703_7ed1_a0b4_28db))
                            / OCEAN_WAVE_BANDS_PER_OCTAVE as f64,
                    );
                ocean_wave_mode(
                    world_geometry,
                    band_rotation_rad[band] + spacing * direction as f64,
                    std::f64::consts::TAU / wave_number,
                    mode_key,
                )
            })
        });
        let table = Rc::new(WaveModesAtStart {
            band_rotation_rad,
            modes,
        });
        *latest = Some(((world_geometry, match_seed), table.clone()));
        table
    })
}

/// A mode at the clock's start, of seeded phase.
fn ocean_wave_mode(
    world_geometry: WorldGeometry,
    direction_angle: f64,
    wavelength_m: f64,
    mode_key: u64,
) -> OceanWaveMode {
    let width_m = f64::from(world_geometry.width_m() as f32);
    let (direction_y, direction_x) = libm::sincos(direction_angle);
    let mut mode_x = libm::round(direction_x * width_m / wavelength_m) as i64;
    if mode_x == 0 && direction_y.abs() < 1.0e-6 {
        mode_x = if direction_x < 0.0 { -1 } else { 1 };
    }
    let wave_number_x_per_m = std::f64::consts::TAU * mode_x as f64 / width_m;
    let wave_number_y_per_m = std::f64::consts::TAU * direction_y / wavelength_m;
    let wave_number_per_m = libm::hypot(wave_number_x_per_m, wave_number_y_per_m);
    OceanWaveMode {
        wave_number_x_per_m,
        wave_number_y_per_m,
        wave_number_per_m,
        angular_frequency_rad_s: libm::sqrt(GRAVITY_M_S2 * wave_number_per_m),
        phase_rad: std::f64::consts::TAU * hash_unit(mode_key ^ 0xbf58_476d_1ce4_e5b9),
    }
}

impl OceanWaveMode {
    /// `(sin, cos)` of the mode's phase at `x_m`, in `[0, width)`, and `y_m`.
    fn phase_at(&self, x_m: f64, y_m: f64) -> (f64, f64) {
        libm::sincos(
            self.wave_number_x_per_m * x_m + self.wave_number_y_per_m * y_m + self.phase_rad,
        )
    }

    fn at_clock(self, world_tick: u64, world_tick_fraction: f32) -> Self {
        let phase_per_tick = self.angular_frequency_rad_s * TICK_SECONDS;
        Self {
            phase_rad: self.phase_rad
                - tick_phase_radians(world_tick, phase_per_tick)
                - f64::from(world_tick_fraction) * phase_per_tick,
            ..self
        }
    }
}

type WaveModeInputs = (WorldGeometry, u64, u64, u32);
type CachedWaveTable<Inputs, Table> = RefCell<Option<(Inputs, Rc<Table>)>>;

thread_local! {
    // Keyed on every input of the tables, which depend on world, seed and
    // clock alone, so all samples at one clock share one table.
    static WAVE_TABLE_AT_START: CachedWaveTable<(WorldGeometry, u64), WaveModesAtStart> =
        const { RefCell::new(None) };
    static LATEST_WAVE_TABLE: CachedWaveTable<WaveModeInputs, OceanWaveTable> =
        const { RefCell::new(None) };
}

fn tick_phase_radians(tick: u64, phase_per_tick_radians: f64) -> f64 {
    let mut phase = 0.0;
    for shift in [48, 32, 16, 0] {
        let chunk = ((tick >> shift) & 0xffff) as f64;
        phase =
            (phase * 65_536.0 + chunk * phase_per_tick_radians).rem_euclid(std::f64::consts::TAU);
    }
    phase
}
