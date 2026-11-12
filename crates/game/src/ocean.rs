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
pub const OCEAN_SWELL_MODE_COUNT: usize = 4;

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

/// The wind sea's modes at a clock: each band's direction rotation and its
/// modes in direction order.
#[derive(Clone, Debug, PartialEq)]
pub struct OceanWaveTable {
    pub band_rotation_rad: [f64; OCEAN_WAVE_BAND_COUNT],
    pub modes: [[OceanWaveMode; OCEAN_WAVE_DIRECTIONS]; OCEAN_WAVE_BAND_COUNT],
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
    for_each_wind_sea_component(
        &table,
        surface_wind_velocity_m_s,
        |band, mode, amplitude_m| {
            components.push(WaveComponent {
                band,
                mode: *mode,
                amplitude_m,
            });
        },
    );
    components
}

/// Calls `visit` with the band, mode and amplitude of each wind-sea mode a
/// point under `surface_wind_velocity_m_s` visits. Each band holds the
/// spectrum's variance over its wavenumbers inside the gameplay window, so
/// bands enter and leave the window empty as the wind moves its peak. Within
/// a band, amplitudes follow Mitsuyasu's cos^2s(θ/2) at the band's frequency,
/// s = s_p (f/f_p)^5 below the peak and s_p (f/f_p)^-2.5 above, normalized
/// so the band keeps its variance. Compromises: bands are half an octave
/// wide throughout, as one table serves every peak; and a band visits only
/// its `VISITED_DIRECTIONS` directions nearest the wind, under a Tukey window
/// flat over their inner half that takes each to zero before the visited set
/// changes, which narrows the broad spreads away from the peak. Returns the
/// sea's mean wavenumber, each band's centre weighted by its variance.
fn for_each_wind_sea_component(
    table: &OceanWaveTable,
    surface_wind_velocity_m_s: Vector,
    mut visit: impl FnMut(usize, &OceanWaveMode, f64),
) -> f64 {
    let speed_m_s = f64::from(surface_wind_velocity_m_s.length());
    if speed_m_s <= 0.0 {
        return 0.0;
    }
    let sea = wind_sea(speed_m_s);
    let peak = sea.peak_wavenumber_per_m;
    let lowest_wavenumber = std::f64::consts::TAU / OCEAN_WAVE_LONGEST_M;
    let spacing = std::f64::consts::TAU / OCEAN_WAVE_DIRECTIONS as f64;
    let window_width = 0.5 * VISITED_DIRECTIONS as f64 * spacing;
    let wind_angle = libm::atan2(
        f64::from(surface_wind_velocity_m_s.y),
        f64::from(surface_wind_velocity_m_s.x),
    );
    let window = GAMEPLAY_WINDOW.map(|ratio| ratio * peak);
    let first_band =
        libm::floor(OCEAN_WAVE_BANDS_PER_OCTAVE as f64 * libm::log2(window[0] / lowest_wavenumber))
            as i64;
    let (mut weighted_wavenumber, mut window_variance) = (0.0, 0.0);
    for band in first_band..first_band + VISITED_BANDS as i64 {
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
        // In deep water (ω_p/ω)⁴ = (k_p/k)², so the variance below k is
        // exp(-5/4 (k_p/k)²) of the whole.
        let variance = sea.variance_m2
            * (libm::exp(-1.25 * (peak / upper).powi(2))
                - libm::exp(-1.25 * (peak / lower).powi(2)));
        weighted_wavenumber += variance * libm::sqrt(lower * upper);
        window_variance += variance;
        let frequency_ratio = libm::sqrt(libm::sqrt(lower * upper) / peak);
        let spread = sea.peak_spread
            * libm::pow(
                frequency_ratio,
                if frequency_ratio < 1.0 { 5.0 } else { -2.5 },
            );
        let position = (wind_angle - table.band_rotation_rad[band]) / spacing;
        let nearest = libm::floor(position);
        let offset = position - nearest;
        let mut roots = [0.0; VISITED_DIRECTIONS];
        for (direction, root) in roots.iter_mut().enumerate() {
            let angle =
                (direction as f64 + 1.0 - 0.5 * VISITED_DIRECTIONS as f64 - offset) * spacing;
            *root = libm::pow(libm::cos(0.5 * angle), spread)
                * libm::cos(
                    std::f64::consts::PI * (libm::fabs(angle) / window_width - 0.5).max(0.0),
                );
        }
        let weight: f64 = roots.iter().map(|root| root * root).sum();
        let scale = libm::sqrt(2.0 * variance / weight);
        for (direction, root) in roots.into_iter().enumerate() {
            let table_direction =
                (nearest as i64 + direction as i64 + 1 - VISITED_DIRECTIONS as i64 / 2)
                    .rem_euclid(OCEAN_WAVE_DIRECTIONS as i64) as usize;
            visit(band, &table.modes[band][table_direction], scale * root);
        }
    }
    if window_variance > 0.0 {
        weighted_wavenumber / window_variance
    } else {
        0.0
    }
}

