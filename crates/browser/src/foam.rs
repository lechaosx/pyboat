//! Whitecaps and the foam they leave: a seeded space–time process of breaking
//! events, which compute passes render each frame into a foam grid anchored
//! to the world. This module holds the model's laws, the per-frame state the
//! passes read, and the passes themselves; `foam-*.wgsl` mirrors the laws.

use std::{
    borrow::Cow,
    f64::consts::{PI, TAU},
    sync::LazyLock,
};

use bytemuck::{Pod, Zeroable};
use simulation::{TICKS_PER_SECOND, WorldGeometry};

use crate::FOAM_SHADER;

const GRAVITY_M_S2: f64 = 9.81;

/// Event classes: octaves of breaking speed from the slowest whitecap.
pub const FOAM_BINS: usize = 5;

/// A breaking crest advances at this fraction of its wave's phase speed:
/// 0.8 (Melville & Matusov 2002), 0.84 (Tian et al. 2010) and 0.9 ± 0.04
/// (Banner & Peirson 2007), as Perlin, Choi & Tian (2013, Annu. Rev. Fluid
/// Mech. 45) review them.
const BREAKING_SPEED_RATIO: f64 = 0.85;

/// The slowest breaker that whitens visibly. Kleiss & Melville (2010, J.
/// Phys. Oceanogr. 40) detect breakers lasting more than 2/3 s whose bubble
/// patch exceeds 0.5 m²; in this event geometry the duration holds from
/// 0.89 m/s and a median crest's swept patch from 0.99 m/s. Their slower
/// breakers depend on image resolution, so the cut stands in for "visible
/// whitecap" rather than for physics.
const SLOWEST_BREAKING_SPEED_M_S: f64 = 0.9;

/// Kleiss & Melville's Rayleigh width of the breaking-crest length
/// distribution, σ/u* = 1.69 χ^0.06 for dimensionless fetch χ. Compromise:
/// the fully developed seas lie beyond their fetches; χ of 10⁶ to 10⁸ gives
/// 3.9 to 5.1, of which this is a value within about 15%.
const BREAKING_SPEED_SPREAD_PER_FRICTION_VELOCITY: f64 = 4.6;

/// The Rayleigh distribution is cut at this many widths, dropping about 3%
/// of its speed-weighted mass; its fastest breakers lie above the Rayleigh
/// curve, which is not followed there.
const BREAKING_SPEED_CUT_WIDTHS: f64 = 3.0;

/// Holthuijsen, Powell & Pietrzak's (2012, J. Geophys. Res. 117) plateau of
/// whitecap coverage above about 24 m/s.
const COVERAGE_PLATEAU: f64 = 0.04;

/// Koepke's (1984, Appl. Opt. 23) threshold for white, which he assumed for
/// the coverage measurements that did not state theirs. Coverage counts a
/// patch while its mean reflectance stays above it.
const WHITE_THRESHOLD: f64 = 0.10;

/// Foam fainter than this reflectance is no longer drawn.
const FAINTEST_REFLECTANCE: f64 = 0.005;

/// Koepke's normalized patch area a(t) = 1 - exp(-0.25 t), fitted to
/// whitecaps in winds of 7.5 to 8.5 m/s. Compromise: one curve for every
/// scale and wind.
const AREA_GROWTH_PER_S: f64 = 0.25;

/// Fresh foam's scaled (two-stream) optical depth and its e-folding time,
/// the least-squares fit of its reflectance to the patch-mean reflectance
/// that inverting Koepke's Table I through a(t) gives; the test
/// `calibration_fits_koepke` refits them.
pub const FRESH_OPTICAL_DEPTH: f64 = 0.798;
pub const OPTICAL_DEPTH_EFOLD_S: f64 = 4.07;

/// Foam is drawn as filaments as thick as fresh foam, over the share of the
/// patch that gives it its mean reflectance, so fresh foam is a solid sheet
/// and aged foam a net with open water between. A named trick for its look:
/// the patch's mean reflectance holds, while the light it passes is that
/// of thick filaments and open water rather than of thinner foam.
/// Compromise: a representative patch's area counted white e-folds in about
/// 1.2 s, faster than Callaghan et al.'s (2012) 1.4 to 4.8 s.
fn drawn_cover(age_s: f64) -> f64 {
    patch_mean_reflectance(age_s) / layer_reflectance(FRESH_OPTICAL_DEPTH)
}

/// Single breaking segments' foam spreads across their travel as by
/// diffusion, its width w² = L² + 2Kt from release, the diffusivity fitted
/// so a representative patch's area grows as Koepke's a(t) over its first
/// eight seconds; `calibration_fits_koepke` refits it. Compromise: Koepke's
/// growth saturates where diffusion keeps spreading, by about 7% at ten
/// seconds.
pub const LATERAL_DIFFUSIVITY_M2_S: f64 = 4.1;

/// A single segment's warped edges move at most this share of the band
/// along the travel and of its crest across it. Keep synchronized with
/// `FOAM_WARP_REACH` in foam-generate.wgsl.
const WARP_REACH: f64 = 0.45;

/// The slowest decay the regional persistence of foam may take: Callaghan
/// et al.'s longest area decay over the centre of their range. Event slots
/// last long enough for it, so they never change with the decay.
const SLOWEST_DECAY_RATIO: f64 = 4.8 / 2.6;

/// Breaking segments are log-normal in length with the mode Gemmrich et al.
/// (2013, J. Geophys. Res. 118, §5) measured, 1 to 2 m, and 0.1% beyond
/// the longest they saw, over 15 m: mode 1.5 m and σ = 0.62, so median
/// 2.20 m. Their histogram gives no more of its shape, which is the named
/// assumption; lengths beyond 15 m are held at it. Keep synchronized with
/// the `FOAM_SEGMENT_*` constants in foam-generate.wgsl.
const SEGMENT_MEDIAN_M: f64 = 2.2045;
const SEGMENT_LOG_DEVIATION: f64 = 0.6205;
const LONGEST_SEGMENT_M: f64 = 15.0;

/// The classes from this one on break as rows along their dominant crests:
/// a row is the wave's length times a log-normal factor of median 1, cut at
/// 3 wavelengths. Slower classes break as single segments, so the event
/// rates of the fastest classes stay within their candidates. Along a row,
/// cells of SEGMENT_SPACING_M hold no, one or two segments, a quarter, half
/// and a quarter of the time, each starting anywhere in its cell, so a row
/// is about half filled and shows no lattice; the counts are a named trick.
/// Keep synchronized with `foam_event_cover` in foam-generate.wgsl.
pub const FIRST_ROW_BIN: usize = 3;
const ROW_LOG_DEVIATION: f64 = 0.5;
const LONGEST_ROW_WAVELENGTHS: f64 = 3.0;
const SEGMENT_SPACING_M: f64 = 5.4;

/// The fresh roller riding the breaking crest's front: Duncan's (1981, Proc.
/// R. Soc. A 377) breaking region, self-similar with area 0.11 L_r² (his Eq.
/// 7, as Carini et al. 2015, J. Geophys. Res. 120, Eq. 7 quote it) and
/// density 0.6 of the water's, whose weight down the slope θ balances his
/// measured drag 0.009 ρ c⁴ / (g sin θ) (Perlin et al. 2013, Annu. Rev.
/// Fluid Mech. 45, p. 128), so L_r = 0.369 c² / (g sin θ); at his middle
/// slope, 12.35°, that is 0.275 of the wavelength (0.23 to 0.34 over his
/// slopes of 10° to 14.7°). Compromise: his quasi-steady laboratory
/// breakers of 0.6 to 1 m/s, scaled by Froude to the sea.
const ROLLER_WAVELENGTHS: f64 = 0.275;

/// Flatau et al.'s (2000, arXiv physics/0006060) asymmetry parameter of
/// bubbles in seawater in the geometric-optics regime. Compromise: foam is
/// mostly air in water films, whose own asymmetry may differ.
pub const BUBBLE_ASYMMETRY: f64 = 0.85;

/// The friction velocity of a sustained 10 m wind under the neutral log
/// profile with Charnock's roughness, as `game::wind_sea` takes it.
fn friction_velocity_m_s(wind_m_s: f64) -> f64 {
    if wind_m_s <= 0.0 {
        return 0.0;
    }
    let constant = (10.0 * GRAVITY_M_S2 / (0.011 * 0.41 * 0.41 * wind_m_s * wind_m_s)).ln();
    let mut log_height = 10.0;
    for _ in 0..4 {
        log_height = constant + 2.0 * f64::ln(log_height);
    }
    0.41 * wind_m_s / log_height
}

/// Whitecap coverage under a sustained 10 m wind: Monahan and
/// O'Muircheartaigh's (1980, J. Phys. Oceanogr. 10) 3.84×10⁻⁶ U^3.41,
/// saturating smoothly at Holthuijsen et al.'s plateau in their tanh form.
/// Compromise: the power law reaches the plateau near 15 m/s where
/// Holthuijsen et al.'s own fit reaches it near 24 m/s, so this lies about
/// twice above theirs at 18 to 22 m/s; their overshoot is left out.
pub fn whitecap_coverage(wind_m_s: f64) -> f64 {
    if wind_m_s <= 0.0 {
        return 0.0;
    }
    COVERAGE_PLATEAU * (3.84e-6 * wind_m_s.powf(3.41) / COVERAGE_PLATEAU).tanh()
}

