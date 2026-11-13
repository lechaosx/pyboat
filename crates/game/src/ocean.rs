use std::cell::RefCell;
use std::rc::Rc;

use physics::glamx::{DVec2, Vec2 as Vector};
use simulation::WorldGeometry;

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
// has no height.
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
        &table,
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
    table: &OceanWaveTable,
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
        let position = (sea.direction_rad - table.band_rotation_rad[band]) / spacing;
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
// The spread of the swell's natural logarithm of height and peak period at a
// point once the annual cycle is removed: from the coefficients of variation
// CV of a point's ratios to its calendar month's mean, σ = √ln(1 + CV²), the
// log-normal of that CV, whose deciles match the measured within 0.05 of the
// mean between 0° and 30° and the median and upper decile beyond.
const SWELL_HEIGHT_LOG_SPREAD: [f32; 10] =
    [0.34, 0.41, 0.43, 0.45, 0.52, 0.59, 0.60, 0.62, 0.62, 0.62];
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
// The swell's height, period and direction each wander with one slow signal,
// a sum of `SWELL_SIGNAL_TERMS` cosines of seeded phases and of frequencies
// drawn from a Gaussian of standard deviation √2 / `SWELL_COHERENCE_S`,
// whose autocorrelation is exp(-(τ / `SWELL_COHERENCE_S`)²) in expectation.
// At a point the measured autocorrelation of the swell's log height falls to
// 1/e in 21 h and its period's and direction's in 18 h (3-hourly series at 75
// open-ocean points over June and July 2019); `SWELL_COHERENCE_S` is their
// 20 h. Compromises: the signals are the same everywhere, so swell
// events reach every longitude at once; they are independent of each other,
// where the measured correlation of log height and period is +0.1 to +0.3;
// and the measured autocorrelation falls faster in the first 6 h, to 0.70,
// which the smooth signal leaves out.
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

/// The swell climate's knots for the renderer, two rows to a knot at 0°,
/// 10°, … 90°: height, its annual amplitude, period and its annual
/// amplitude, then direction, the spreads of log height and log period, and
/// the direction's spread in degrees.
pub fn swell_climate_knots() -> [[f32; 4]; 20] {
    std::array::from_fn(|row| {
        let knot = row / 2;
        if row % 2 == 0 {
            [
                SWELL_HEIGHT_M[knot],
                SWELL_HEIGHT_SEASONAL_AMPLITUDE[knot],
                SWELL_PERIOD_S[knot],
                SWELL_PERIOD_SEASONAL_AMPLITUDE_S[knot],
            ]
        } else {
            [
                SWELL_DIRECTION_DEGREES[knot],
                SWELL_HEIGHT_LOG_SPREAD[knot],
                SWELL_PERIOD_LOG_SPREAD[knot],
                SWELL_DIRECTION_SPREAD_DEGREES[knot],
            ]
        }
    })
}

/// The clock's part of the background swell: the annual cycle at the
/// northern hemisphere's latitudes, cos 2π(season - winter), and the slow
/// signals its height, period and direction wander with, each of unit
/// variance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwellClock {
    pub season: f64,
    pub height: f64,
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
        height: signal(0),
        period: signal(1),
        direction: signal(2),
    }
}

/// The background swell at a point and clock: the swell that storms beyond
/// the world's scale send a sea at its latitude in its season, wandering with
/// slow signals that keep the climate's mean square height and mean period,
/// log-normal in both. Compromise: every water point takes the open sea's
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
    let height_spread = knot(SWELL_HEIGHT_LOG_SPREAD);
    let period_spread = knot(SWELL_PERIOD_LOG_SPREAD);
    let significant_height_m = knot(SWELL_HEIGHT_M)
        * libm::sqrt(1.0 + knot(SWELL_HEIGHT_SEASONAL_AMPLITUDE) * season)
        * libm::exp(height_spread * clock.height - height_spread * height_spread);
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