/// What raises the sea at a point: the sustained surface wind, the upper wind
/// that sets swell, and the water's depth.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OceanWaveForcing {
    pub surface_wind_velocity_m_s: Vector,
    pub upper_wind_velocity_m_s: Vector,
    pub water_depth_m: f32,
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
            upper_wind_velocity_m_s: query.weather.upper_wind_velocity_m_s,
            water_depth_m: query.water_depth_m,
        },
    )
}

/// The sea's surface at a point and clock under `forcing`: the wind sea's
/// modes with Tayfun's (1980) narrow-band second order,
/// η₂ = (k_m/2)(η² - η̂²) at their mean wavenumber, where η̂ is the sum's
/// quadrature, and the upper wind's swell. Compromise: the significant height
/// is capped at 0.78 of the depth, McCowan's (1894) breaker limit, a hard cap
/// that a smooth saturation is to replace with the shore's waves.
pub fn ocean_wave_state(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    position_m: Vector,
    forcing: OceanWaveForcing,
) -> OceanState {
    let upper_wind_speed_m_s = forcing.upper_wind_velocity_m_s.length();
    let upper_wind_direction = if upper_wind_speed_m_s > 0.0 {
        forcing.upper_wind_velocity_m_s / upper_wind_speed_m_s
    } else {
        Vector::ZERO
    };
    let wind_sea_height_m =
        wind_sea(f64::from(forcing.surface_wind_velocity_m_s.length())).significant_height_m();
    // Swell from the upper wind stands in for swell travelling from distant storms.
    let swell_height_m = 0.006 * f64::from(upper_wind_speed_m_s).powi(2);
    let unrestricted_height_m = libm::hypot(wind_sea_height_m, swell_height_m);
    let significant_height_m = unrestricted_height_m.min(0.78 * f64::from(forcing.water_depth_m));
    let depth_scale = if unrestricted_height_m > 0.0 {
        significant_height_m / unrestricted_height_m
    } else {
        0.0
    };

    let table = ocean_wave_table(world_geometry, match_seed, world_tick, world_tick_fraction);
    let width_m = f64::from(world_geometry.width_m() as f32);
    let x_m = f64::from(position_m.x).rem_euclid(width_m);
    let y_m = f64::from(position_m.y);
    let mut sum = SurfaceSum::default();
    let mean_wavenumber = for_each_wind_sea_component(
        &table,
        forcing.surface_wind_velocity_m_s,
        |_, mode, amplitude_m| sum.add(mode, depth_scale * amplitude_m, x_m, y_m),
    );
    let mut displacement_m = sum.displacement_m
        + 0.5
            * mean_wavenumber
            * (sum.displacement_m * sum.displacement_m - sum.quadrature_m * sum.quadrature_m);
    let mut slope = sum.slope
        + mean_wavenumber
            * (sum.displacement_m * sum.slope - sum.quadrature_m * sum.quadrature_slope);
    let mut orbital_velocity_m_s = sum.orbital_velocity_m_s;

    let swell_modes =
        cached_ocean_swell_modes(world_geometry, match_seed, world_tick, world_tick_fraction);
    let swell_weights = swell_modes.map(|mode| {
        let alignment = (mode.wave_number_x_per_m * f64::from(upper_wind_direction.x)
            + mode.wave_number_y_per_m * f64::from(upper_wind_direction.y))
            / mode.wave_number_per_m;
        0.20 + alignment.max(0.0).powi(4)
    });
    let swell_scale = depth_scale * swell_height_m
        / (4.0
            * libm::sqrt(
                0.5 * swell_weights
                    .iter()
                    .map(|weight| weight * weight)
                    .sum::<f64>(),
            ));
    let mut swell = SurfaceSum::default();
    for (mode, weight) in swell_modes.iter().zip(swell_weights) {
        swell.add(mode, swell_scale * weight, x_m, y_m);
    }
    displacement_m += swell.displacement_m;
    slope += swell.slope;
    orbital_velocity_m_s += swell.orbital_velocity_m_s;

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

/// The wind sea's wave table at a clock. Each band's modes take seeded
/// wavenumbers within the band, with eastward wavenumbers fitted to the
/// world's width so waves join across longitude. Only the phases depend on
/// the clock, so the rest is built once per world and seed.
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
        });
        *latest = Some((inputs, table.clone()));
        table
    })
}