/// A breaker of breaking speed `speed_m_s`: its wave's wavelength and
/// period, from deep-water dispersion.
#[derive(Clone, Copy, Debug)]
struct Breaker {
    speed_m_s: f64,
    wavelength_m: f64,
    period_s: f64,
}

impl Breaker {
    fn new(speed_m_s: f64) -> Self {
        let phase_speed = speed_m_s / BREAKING_SPEED_RATIO;
        Self {
            speed_m_s,
            wavelength_m: TAU * phase_speed * phase_speed / GRAVITY_M_S2,
            period_s: TAU * phase_speed / GRAVITY_M_S2,
        }
    }

    /// The width of the band the crest sweeps while it breaks, one period.
    fn band_m(&self) -> f64 {
        self.speed_m_s * self.period_s
    }
}

fn bin_speeds_m_s(bin: usize) -> [f64; 2] {
    let slowest = SLOWEST_BREAKING_SPEED_M_S * (1u32 << bin) as f64;
    [slowest, 2.0 * slowest]
}

/// Koepke's normalized patch area at `age_s`.
fn area_growth(age_s: f64) -> f64 {
    1.0 - (-AREA_GROWTH_PER_S * age_s).exp()
}

/// The swept patch's area scale after its active phase of `period_s`: it
/// dilates about its centre so its area follows a(t) / a(T).
fn dilation(age_s: f64, period_s: f64) -> f64 {
    if age_s <= period_s {
        1.0
    } else {
        (area_growth(age_s) / area_growth(period_s)).sqrt()
    }
}

/// Foam's mean scaled optical depth at `age_s` from release.
fn optical_depth(age_s: f64) -> f64 {
    FRESH_OPTICAL_DEPTH * (-age_s / OPTICAL_DEPTH_EFOLD_S).exp()
}

/// The reflectance of a grey, non-absorbing layer of scaled optical depth
/// `depth` in the two-stream approximation: τ*/(1 + τ*), which saturates at
/// one by energy conservation.
fn layer_reflectance(depth: f64) -> f64 {
    depth / (1.0 + depth)
}

/// A patch's mean reflectance at `age_s`.
fn patch_mean_reflectance(age_s: f64) -> f64 {
    layer_reflectance(optical_depth(age_s))
}

/// Standard normal points and weights: midpoints over ±6 deviations.
static NORMAL_POINTS: LazyLock<Vec<(f64, f64)>> = LazyLock::new(|| {
    const POINTS: usize = 96;
    let step = 12.0 / POINTS as f64;
    let points: Vec<(f64, f64)> = (0..POINTS)
        .map(|index| {
            let z = -6.0 + (index as f64 + 0.5) * step;
            (z, (-0.5 * z * z).exp())
        })
        .collect();
    let total: f64 = points.iter().map(|(_, weight)| weight).sum();
    points
        .into_iter()
        .map(|(z, weight)| (z, weight / total))
        .collect()
});

/// The age at which a patch's mean reflectance falls to `reflectance`.
fn age_at_reflectance_s(reflectance: f64) -> f64 {
    let [mut young, mut old] = [0.0, 200.0];
    for _ in 0..60 {
        let age = 0.5 * (young + old);
        if patch_mean_reflectance(age) > reflectance {
            young = age;
        } else {
            old = age;
        }
    }
    0.5 * (young + old)
}

/// The age at which foam has faded under `FAINTEST_REFLECTANCE`.
static FADED_AGE_S: LazyLock<f64> = LazyLock::new(|| age_at_reflectance_s(FAINTEST_REFLECTANCE));

/// The active patch's area after `u` of its breaking, per unit of its band
/// times its crest: a crest of fixed length sweeping at its breaking speed,
/// so the area grows uniformly as Kleiss & Melville's (2010) Eq. 16 takes
/// it.
pub fn swept_area(u: f64) -> f64 {
    u.clamp(0.0, 1.0)
}

/// The first moment of that area about the patch's rear.
pub fn swept_moment(u: f64) -> f64 {
    0.5 * u.clamp(0.0, 1.0).powi(2)
}

/// When the foam a distance `along` of a band of `band` behind the patch's
/// rear leaves the roller and stays: once the roller's rear has passed it,
/// or when breaking ends after `period_s`.
fn release_s(along: f64, band: f64, period_s: f64) -> f64 {
    ((along + ROLLER_WAVELENGTHS / BREAKING_SPEED_RATIO * band) / band * period_s).min(period_s)
}

/// An event's foam integrated over its life, per unit of its swept band
/// times its crest length.
#[derive(Clone, Copy, Debug, Default)]
struct EventFoam {
    /// Area-time while the patch's mean reflectance reaches the threshold
    /// for white, which coverage counts.
    counted_area_s: f64,
    /// Integrals over the patch's area-time of the share its drawn filaments
    /// cover, and of that share times their reflectance, squared diffuse
    /// transmittance, and unscattered transmittance down and up through the
    /// layer at nadir.
    covered_s: f64,
    reflectance_s: f64,
    diffuse_transmittance_squared_s: f64,
    nadir_transmittance_s: f64,
}

/// Unscattered transmittance through scaled optical depth `depth` along
/// `air_masses` of path: the extinction depth is τ*/(1 - g) (Coakley and
/// Chýlek 1975).
fn unscattered_transmittance(depth: f64, air_masses: f64) -> f64 {
    (-depth / (1.0 - BUBBLE_ASYMMETRY) * air_masses).exp()
}

/// Integrates an event breaking for `period_s` over its life: a single
/// segment `crest_m` long, whose slices spread across its travel by
/// diffusion; or a row, which dilates about its centre as Koepke's a(t).
fn event_foam(period_s: f64, crest_m: Option<f64>) -> EventFoam {
    const SLICES: usize = 64;
    let faded_age_s = *FADED_AGE_S;
    let step_s = (period_s / 32.0).min(0.05);
    let depth = FRESH_OPTICAL_DEPTH;
    let mut foam = EventFoam::default();
    let mut counting = true;
    let mut time_s = 0.5 * step_s;
    while time_s < period_s + faded_age_s {
        let scale = if crest_m.is_some() {
            1.0
        } else {
            dilation(time_s, period_s).powi(2)
        };
        let swept = (time_s / period_s).min(1.0);
        let (mut area, mut means) = (0.0, [0.0; 4]);
        for slice in 0..SLICES {
            let along = (slice as f64 + 0.5) / SLICES as f64;
            if along > swept {
                break;
            }
            let age_s = (time_s - release_s(along, 1.0, period_s)).max(0.0);
            if age_s > faded_age_s {
                continue;
            }
            let covered = drawn_cover(age_s);
            // A spreading slice covers the same filaments over more area.
            area += crest_m.map_or(1.0, |crest_m| {
                (1.0 + 2.0 * LATERAL_DIFFUSIVITY_M2_S * age_s / (crest_m * crest_m)).sqrt()
            }) / SLICES as f64;
            for (sum, value) in means.iter_mut().zip([
                covered,
                covered * layer_reflectance(depth),
                covered * (1.0 + depth).powi(-2),
                covered * unscattered_transmittance(depth, 2.0),
            ]) {
                *sum += value / SLICES as f64;
            }
        }
        if area > 0.0 && means[1] / area < WHITE_THRESHOLD {
            counting = false;
        }
        if counting {
            foam.counted_area_s += scale * area * step_s;
        }
        foam.covered_s += scale * means[0] * step_s;
        foam.reflectance_s += scale * means[1] * step_s;
        foam.diffuse_transmittance_squared_s += scale * means[2] * step_s;
        foam.nadir_transmittance_s += scale * means[3] * step_s;
        time_s += step_s;
    }
    foam
}

/// A breaking segment's length at standard normal `z`.
fn segment_length_m(z: f64) -> f64 {
    (SEGMENT_MEDIAN_M * (SEGMENT_LOG_DEVIATION * z).exp()).min(LONGEST_SEGMENT_M)
}

/// Gauss–Hermite points and weights of the standard normal, over which the
/// classes of single segments are tabled.
const SEGMENT_POINTS: [(f64, f64); 5] = [
    (-2.856_970_013_872_806, 0.011_257_411_327_720_7),
    (-1.355_626_179_974_266, 0.222_075_922_005_612_6),
    (0.0, 0.533_333_333_333_333_3),
    (1.355_626_179_974_266, 0.222_075_922_005_612_6),
    (2.856_970_013_872_806, 0.011_257_411_327_720_7),
];

/// The mean length of a breaking segment.
fn mean_segment_m() -> f64 {
    NORMAL_POINTS
        .iter()
        .map(|&(z, weight)| weight * segment_length_m(z))
        .sum()
}

/// E[F] of a row's length factor, cut at `LONGEST_ROW_WAVELENGTHS`.
fn mean_row_factor() -> f64 {
    NORMAL_POINTS
        .iter()
        .map(|&(z, weight)| weight * (ROW_LOG_DEVIATION * z).exp().min(LONGEST_ROW_WAVELENGTHS))
        .sum()
}