pub(crate) fn procedural_wave_state(query: OceanQuery) -> OceanState {
    ocean_wave_state(
        query.world_geometry,
        query.match_seed,
        query.world_tick,
        query.world_tick_fraction,
        query.position_m,
        OceanWaveForcing {
            surface_wind_velocity_m_s: query.wave_forcing_wind_velocity_m_s,
            swell: background_swell(
                query.world_geometry,
                query.match_seed,
                query.world_tick,
                query.world_tick_fraction,
                query.position_m,
            ),
            water_depth_m: query.water_depth_m,
        },
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
    let table = ocean_wave_table(world_geometry, match_seed, world_tick, world_tick_fraction);
    let width_m = f64::from(world_geometry.width_m() as f32);
    let x_m = f64::from(position_m.x).rem_euclid(width_m);
    let y_m = f64::from(position_m.y);
    let mut sum = SurfaceSum::default();
    let wind_sea = SeaPartition::wind_sea(forcing.surface_wind_velocity_m_s);
    let swell = SeaPartition::swell(forcing.swell);
    let mut wind_amplitudes = [[0.0; OCEAN_WAVE_DIRECTIONS]; OCEAN_WAVE_BAND_COUNT];
    let (mut window_variance, mut weighted_wavenumber) =
        for_each_component(&table, &wind_sea, |band, direction, amplitude_m, _| {
            wind_amplitudes[band][direction] = amplitude_m;
            sum.add(&table.modes[band][direction], amplitude_m, x_m, y_m);
        });
    // A component both seas share takes the larger amplitude: the swell adds
    // only its excess there, and the variance it shares counts once.
    let mut shared_variance = 0.0;
    for_each_component(
        &table,
        &swell,
        |band, direction, amplitude_m, wavenumber| {
            let wind_amplitude_m = wind_amplitudes[band][direction];
            let excess_variance =
                0.5 * (amplitude_m.max(wind_amplitude_m).powi(2) - wind_amplitude_m.powi(2));
            sum.add(
                &table.modes[band][direction],
                (amplitude_m - wind_amplitude_m).max(0.0),
                x_m,
                y_m,
            );
            shared_variance += 0.5 * amplitude_m.min(wind_amplitude_m).powi(2);
            window_variance += excess_variance;
            weighted_wavenumber += excess_variance * wavenumber;
        },
    );
    let unrestricted_height_m =
        4.0 * libm::sqrt((wind_sea.variance_m2 + swell.variance_m2 - shared_variance).max(0.0));
    let significant_height_m = unrestricted_height_m.min(0.78 * f64::from(forcing.water_depth_m));
    let depth_scale = if unrestricted_height_m > 0.0 {
        significant_height_m / unrestricted_height_m
    } else {
        0.0
    };
    let mean_wavenumber = if window_variance > 0.0 {
        weighted_wavenumber / window_variance
    } else {
        0.0
    };
    let linear_m = depth_scale * sum.displacement_m;
    let quadrature_m = depth_scale * sum.quadrature_m;
    let displacement_m =
        linear_m + 0.5 * mean_wavenumber * (linear_m * linear_m - quadrature_m * quadrature_m);
    let slope = depth_scale * sum.slope
        + mean_wavenumber
            * depth_scale
            * (linear_m * sum.slope - quadrature_m * sum.quadrature_slope);
    let orbital_velocity_m_s = depth_scale * sum.orbital_velocity_m_s;

    let surface_displacement_m = displacement_m as f32;
    let significant_wave_height_m = significant_height_m as f32;
    OceanState {
        current_velocity_m_s: Vector::ZERO,
        wave_orbital_velocity_m_s: orbital_velocity_m_s.as_vec2(),
        significant_wave_height_m,
        surface_displacement_m,
        surface_slope: slope.as_vec2(),
        rogue_crest_excess_m: (surface_displacement_m - significant_wave_height_m).max(0.0),
    }
}

/// Sums of deep-water modes at a point: height, its quadrature, their slopes,
/// and the surface's horizontal orbital velocity.
#[derive(Default)]
struct SurfaceSum {
    displacement_m: f64,
    quadrature_m: f64,
    slope: DVec2,
    quadrature_slope: DVec2,
    orbital_velocity_m_s: DVec2,
}

impl SurfaceSum {
    fn add(&mut self, mode: &OceanWaveMode, amplitude_m: f64, x_m: f64, y_m: f64) {
        let wave_number = DVec2::new(mode.wave_number_x_per_m, mode.wave_number_y_per_m);
        let phase = wave_number.x * x_m + wave_number.y * y_m + mode.phase_rad;
        let (sine, cosine) = libm::sincos(phase);
        self.displacement_m += amplitude_m * cosine;
        self.quadrature_m += amplitude_m * sine;
        self.slope -= amplitude_m * sine * wave_number;
        self.quadrature_slope += amplitude_m * cosine * wave_number;
        self.orbital_velocity_m_s += amplitude_m * mode.angular_frequency_rad_s * cosine
            / mode.wave_number_per_m
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