fn ocean_wave_table_at_start(world_geometry: WorldGeometry, match_seed: u64) -> Rc<OceanWaveTable> {
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
        let table = Rc::new(OceanWaveTable {
            band_rotation_rad,
            modes,
        });
        *latest = Some(((world_geometry, match_seed), table.clone()));
        table
    })
}

/// The upper wind's swell modes: wavelengths log-uniform from 120 to 600 m,
/// directions over a quarter turn from a seeded rotation.
pub fn ocean_swell_modes(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
) -> [OceanWaveMode; OCEAN_SWELL_MODE_COUNT] {
    let rotation = std::f64::consts::TAU * hash_unit(match_seed ^ 0x3c6e_f372_fe94_f82b);
    std::array::from_fn(|swell_index| {
        let mode_index = 12 + swell_index;
        let mode_key = match_seed
            ^ 0xa409_3822_299f_31d0
            ^ (mode_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let wavelength_m = 120.0 * libm::pow(5.0, hash_unit(mode_key ^ 0xe703_7ed1_a0b4_28db));
        ocean_wave_mode(
            world_geometry,
            rotation + std::f64::consts::TAU * mode_index as f64 / 16.0,
            wavelength_m,
            mode_key,
        )
        .at_clock(world_tick, world_tick_fraction)
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
type CachedWaveTable<Inputs> = RefCell<Option<(Inputs, Rc<OceanWaveTable>)>>;

thread_local! {
    // Keyed on every input of the tables, which depend on world, seed and
    // clock alone, so all samples at one clock share one table.
    static WAVE_TABLE_AT_START: CachedWaveTable<(WorldGeometry, u64)> =
        const { RefCell::new(None) };
    static LATEST_WAVE_TABLE: CachedWaveTable<WaveModeInputs> =
        const { RefCell::new(None) };
    static LATEST_SWELL_MODES: RefCell<Option<(WaveModeInputs, [OceanWaveMode; OCEAN_SWELL_MODE_COUNT])>> =
        const { RefCell::new(None) };
}

fn cached_ocean_swell_modes(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
) -> [OceanWaveMode; OCEAN_SWELL_MODE_COUNT] {
    let inputs = (
        world_geometry,
        match_seed,
        world_tick,
        world_tick_fraction.to_bits(),
    );
    LATEST_SWELL_MODES.with_borrow_mut(|latest| {
        if let Some((latest_inputs, modes)) = latest
            && *latest_inputs == inputs
        {
            return *modes;
        }
        let modes = ocean_swell_modes(world_geometry, match_seed, world_tick, world_tick_fraction);
        *latest = Some((inputs, modes));
        modes
    })
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