/// How often a row's cell holds no, one or two segments.
const SEGMENTS_PER_CELL: [f64; 3] = [0.25, 0.5, 0.25];

/// The share of a row its segments cover, overlaps counted once: a point is
/// bare where no segment of its own cell or the three before it reaches
/// it.
static ROW_FILL: LazyLock<f64> = LazyLock::new(|| {
    const STEPS: usize = 64;
    let longer_than = |length_m: f64| -> f64 {
        NORMAL_POINTS
            .iter()
            .filter(|&&(z, _)| segment_length_m(z) > length_m)
            .map(|(_, weight)| weight)
            .sum()
    };
    let mut bare = 0.0;
    // Places on the steps' edges and starts at their midpoints, so no start
    // falls on the place itself, which would count it reached with all of
    // its step's weight.
    for point in 0..STEPS {
        let place_m = point as f64 / STEPS as f64 * SEGMENT_SPACING_M;
        let mut missed = 1.0;
        for back in 0..4 {
            let reached: f64 = (0..STEPS)
                .map(|start| {
                    let start_m = (start as f64 + 0.5) / STEPS as f64 * SEGMENT_SPACING_M;
                    let distance_m = place_m + back as f64 * SEGMENT_SPACING_M - start_m;
                    if distance_m < 0.0 {
                        0.0
                    } else {
                        longer_than(distance_m)
                    }
                })
                .sum::<f64>()
                / STEPS as f64;
            missed *= SEGMENTS_PER_CELL
                .iter()
                .enumerate()
                .map(|(count, share)| share * (1.0 - reached).powi(count as i32))
                .sum::<f64>();
        }
        bare += missed / STEPS as f64;
    }
    1.0 - bare
});

/// The share of a row its breaking segments cover.
pub fn foam_row_fill() -> f64 {
    *ROW_FILL
}

/// The breaking length an event of a class on a wave of `wavelength_m`
/// carries on average: one segment, or a row's filled share.
fn mean_breaking_length_m(bin: usize, wavelength_m: f64) -> f64 {
    if bin < FIRST_ROW_BIN {
        mean_segment_m()
    } else {
        foam_row_fill() * mean_row_factor() * wavelength_m
    }
}

/// Points of a class's tables, evenly in the logarithm of breaking speed
/// over its octave. Keep synchronized with `FOAM_SHAPE_POINTS` in
/// foam-state.wgsl.
pub(crate) const SHAPE_POINTS: usize = 9;

/// An event class's constants. Its events' breaking speeds spread evenly in
/// their logarithm from the class's slowest up to the fastest the wind
/// breaks, so its means are tabled against that top speed: per event, the
/// breaking length times breaking duration, the counted coverage
/// area-time, the covered area-time, and that area-time weighted by the
/// filaments' reflectance, squared diffuse transmittance, and unscattered
/// transmittance down and up through the layer at nadir; each integrated
/// over the octave's base-2 logarithm of speed from its slowest to each
/// point.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BinShape {
    pub(crate) speeds_m_s: [f64; 2],
    pub(crate) cumulative: [[f64; 6]; SHAPE_POINTS],
    /// The farthest any of its foam reaches from where it broke.
    pub(crate) reach_m: f64,
    /// Its longest life: breaking plus fading at the slowest decay.
    pub(crate) longest_life_s: f64,
}

/// A class's means over its events up to a top speed: breaking length times
/// duration and counted coverage area-time per event, covered area-time
/// per counted area-time, and the covered foam's mean reflectance.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClassMeans {
    pub(crate) crest_time_m_s: f64,
    pub(crate) counted_area_m2_s: f64,
    pub(crate) area_ratio: f64,
    pub(crate) reflectance: f64,
}

impl BinShape {
    /// The means over events breaking from the class's slowest speed up to
    /// `top_m_s`, if that reaches above it.
    pub(crate) fn means(&self, top_m_s: f64) -> Option<ClassMeans> {
        let octaves = (top_m_s.min(self.speeds_m_s[1]) / self.speeds_m_s[0]).log2();
        if octaves <= 1.0e-6 {
            return None;
        }
        let position = octaves * (SHAPE_POINTS - 1) as f64;
        let index = (position as usize).min(SHAPE_POINTS - 2);
        let fraction = position - index as f64;
        let sums: [f64; 6] = std::array::from_fn(|sum| {
            let [lower, upper] = [self.cumulative[index][sum], self.cumulative[index + 1][sum]];
            (lower + fraction * (upper - lower)) / octaves
        });
        Some(ClassMeans {
            crest_time_m_s: sums[0],
            counted_area_m2_s: sums[1],
            area_ratio: sums[2] / sums[1],
            reflectance: sums[3] / sums[2],
        })
    }
}

fn bin_shape(bin: usize) -> BinShape {
    // Midpoints in each table step's logarithm of speed.
    const STEP_SAMPLES: usize = 2;
    let speeds_m_s = bin_speeds_m_s(bin);
    let steps = SHAPE_POINTS - 1;
    let mut cumulative = [[0.0; 6]; SHAPE_POINTS];
    for step in 0..steps {
        let mut sums = [0.0; 6];
        for sample in 0..STEP_SAMPLES {
            let octaves =
                (step as f64 + (sample as f64 + 0.5) / STEP_SAMPLES as f64) / steps as f64;
            let breaker = Breaker::new(speeds_m_s[0] * 2f64.powf(octaves));
            let crest_m = mean_breaking_length_m(bin, breaker.wavelength_m);
            // Rows at their mean length; single segments over theirs, as
            // their spreading depends on it.
            let lengths: Vec<(f64, f64)> = if bin < FIRST_ROW_BIN {
                SEGMENT_POINTS
                    .iter()
                    .map(|&(z, weight)| (segment_length_m(z), weight))
                    .collect()
            } else {
                vec![(crest_m, 1.0)]
            };
            let mut values = [crest_m * breaker.period_s, 0.0, 0.0, 0.0, 0.0, 0.0];
            for (length_m, weight) in lengths {
                let event = event_foam(breaker.period_s, (bin < FIRST_ROW_BIN).then_some(length_m));
                let unit = weight * length_m * breaker.band_m();
                for (value, integral) in values[1..].iter_mut().zip([
                    event.counted_area_s,
                    event.covered_s,
                    event.reflectance_s,
                    event.diffuse_transmittance_squared_s,
                    event.nadir_transmittance_s,
                ]) {
                    *value += unit * integral;
                }
            }
            for (sum, value) in sums.iter_mut().zip(values) {
                *sum += value / (STEP_SAMPLES * steps) as f64;
            }
        }
        cumulative[step + 1] = std::array::from_fn(|sum| cumulative[step][sum] + sums[sum]);
    }
    // The fastest breaker of the class with its longest crest, its
    // rectangle dilated fully about its centre.
    let fastest = Breaker::new(speeds_m_s[1]);
    let longest_crest_m = if bin < FIRST_ROW_BIN {
        LONGEST_SEGMENT_M
    } else {
        LONGEST_ROW_WAVELENGTHS * fastest.wavelength_m
    };
    let longest_life_s = fastest.period_s + SLOWEST_DECAY_RATIO * *FADED_AGE_S;
    // A segment's trail spreads by diffusion and its warped edges move by up
    // to `WARP_REACH` of the band and the crest; a row dilates. As
    // foam-generate.wgsl draws them.
    let (spread, widest_m, warp) = if bin < FIRST_ROW_BIN {
        (
            1.0,
            (longest_crest_m * longest_crest_m + 2.0 * LATERAL_DIFFUSIVITY_M2_S * longest_life_s)
                .sqrt(),
            WARP_REACH,
        )
    } else {
        (
            dilation(fastest.period_s + *FADED_AGE_S, fastest.period_s),
            longest_crest_m,
            0.0,
        )
    };
    BinShape {
        speeds_m_s,
        cumulative,
        reach_m: 0.5 * fastest.band_m()
            + spread
                * ((0.5 + warp) * fastest.band_m()).hypot(0.5 * widest_m + warp * longest_crest_m),
        longest_life_s,
    }
}

pub(crate) static BIN_SHAPES: LazyLock<[BinShape; FOAM_BINS]> =
    LazyLock::new(|| std::array::from_fn(bin_shape));

/// The fastest breaking speed under a sustained wind: Kleiss and Melville's
/// Rayleigh width, cut at `BREAKING_SPEED_CUT_WIDTHS` of it.
pub(crate) fn fastest_breaking_speed_m_s(wind_m_s: f64) -> f64 {
    BREAKING_SPEED_CUT_WIDTHS
        * BREAKING_SPEED_SPREAD_PER_FRICTION_VELOCITY
        * friction_velocity_m_s(wind_m_s)
}

/// Each class's breaking-crest length per area, relative to the others,
/// under a sustained wind: Kleiss and Melville's Rayleigh Λ(c), with x₀ = 0
/// as speeds are taken relative to the water, cut at the fastest speed.
fn breaking_crest_shares(wind_m_s: f64) -> [f64; FOAM_BINS] {
    let fastest_m_s = fastest_breaking_speed_m_s(wind_m_s);
    let spread_m_s = fastest_m_s / BREAKING_SPEED_CUT_WIDTHS;
    let below =
        |speed_m_s: f64| 1.0 - (-0.5 * (speed_m_s.min(fastest_m_s) / spread_m_s).powi(2)).exp();
    std::array::from_fn(|bin| {
        if spread_m_s <= 0.0 {
            return 0.0;
        }
        let [slow, fast] = bin_speeds_m_s(bin);
        below(fast) - below(slow)
    })
}

/// Each class's means under a sustained wind, over the speeds it breaks.
fn class_means(wind_m_s: f64) -> [Option<ClassMeans>; FOAM_BINS] {
    let fastest_m_s = fastest_breaking_speed_m_s(wind_m_s);
    std::array::from_fn(|bin| BIN_SHAPES[bin].means(fastest_m_s))
}

/// Each class's event rate per area and time under a sustained wind: its
/// share of breaking-crest length over each event's crest length and
/// duration, as Phillips (1985) relates them, normalized so the events'
/// counted area-time is the whitecap coverage.
pub(crate) fn event_rates_per_m2_s(wind_m_s: f64) -> [f64; FOAM_BINS] {
    let crests = breaking_crest_shares(wind_m_s);
    let means = class_means(wind_m_s);
    let relative: [f64; FOAM_BINS] = std::array::from_fn(|bin| {
        means[bin].map_or(0.0, |means| crests[bin] / means.crest_time_m_s)
    });
    let counted: f64 = (0..FOAM_BINS)
        .map(|bin| means[bin].map_or(0.0, |means| relative[bin] * means.counted_area_m2_s))
        .sum();
    if counted <= 0.0 {
        return [0.0; FOAM_BINS];
    }
    let coverage = whitecap_coverage(wind_m_s);
    relative.map(|rate| coverage * rate / counted)
}

/// The mean reflectance the whitecaps of each class add to the sea under a
/// sustained wind, over all their foam, while `drawn` of each is drawn:
/// Σ W fⱼ Aⱼ R̄ⱼ, each class's share of the coverage, its foam's area per
/// counted coverage, and its foam's mean reflectance.
pub fn foam_mean_reflectance(wind_m_s: f64, drawn: [f64; FOAM_BINS]) -> f64 {
    let means = class_means(wind_m_s);
    event_rates_per_m2_s(wind_m_s)
        .iter()
        .zip(means)
        .zip(drawn)
        .map(|((rate, means), drawn)| {
            means.map_or(0.0, |means| {
                drawn * rate * means.counted_area_m2_s * means.area_ratio * means.reflectance
            })
        })
        .sum()
}

/// How far an event may drift from its cell with the water's surface drift
/// beyond the view centre's, which carries the whole lattice. Each is the
/// 95th percentile of that drift over its class's longest life, between
/// points within the class's widest discrete view (half a 1920×1080 view's
/// diagonal of footprints of 0.85 times its central wavelength), sampled
/// over the default world's water through a year; the 99th percentiles are
/// about half as large again.
/// Compromise: drift beyond it is clamped, so where currents shear most,
/// foam at the edge of wide views lags its local water.
const DRIFT_MARGIN_M: [f64; FOAM_BINS] = [11.5, 14.8, 17.3, 21.6, 28.6];

/// A class's lattice: square cells, a whole number of them around the world,
/// each wide enough to hold the reach of its events and their drift both
/// ways, and slots long enough for its slowest-fading events, so a point
/// sees only the events of the cells around it in this slot and the last.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BinLattice {
    pub(crate) cell_m: f64,
    pub(crate) cells_around: i64,
    pub(crate) slot_s: f64,
}

pub(crate) fn bin_lattice(bin: usize, world_width_m: f64) -> BinLattice {
    let shape = BIN_SHAPES[bin];
    let least_m = 2.0 * (shape.reach_m + DRIFT_MARGIN_M[bin]);
    let cells_around = ((world_width_m / least_m).floor() as i64).max(1);
    BinLattice {
        cell_m: world_width_m / cells_around as f64,
        cells_around,
        slot_s: shape.longest_life_s,
    }
}

/// Where a view sits on a class's lattice: the cell below its centre's
/// place in the water, which rides the view centre's orbital displacement
/// and integrated surface drift, its place in that cell, and the slot and
/// seconds into it at `world_time_s`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LatticeView {
    pub(crate) cell: [i64; 2],
    pub(crate) place: [f64; 2],
    pub(crate) slot: i64,
    pub(crate) slot_time_s: f64,
}

pub(crate) fn lattice_view(
    lattice: BinLattice,
    camera_m: [f64; 2],
    water_offset_m: [f64; 2],
    world_time_s: f64,
) -> LatticeView {
    let position = [0, 1].map(|axis| (camera_m[axis] - water_offset_m[axis]) / lattice.cell_m);
    let floor = position.map(f64::floor);
    let slot = (world_time_s / lattice.slot_s).floor();
    LatticeView {
        cell: [
            (floor[0] as i64).rem_euclid(lattice.cells_around),
            floor[1] as i64,
        ],
        place: [position[0] - floor[0], position[1] - floor[1]],
        slot: slot as i64,
        slot_time_s: world_time_s - slot * lattice.slot_s,
    }
}

/// The surface water's drift under a sustained wind at a latitude, beside
/// the ocean current: the whole wind sea's surface Stokes drift, which for
/// Pierson and Moskowitz's spectrum is 2αg Γ(5/4) (4/5)^¼ / ω_p, about
/// 1.6% of the wind at 19.5 m, along the wind; and Ardhuin et al.'s (2009,
/// J. Phys. Oceanogr. 39) Eulerian wind drift of 0.4–0.8% of the wind,
/// 45–70° to its right in the northern hemisphere. Compromises: the Stokes
/// drift leaves out the waves' spreading, which shortens it by a few
/// percent; and the wind drift turns as the game's boundary layer turns,
/// atan(f / r) for Coriolis parameter f and friction rate r, which runs
/// continuously through the equator, scaled to 55° at 45° of latitude.
pub fn surface_drift_m_s(wind_m_s: [f64; 2], latitude_degrees: f64) -> [f64; 2] {
    const PIERSON_MOSKOWITZ_ALPHA: f64 = 0.0081;
    // Γ(5/4) (4/5)^¼.
    const SPECTRAL_INTEGRAL: f64 = 0.857_2;
    const WIND_DRIFT_SHARE: f64 = 0.006;
    const MIDLATITUDE_TURN_RAD: f64 = 55.0 * PI / 180.0;
    // Keep synchronized with `BOUNDARY_LAYER_FRICTION_VELOCITY_M_S` over
    // `BOUNDARY_LAYER_DEPTH_M` and `EARTH_ROTATION_RATE_RAD_S` in
    // crates/game/src/lib.rs.
    const FRICTION_RATE_PER_S: f64 = 0.018 / 1000.0;
    const ROTATION_RATE_RAD_S: f64 = 7.292e-5;
    let speed = wind_m_s[0].hypot(wind_m_s[1]);
    if speed <= 0.0 {
        return [0.0; 2];
    }
    let direction = wind_m_s.map(|component| component / speed);
    let peak_frequency = (GRAVITY_M_S2 * game::wind_sea(speed).peak_wavenumber_per_m).sqrt();
    let stokes = 2.0 * PIERSON_MOSKOWITZ_ALPHA * GRAVITY_M_S2 * SPECTRAL_INTEGRAL / peak_frequency;
    let turn = |latitude: f64| {
        (2.0 * ROTATION_RATE_RAD_S * latitude.to_radians().sin()).atan2(FRICTION_RATE_PER_S)
    };
    let angle = -MIDLATITUDE_TURN_RAD * turn(latitude_degrees) / turn(45.0);
    let (sine, cosine) = angle.sin_cos();
    let drift = WIND_DRIFT_SHARE * speed;
    [
        stokes * direction[0] + drift * (cosine * direction[0] - sine * direction[1]),
        stokes * direction[1] + drift * (sine * direction[0] + cosine * direction[1]),
    ]
}

/// Foam stands about this thick when fresh, within Reul and Chapron's
/// (2003, J. Geophys. Res. 108) 1–4 cm, in proportion to its optical depth.
/// Keep synchronized with `FOAM_FRESH_THICKNESS_M` in foam-generate.wgsl.
const FRESH_THICKNESS_M: f64 = 0.02;
/// The hole noise's finest octave; keep synchronized with
/// `FOAM_FINEST_LACE_M`.
const FINEST_LACE_M: f64 = 0.02;
/// The short shadows reach no further than this many texels; at grazing
/// light the waves' own shadowing, which the glint leaves out, dominates.
const SHADOW_REACH_TEXELS: f64 = 4.0;
/// Below this many expected events in view over a life, the frame draws no
/// foam.
const SKIPPED_EVENTS: f64 = 1.0e-3;
/// Texels of the forcing grid across and down, and the cells a class may
/// look up across and down.
const FORCING_TEXELS: u32 = 64;
const CELL_TEXELS: u32 = 64;
// Keep synchronized with `FOAM_TILE_TEXELS` in foam-state.wgsl.
const TILE_TEXELS: u32 = 32;
/// The live events a frame lists, beyond which it drops them.
const RECORDS: u32 = 1 << 17;
/// Bytes of a live event's record, `FoamEvent` in foam-generate.wgsl, and of
/// its reach.
const RECORD_BYTES: u32 = 40;
const REACH_BYTES: u32 = 16;

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct FoamClassUniform {
    lattice: [i32; 4],
    place: [f32; 4],
    window: [i32; 4],
    timing: [f32; 4],
    table: [[f32; 4]; SHAPE_ROWS],
}

/// Rows of four holding a class's table, `BinShape::cumulative` point by
/// point.
const SHAPE_ROWS: usize = (6 * SHAPE_POINTS).div_ceil(4);

/// The foam's per-frame state, which its passes and the scene read; as
/// `FoamUniforms` in foam-state.wgsl.
#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
pub struct FoamUniforms {
    classes: [FoamClassUniform; FOAM_BINS],
    grid: [f32; 4],
    grid_texels: [u32; 4],
    forcing: [f32; 4],
    water: [f32; 4],
    sun: [f32; 4],
    moon: [f32; 4],
    flags: [u32; 4],
    lace: [f32; 4],
}

impl FoamUniforms {
    /// Whether the frame draws foam, and so runs its passes.
    pub fn active(&self) -> bool {
        self.flags[0] != 0
    }

    /// The share of each class drawn as discrete events.
    pub fn drawn(&self) -> [f64; FOAM_BINS] {
        self.classes.map(|class| f64::from(class.place[3]))
    }

    /// The foam grid's texels across and down this frame.
    pub fn grid_texels(&self) -> [u32; 2] {
        [self.grid_texels[0], self.grid_texels[1]]
    }

    /// Each class's slowest breaking speed, its cell's width, and the view
    /// centre's place in its cell, in cells.
    pub fn class_cells(&self) -> [(f32, f32, [f32; 2]); FOAM_BINS] {
        self.classes.map(|class| {
            (
                class.timing[3],
                class.place[2],
                [class.place[0], class.place[1]],
            )
        })
    }

    /// The foam grid's and the forcing grid's corners from the view centre,
    /// and their texels' sizes.
    pub fn grid_and_forcing_m(&self) -> [([f32; 2], f32); 2] {
        [
            ([self.grid[0], self.grid[1]], self.grid[2]),
            ([self.forcing[0], self.forcing[1]], self.forcing[2]),
        ]
    }

    /// Workgroups of the forcing, cell, and event and shadow passes.
    pub fn workgroups(&self) -> [[u32; 3]; 3] {
        let cells = self.classes.iter().fold([0, 0], |[x, y], class| {
            [x.max(class.window[2] as u32), y.max(class.window[3] as u32)]
        });
        [
            [FORCING_TEXELS / 8, FORCING_TEXELS / 8, 1],
            [cells[0], cells[1], 2 * FOAM_BINS as u32],
            [self.grid_texels[2], self.grid_texels[3], 1],
        ]
    }
}

/// Texels across the forcing grid; the cells' texture width and height, and
/// layers; the bytes of the frame's list of live events and of their
/// reaches; then the bytes of `FoamUniforms`.
pub fn foam_texture_sizes() -> [u32; 6] {
    [
        FORCING_TEXELS,
        CELL_TEXELS,
        2 * FOAM_BINS as u32,
        RECORDS * RECORD_BYTES,
        RECORDS * REACH_BYTES,
        size_of::<FoamUniforms>() as u32,
    ]
}

/// The foam grid's texels across and down for a scene of `scene_size`
/// pixels with `pixels_per_texel`: the view and a texel's margin each side,
/// as the grid snaps to the world.
pub fn foam_grid_texels(scene_size: (u32, u32), pixels_per_texel: u32) -> [u32; 2] {
    [scene_size.0, scene_size.1].map(|pixels| pixels.div_ceil(pixels_per_texel) + 3)
}

/// What a frame's foam depends on beyond the world and its clock.
#[derive(Clone, Copy, Debug)]
pub struct FoamView {
    pub camera_m: [f32; 2],
    pub view_m: [f32; 2],
    /// The scene's sea-level pixel footprint and pixels per foam texel.
    pub pixel_footprint_m: f32,
    pub pixels_per_texel: u32,
    /// The grid's texels across and down, as allocated.
    pub grid_capacity: [u32; 2],
    /// The water's orbital displacement at the view centre, and the view
    /// centre's integrated surface drift and its present velocity.
    pub orbital_displacement_m: [f64; 2],
    pub drift_m: [f64; 2],
    pub drift_velocity_m_s: [f64; 2],
    /// The Sun's and Moon's directions at the view centre.
    pub light_directions: [[f32; 3]; 2],
    /// The strongest sustained wind sampled over the view.
    pub strongest_wind_m_s: f64,
    /// Evaluates every class and the shadows at the forced wind, for timing.
    pub forced: bool,
}

/// The foam's state for a frame at the world clock.
pub fn foam_uniforms(
    world_geometry: WorldGeometry,
    world_tick: u64,
    world_tick_fraction: f32,
    view: FoamView,
) -> FoamUniforms {
    let width_m = f64::from(world_geometry.width_m() as f32);
    let world_time_s =
        (world_tick as f64 + f64::from(world_tick_fraction)) / TICKS_PER_SECOND as f64;
    let camera_m = view.camera_m.map(f64::from);
    let view_m = view.view_m.map(f64::from);
    let texel_m = f64::from(view.pixel_footprint_m) * f64::from(view.pixels_per_texel);
    // A grid snapped to the world, a texel beyond the view each side.
    let origin_m = [0, 1]
        .map(|axis| ((camera_m[axis] - 0.5 * view_m[axis]) / texel_m).floor() * texel_m - texel_m);
    let texels = [0, 1]
        .map(|axis| (((view_m[axis] / texel_m).ceil() as u32) + 3).min(view.grid_capacity[axis]));
    let grid_minimum = [0, 1].map(|axis| origin_m[axis] - camera_m[axis]);
    let grid_maximum = [0, 1].map(|axis| grid_minimum[axis] + f64::from(texels[axis]) * texel_m);
    let water_offset_m = [0, 1].map(|axis| view.orbital_displacement_m[axis] + view.drift_m[axis]);
    let mut forcing_minimum = grid_minimum;
    let mut forcing_maximum = grid_maximum;
    let classes = std::array::from_fn(|bin| {
        let shape = BIN_SHAPES[bin];
        let lattice = bin_lattice(bin, width_m);
        let place = lattice_view(lattice, camera_m, water_offset_m, world_time_s);
        let cell = |offset: f64, axis: usize| {
            (place.place[axis] + offset / lattice.cell_m - 0.5).floor() as i64
        };
        let first = [cell(grid_minimum[0], 0), cell(grid_minimum[1], 1)];
        let across = [0, 1]
            .map(|axis| cell([grid_maximum[0], grid_maximum[1]][axis], axis) + 2 - first[axis]);
        // A class draws discretely while its band spans two pixels and fades
        // over one octave of footprint into its unresolved mean, and while
        // its cells fit their texture. Pixels rather than the grid's texels
        // set it, so every tier draws the same events.
        let class_breaker = Breaker::new(shape.speeds_m_s[0] * std::f64::consts::SQRT_2);
        let resolved = (0.85 * class_breaker.wavelength_m / f64::from(view.pixel_footprint_m))
            .log2()
            .clamp(0.0, 1.0);
        let fitting =
            (2.0 - across[0].max(across[1]) as f64 / f64::from(CELL_TEXELS / 2)).clamp(0.0, 1.0);
        let drawn = resolved.min(fitting);
        if drawn > 0.0 || view.forced {
            for axis in 0..2 {
                forcing_minimum[axis] = forcing_minimum[axis]
                    .min((first[axis] as f64 - place.place[axis]) * lattice.cell_m);
                forcing_maximum[axis] = forcing_maximum[axis].max(
                    ((first[axis] + across[axis]) as f64 - place.place[axis]) * lattice.cell_m,
                );
            }
        }
        FoamClassUniform {
            lattice: [
                place.cell[0] as i32,
                place.cell[1] as i32,
                lattice.cells_around as i32,
                place.slot as i32,
            ],
            place: [
                place.place[0] as f32,
                place.place[1] as f32,
                lattice.cell_m as f32,
                drawn as f32,
            ],
            window: [
                first[0] as i32,
                first[1] as i32,
                across[0].min(i64::from(CELL_TEXELS)) as i32,
                across[1].min(i64::from(CELL_TEXELS)) as i32,
            ],
            timing: [
                place.slot_time_s as f32,
                lattice.slot_s as f32,
                DRIFT_MARGIN_M[bin] as f32,
                shape.speeds_m_s[0] as f32,
            ],
            table: std::array::from_fn(|row| {
                std::array::from_fn(|column| {
                    let index = 4 * row + column;
                    shape
                        .cumulative
                        .get(index / 6)
                        .map_or(0.0, |point| point[index % 6] as f32)
                })
            }),
        }
    });
    let forcing_side_m = (0..2)
        .map(|axis| forcing_maximum[axis] - forcing_minimum[axis])
        .fold(0.0, f64::max)
        * (1.0 + 2.0 / f64::from(FORCING_TEXELS));
    let forcing_centre = [0, 1].map(|axis| 0.5 * (forcing_minimum[axis] + forcing_maximum[axis]));
    let light = |direction: [f32; 3]| {
        let [x, y, z] = direction.map(f64::from);
        let horizontal = x.hypot(y);
        let reach = if view.forced {
            SHADOW_REACH_TEXELS
        } else if z > 0.0 {
            (FRESH_THICKNESS_M * horizontal / z / texel_m).min(SHADOW_REACH_TEXELS)
        } else {
            0.0
        };
        [x as f32, y as f32, z as f32, reach as f32]
    };
    let view_area_m2 = view_m[0] * view_m[1];
    let expected_events: f64 = event_rates_per_m2_s(view.strongest_wind_m_s)
        .iter()
        .zip(BIN_SHAPES.iter())
        .map(|(rate, shape)| rate * view_area_m2 * shape.longest_life_s)
        .sum();
    let active = view.forced || expected_events >= SKIPPED_EVENTS;
    let unresolved_octaves = ((2.0 * texel_m / FINEST_LACE_M).log2() + 1.0).max(1.0);
    FoamUniforms {
        classes,
        grid: [
            grid_minimum[0] as f32,
            grid_minimum[1] as f32,
            texel_m as f32,
            *FADED_AGE_S as f32,
        ],
        grid_texels: [
            texels[0],
            texels[1],
            texels[0].div_ceil(TILE_TEXELS),
            texels[1].div_ceil(TILE_TEXELS),
        ],
        forcing: [
            (forcing_centre[0] - 0.5 * forcing_side_m) as f32,
            (forcing_centre[1] - 0.5 * forcing_side_m) as f32,
            (forcing_side_m / f64::from(FORCING_TEXELS)) as f32,
            FORCING_TEXELS as f32,
        ],
        water: [
            view.orbital_displacement_m[0] as f32,
            view.orbital_displacement_m[1] as f32,
            view.drift_velocity_m_s[0] as f32,
            view.drift_velocity_m_s[1] as f32,
        ],
        sun: light(view.light_directions[0]),
        moon: light(view.light_directions[1]),
        flags: [
            u32::from(active),
            u32::from(view.forced),
            view.pixels_per_texel,
            0,
        ],
        lace: [
            view_m[0].max(view_m[1]) as f32,
            (FRESH_THICKNESS_M / FRESH_OPTICAL_DEPTH * TAU / FINEST_LACE_M * (4.0f64 / 3.0).sqrt()
                / unresolved_octaves.sqrt()) as f32,
            RECORDS as f32,
            foam_row_fill() as f32,
        ],
    }
}

/// The view centre's surface drift accumulated over the frames, which
/// carries the whitecap lattice as the view centre's wind carries the
/// clouds. Its origin is arbitrary, so it needs no warm-up.
#[derive(Clone, Copy, Debug, Default)]
pub struct FoamDrift {
    pub offset_m: [f64; 2],
}

impl FoamDrift {
    pub fn advance(&mut self, elapsed_seconds: f64, velocity_m_s: [f64; 2]) {
        let elapsed_seconds = elapsed_seconds.max(0.0);
        for (offset, velocity) in self.offset_m.iter_mut().zip(velocity_m_s) {
            *offset += elapsed_seconds * velocity;
        }
    }
}

/// The surface water's drift at the view centre: the wind sea's and the
/// wind's, as `surface_drift_m_s` gives them, and the ocean current.
pub fn view_surface_drift_m_s(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    camera_m: [f32; 2],
) -> [f64; 2] {
    let environment = game::procedural_world_environment_interpolated(
        world_geometry,
        match_seed,
        world_tick,
        world_tick_fraction,
        rapier2d_f64::glamx::Vec2::new(camera_m[0], camera_m[1]),
    );
    let wind = environment.wave_forcing.surface_wind_velocity_m_s;
    let drift = surface_drift_m_s(
        [f64::from(wind.x), f64::from(wind.y)],
        f64::from(environment.celestial.geographic_latitude_degrees),
    );
    let current = environment.ocean.current_velocity_m_s;
    [
        drift[0] + f64::from(current.x),
        drift[1] + f64::from(current.y),
    ]
}

/// The strongest sustained wind among the view centre and corners.
pub fn view_strongest_wind_m_s(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    camera_m: [f32; 2],
    view_m: [f32; 2],
) -> f64 {
    [
        (0.0, 0.0),
        (-0.5, -0.5),
        (0.5, -0.5),
        (-0.5, 0.5),
        (0.5, 0.5),
    ]
    .into_iter()
    .map(|(x, y)| {
        let position = rapier2d_f64::glamx::Vec2::new(
            camera_m[0] + x * view_m[0],
            camera_m[1] + y * view_m[1],
        );
        f64::from(
            game::procedural_unsheltered_wind_velocity_m_s(
                world_geometry,
                match_seed,
                world_tick,
                position,
            )
            .length(),
        )
    })
    .fold(0.0, f64::max)
}

fn storage_entry(
    binding: u32,
    format: wgpu::TextureFormat,
    dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format,
            view_dimension: dimension,
        },
        count: None,
    }
}

fn texture_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    sample_type: wgpu::TextureSampleType,
    dimension: wgpu::TextureViewDimension,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: dimension,
            multisampled: false,
        },
        count: None,
    }
}

fn buffer_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

const UNIFORM_ENTRY: wgpu::BindGroupLayoutEntry = wgpu::BindGroupLayoutEntry {
    binding: 0,
    visibility: wgpu::ShaderStages::COMPUTE,
    ty: wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
    },
    count: None,
};

struct GridTextures {
    grid: wgpu::TextureView,
    tiles: wgpu::TextureView,
    shadows: wgpu::TextureView,
    scene_tiles: wgpu::TextureView,
    textures: [wgpu::Texture; 2],
    capacity: [u32; 2],
}

/// The foam passes and their textures: a forcing grid of wind, drift and
/// sub-pixel foam; each class's cells, which point into the frame's list of
/// live events; the foam grid at
/// the scene's resolution or half of it, with its tiles; and the short
/// shadows. The scene reads them through `SCENE_LAYOUT_ENTRIES` at bindings
/// 14 to 19 of its first group.
pub struct Foam {
    uniforms: wgpu::Buffer,
    forcing_texture: wgpu::Texture,
    forcing: wgpu::TextureView,
    cells: wgpu::TextureView,
    records: wgpu::Buffer,
    record_count: wgpu::Buffer,
    reaches: wgpu::Buffer,
    sampler: wgpu::Sampler,
    grid: GridTextures,
    layouts: [wgpu::BindGroupLayout; 4],
    bind_groups: [wgpu::BindGroup; 4],
    pipelines: [wgpu::ComputePipeline; 4],
}

impl Foam {
    pub const SCENE_LAYOUT_ENTRIES: [wgpu::BindGroupLayoutEntry; 6] = [
        wgpu::BindGroupLayoutEntry {
            binding: 14,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 15,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Uint,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 16,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 17,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 18,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 19,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ];

    /// `scene_uniform_layout` holds the scene uniforms, which the passes read
    /// for the clock, the seed and the wave modes.
    pub fn new(device: &wgpu::Device, scene_uniform_layout: &wgpu::BindGroupLayout) -> Self {
        let texture = |label, size: [u32; 3], format, usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: size[2],
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let array_view = |texture: &wgpu::Texture| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            })
        };
        let usage = wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING;
        let forcing_texture = texture(
            "Foam forcing",
            [FORCING_TEXELS, FORCING_TEXELS, 2],
            wgpu::TextureFormat::Rgba16Float,
            usage | wgpu::TextureUsages::COPY_SRC,
        );
        let forcing = array_view(&forcing_texture);
        let cells = array_view(&texture(
            "Foam cells",
            [CELL_TEXELS, CELL_TEXELS, 2 * FOAM_BINS as u32],
            wgpu::TextureFormat::Rg32Uint,
            usage,
        ));
        let records = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Foam events"),
            size: u64::from(RECORDS * RECORD_BYTES),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let reaches = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Foam event reaches"),
            size: u64::from(RECORDS * REACH_BYTES),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let record_count = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Foam event count"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Foam uniforms"),
            size: size_of::<FoamUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Foam forcing sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let compute = wgpu::ShaderStages::COMPUTE;
        let float = wgpu::TextureSampleType::Float { filterable: false };
        let array = wgpu::TextureViewDimension::D2Array;
        let flat = wgpu::TextureViewDimension::D2;
        let layout = |label, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let layouts = [
            layout(
                "Foam forcing layout",
                &[
                    UNIFORM_ENTRY,
                    storage_entry(1, wgpu::TextureFormat::Rgba16Float, array),
                ],
            ),
            layout(
                "Foam cell layout",
                &[
                    UNIFORM_ENTRY,
                    texture_entry(
                        2,
                        compute,
                        wgpu::TextureSampleType::Float { filterable: true },
                        array,
                    ),
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: compute,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    storage_entry(4, wgpu::TextureFormat::Rg32Uint, array),
                    buffer_entry(5, false),
                    buffer_entry(6, false),
                    buffer_entry(15, false),
                ],
            ),
            layout(
                "Foam event layout",
                &[
                    UNIFORM_ENTRY,
                    texture_entry(7, compute, wgpu::TextureSampleType::Uint, array),
                    buffer_entry(14, true),
                    buffer_entry(16, true),
                    storage_entry(8, wgpu::TextureFormat::Rgba16Float, flat),
                    storage_entry(9, wgpu::TextureFormat::R32Uint, flat),
                ],
            ),
            layout(
                "Foam shadow layout",
                &[
                    UNIFORM_ENTRY,
                    texture_entry(10, compute, float, flat),
                    texture_entry(11, compute, wgpu::TextureSampleType::Uint, flat),
                    storage_entry(12, wgpu::TextureFormat::Rgba8Unorm, flat),
                    storage_entry(13, wgpu::TextureFormat::R32Uint, flat),
                ],
            ),
        ];
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Foam shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(FOAM_SHADER)),
        });
        let pipelines = [
            "generate_foam_forcing",
            "generate_foam_cells",
            "generate_foam_events",
            "generate_foam_shadows",
        ]
        .into_iter()
        .zip(&layouts)
        .map(|(entry_point, layout)| {
            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(entry_point),
                bind_group_layouts: &[Some(scene_uniform_layout), Some(layout)],
                immediate_size: 0,
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap_or_else(|_| unreachable!());
        let grid = Self::grid_textures(device, [1, 1]);
        let bind_groups = Self::bind_groups(
            device,
            &layouts,
            &uniforms,
            [&forcing, &cells],
            [&records, &record_count, &reaches],
            &sampler,
            &grid,
        );
        Self {
            uniforms,
            forcing_texture,
            forcing,
            cells,
            records,
            record_count,
            reaches,
            sampler,
            grid,
            layouts,
            bind_groups,
            pipelines,
        }
    }

    fn grid_textures(device: &wgpu::Device, capacity: [u32; 2]) -> GridTextures {
        let usage = wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC;
        let texture = |label, size: [u32; 2], format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let tiles = capacity.map(|texels| texels.div_ceil(TILE_TEXELS));
        let grid = texture("Foam grid", capacity, wgpu::TextureFormat::Rgba16Float);
        let scene_tiles = texture("Foam scene tiles", tiles, wgpu::TextureFormat::R32Uint);
        GridTextures {
            grid: grid.create_view(&Default::default()),
            tiles: texture("Foam tiles", tiles, wgpu::TextureFormat::R32Uint)
                .create_view(&Default::default()),
            shadows: texture("Foam shadows", capacity, wgpu::TextureFormat::Rgba8Unorm)
                .create_view(&Default::default()),
            scene_tiles: scene_tiles.create_view(&Default::default()),
            textures: [grid, scene_tiles],
            capacity,
        }
    }

    fn bind_groups(
        device: &wgpu::Device,
        layouts: &[wgpu::BindGroupLayout; 4],
        uniforms: &wgpu::Buffer,
        [forcing, cells]: [&wgpu::TextureView; 2],
        [records, record_count, reaches]: [&wgpu::Buffer; 3],
        sampler: &wgpu::Sampler,
        grid: &GridTextures,
    ) -> [wgpu::BindGroup; 4] {
        let view = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        let uniform = wgpu::BindGroupEntry {
            binding: 0,
            resource: uniforms.as_entire_binding(),
        };
        let entries: [Vec<wgpu::BindGroupEntry>; 4] = [
            vec![uniform.clone(), view(1, forcing)],
            vec![
                uniform.clone(),
                view(2, forcing),
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                view(4, cells),
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: records.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: record_count.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 15,
                    resource: reaches.as_entire_binding(),
                },
            ],
            vec![
                uniform.clone(),
                view(7, cells),
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: records.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 16,
                    resource: reaches.as_entire_binding(),
                },
                view(8, &grid.grid),
                view(9, &grid.tiles),
            ],
            vec![
                uniform,
                view(10, &grid.grid),
                view(11, &grid.tiles),
                view(12, &grid.shadows),
                view(13, &grid.scene_tiles),
            ],
        ];
        std::array::from_fn(|pass| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Foam pass"),
                layout: &layouts[pass],
                entries: &entries[pass],
            })
        })
    }

    /// Sizes the foam grid for a scene of `scene_size` pixels with
    /// `pixels_per_texel`, returning whether its textures were replaced, so
    /// the scene's bind group must be rebuilt.
    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        scene_size: (u32, u32),
        pixels_per_texel: u32,
    ) -> bool {
        let capacity = foam_grid_texels(scene_size, pixels_per_texel);
        if capacity == self.grid.capacity {
            return false;
        }
        self.grid = Self::grid_textures(device, capacity);
        self.bind_groups = Self::bind_groups(
            device,
            &self.layouts,
            &self.uniforms,
            [&self.forcing, &self.cells],
            [&self.records, &self.record_count, &self.reaches],
            &self.sampler,
            &self.grid,
        );
        true
    }

    /// The grid's texels across and down, as allocated.
    pub fn capacity(&self) -> [u32; 2] {
        self.grid.capacity
    }

    /// The foam grid, the tiles the scene reads it in, and the forcing grid,
    /// for readback.
    pub fn readback_textures(&self) -> [&wgpu::Texture; 3] {
        [
            &self.grid.textures[0],
            &self.grid.textures[1],
            &self.forcing_texture,
        ]
    }

    /// The frame's list of live events and its count, for readback.
    pub fn readback_events(&self) -> [&wgpu::Buffer; 2] {
        [&self.records, &self.record_count]
    }

    /// The scene's bindings of the foam's textures, sampler and state.
    pub fn scene_entries(&self) -> [wgpu::BindGroupEntry<'_>; 6] {
        let view = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        [
            view(14, &self.grid.grid),
            view(15, &self.grid.scene_tiles),
            view(16, &self.grid.shadows),
            view(17, &self.forcing),
            wgpu::BindGroupEntry {
                binding: 18,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 19,
                resource: self.uniforms.as_entire_binding(),
            },
        ]
    }

    /// Holds `uniforms` for the frame's next submission, and empties its
    /// list of live events.
    pub fn write(&self, queue: &wgpu::Queue, uniforms: &FoamUniforms) {
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(uniforms));
        queue.write_buffer(&self.record_count, 0, &[0; 4]);
    }

    /// Runs the passes `uniforms`, as written, draws.
    pub fn dispatch(
        &self,
        pass: &mut wgpu::ComputePass<'_>,
        scene_uniforms: &wgpu::BindGroup,
        uniforms: &FoamUniforms,
    ) {
        if !uniforms.active() {
            return;
        }
        let [forcing, cells, tiles] = uniforms.workgroups();
        pass.set_bind_group(0, scene_uniforms, &[]);
        for (index, [x, y, z]) in [forcing, cells, tiles, tiles].into_iter().enumerate() {
            if x * y * z == 0 {
                continue;
            }
            pass.set_pipeline(&self.pipelines[index]);
            pass.set_bind_group(1, &self.bind_groups[index], &[]);
            pass.dispatch_workgroups(x, y, z);
        }
    }
}

/// The reflectance an event leaves integrated over its area and life: a
/// single segment `crest_m` long, or a row that long at its filled share,
/// breaking at `speed_m_s`.
pub fn foam_event_reflectance_time(speed_m_s: f64, crest_m: f64, row: bool) -> f64 {
    let breaker = Breaker::new(speed_m_s);
    let event = event_foam(breaker.period_s, (!row).then_some(crest_m));
    let fill = if row { foam_row_fill() } else { 1.0 };
    fill * crest_m * breaker.band_m() * event.reflectance_s
}

/// The age at which foam has faded from sight.
pub fn foam_faded_age_s() -> f64 {
    *FADED_AGE_S
}

/// The share of the sea the events of each class cover under a sustained
/// wind, overlaps counted twice, while `drawn` of each is drawn.
pub fn foam_mean_cover(wind_m_s: f64, drawn: [f64; FOAM_BINS]) -> f64 {
    let means = class_means(wind_m_s);
    event_rates_per_m2_s(wind_m_s)
        .iter()
        .zip(means)
        .zip(drawn)
        .map(|((rate, means), drawn)| {
            means.map_or(0.0, |means| {
                drawn * rate * means.counted_area_m2_s * means.area_ratio
            })
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use simulation::WorldGeometry;

    /// Koepke's Table I effective reflectance of foam patches counted to ages
    /// of 1 to 10 s, through his integration in steps of 1 s, inverted into
    /// each second's patch-mean reflectance.
    fn koepke_patch_reflectance() -> [f64; 10] {
        const EFFECTIVE: [f64; 10] = [0.41, 0.35, 0.30, 0.28, 0.25, 0.23, 0.21, 0.19, 0.18, 0.16];
        let mut area = 0.0;
        let mut previous = 0.0;
        std::array::from_fn(|index| {
            let growth = area_growth(index as f64 + 1.0);
            area += growth;
            let product = EFFECTIVE[index] * area;
            let reflectance = (product - previous) / growth;
            previous = product;
            reflectance
        })
    }

    fn fit_error(depth: f64, efold_s: f64) -> f64 {
        koepke_patch_reflectance()
            .iter()
            .enumerate()
            .map(|(index, measured)| {
                let age_s = index as f64 + 1.0;
                (layer_reflectance(depth * (-age_s / efold_s).exp()) - measured).powi(2)
            })
            .sum()
    }

    /// The least-squares fresh depth and e-folding time, by coordinate
    /// search with shrinking steps.
    fn koepke_fit() -> (f64, f64) {
        let (mut depth, mut efold_s) = (0.85, 3.8);
        let mut step = 0.1;
        while step > 1e-5 {
            let mut improved = false;
            for (dd, de) in [
                (step, 0.0),
                (-step, 0.0),
                (0.0, 4.0 * step),
                (0.0, -4.0 * step),
            ] {
                if fit_error(depth + dd, efold_s + de) < fit_error(depth, efold_s) {
                    depth += dd;
                    efold_s += de;
                    improved = true;
                }
            }
            if !improved {
                step *= 0.5;
            }
        }
        (depth, efold_s)
    }

    /// The counted-coverage-weighted breaking period at 8 m/s, within the
    /// winds of Koepke's and Callaghan et al.'s whitecaps.
    fn representative_period_s() -> f64 {
        let rates = event_rates_per_m2_s(8.0);
        let fastest_m_s = fastest_breaking_speed_m_s(8.0);
        let (mut weight, mut period) = (0.0, 0.0);
        for (bin, rate) in rates.iter().enumerate() {
            let [slowest_m_s, top_m_s] = bin_speeds_m_s(bin);
            let top_m_s = top_m_s.min(fastest_m_s).max(slowest_m_s);
            for index in 0..16 {
                let breaker = Breaker::new(
                    slowest_m_s * (top_m_s / slowest_m_s).powf((index as f64 + 0.5) / 16.0),
                );
                let crest_m = mean_breaking_length_m(bin, breaker.wavelength_m);
                let counted = rate
                    * crest_m
                    * breaker.band_m()
                    * event_foam(breaker.period_s, (bin < FIRST_ROW_BIN).then_some(crest_m))
                        .counted_area_s;
                weight += counted;
                period += counted * breaker.period_s;
            }
        }
        period / weight
    }

    /// A patch's area at `time_s`, a crest `crest_m` long breaking for
    /// `period_s`, its slices spreading with `diffusivity_m2_s`.
    fn spread_area(period_s: f64, crest_m: f64, diffusivity_m2_s: f64, time_s: f64) -> f64 {
        const SLICES: usize = 200;
        (0..SLICES)
            .map(|slice| (slice as f64 + 0.5) / SLICES as f64)
            .filter(|&along| along <= time_s / period_s)
            .map(|along| {
                let age_s = (time_s - release_s(along, 1.0, period_s)).max(0.0);
                (crest_m * crest_m + 2.0 * diffusivity_m2_s * age_s).sqrt() / SLICES as f64
            })
            .sum()
    }

    /// The diffusivity whose representative patch grows most nearly as
    /// Koepke's a(t) over the eight seconds after breaking.
    fn diffusivity_fit(period_s: f64, crest_m: f64) -> f64 {
        let error = |diffusivity_m2_s: f64| -> f64 {
            let broken = spread_area(period_s, crest_m, diffusivity_m2_s, period_s);
            (0..=40)
                .map(|step| {
                    let time_s = period_s + 0.2 * step as f64;
                    (spread_area(period_s, crest_m, diffusivity_m2_s, time_s) / broken
                        - area_growth(time_s) / area_growth(period_s))
                    .powi(2)
                })
                .sum()
        };
        (1..400)
            .map(|tenth| f64::from(tenth) / 10.0)
            .min_by(|a, b| error(*a).total_cmp(&error(*b)))
            .unwrap()
    }

    // The fresh depth and its decay fit Koepke's inverted patches, and the
    // diffusivity spreads a representative patch as his a(t). A calibration,
    // not a check.
    #[test]
    fn calibration_fits_koepke() {
        let (depth, efold_s) = koepke_fit();
        assert!(
            (depth - FRESH_OPTICAL_DEPTH).abs() < 0.005,
            "fresh depth {depth}"
        );
        assert!(
            (efold_s - OPTICAL_DEPTH_EFOLD_S).abs() < 0.02,
            "e-folding {efold_s} s"
        );
        let diffusivity_m2_s = diffusivity_fit(representative_period_s(), mean_segment_m());
        assert!(
            (diffusivity_m2_s - LATERAL_DIFFUSIVITY_M2_S).abs() < 0.11,
            "diffusivity {diffusivity_m2_s} m²/s"
        );
    }

    // Koepke's effective reflectance of whitecaps over the life coverage
    // counts, ∫ a r dt / ∫ a dt, is 22 ± 8%. An implementation check, as the
    // fit came from his data.
    #[test]
    fn counted_whitecaps_reflect_as_koepke_found() {
        let counted_s = age_at_reflectance_s(WHITE_THRESHOLD);
        let steps = 10_000;
        let (mut reflected, mut area) = (0.0, 0.0);
        for step in 0..steps {
            let age_s = (step as f64 + 0.5) / steps as f64 * counted_s;
            reflected += area_growth(age_s) * patch_mean_reflectance(age_s);
            area += area_growth(age_s);
        }
        let effective = reflected / area;
        assert!((effective - 0.22).abs() <= 0.08, "{effective}");
    }

    // Fresh foam's patch-mean reflectance, which inherits Koepke's assumed
    // 55% peak, lies within 0.1 of Dierssen's (2019) independently measured
    // thick foam, about 0.5.
    #[test]
    fn fresh_foam_reflects_as_thick_foam_measured() {
        let fresh = patch_mean_reflectance(0.0);
        assert!((fresh - 0.5).abs() <= 0.1, "{fresh}");
    }

    // Event rates change continuously with the wind, across the classes'
    // edges and the distribution's cuts: a step ten times shorter changes
    // them about ten times less, where a jump would not shrink.
    #[test]
    fn event_rates_change_continuously_with_wind() {
        let largest_change = |step: f64| {
            let mut largest = [0.0f64; FOAM_BINS];
            let mut wind_m_s = 3.0;
            while wind_m_s < 40.0 {
                let [before, after] = [wind_m_s, wind_m_s + step].map(event_rates_per_m2_s);
                for bin in 0..FOAM_BINS {
                    largest[bin] = largest[bin].max((after[bin] - before[bin]).abs());
                }
                wind_m_s += 0.0037;
            }
            largest
        };
        let [coarse, fine] = [1e-2, 1e-3].map(largest_change);
        for bin in 0..FOAM_BINS {
            assert!(
                fine[bin] <= 0.2 * coarse[bin],
                "class {bin}: {} then {}",
                coarse[bin],
                fine[bin]
            );
        }
    }

    // A world point lies in the same cell of each class's lattice, at the
    // same place in it, whatever the view centre and across the world's
    // longitude seam: the lattice fits the world, so x and x plus its width
    // are one point.
    #[test]
    fn the_lattice_stays_on_the_water_across_views_and_the_seam() {
        let width_m = f64::from(WorldGeometry::default().width_m() as f32);
        let water_offset_m = [1234.5, -678.9];
        let point_m = [5.0, 2000.0];
        for bin in 0..FOAM_BINS {
            let lattice = bin_lattice(bin, width_m);
            assert!((lattice.cells_around as f64 * lattice.cell_m - width_m).abs() < 1e-6);
            let mut reference = None;
            for camera_m in [
                [5.0, 2000.0],
                [40.0, 1990.0],
                [width_m - 30.0, 2010.0],
                [width_m - 0.25, 1500.0],
            ] {
                let view = lattice_view(lattice, camera_m, water_offset_m, 1.0e6);
                let mut offset_m = [point_m[0] - camera_m[0], point_m[1] - camera_m[1]];
                offset_m[0] -= (offset_m[0] / width_m).round() * width_m;
                let position =
                    [0, 1].map(|axis| view.place[axis] + offset_m[axis] / lattice.cell_m);
                let relative = position.map(f64::floor);
                let cell = [
                    (view.cell[0] + relative[0] as i64).rem_euclid(lattice.cells_around),
                    view.cell[1] + relative[1] as i64,
                ];
                let place = [0, 1].map(|axis| position[axis] - relative[axis]);
                let Some((first_cell, first_place)) = reference else {
                    reference = Some((cell, place));
                    continue;
                };
                assert_eq!(cell, first_cell, "class {bin}, camera {camera_m:?}");
                for axis in 0..2 {
                    assert!(
                        ((place[axis] - first_place[axis]) * lattice.cell_m).abs() < 1e-3,
                        "class {bin}, camera {camera_m:?}: {place:?}"
                    );
                }
            }
        }
    }
}
