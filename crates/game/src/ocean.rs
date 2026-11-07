use std::cell::Cell;

use physics::glamx::Vec2 as Vector;
use simulation::WorldGeometry;

use crate::{OceanQuery, OceanState, TICK_SECONDS, hash_unit, smoothstep_range};

const MAX_SIGNIFICANT_WAVE_HEIGHT_M: f32 = 14.0;
pub const OCEAN_WAVE_MODE_COUNT: usize = 16;
const OCEAN_WIND_SEA_MODE_COUNT: usize = 12;
const GRAVITY_M_S2: f64 = 9.81;

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

pub(crate) fn procedural_wave_state(query: OceanQuery) -> OceanState {
    let wind_speed_m_s = query.wave_forcing_wind_velocity_m_s.length();
    let upper_wind_speed_m_s = query.weather.upper_wind_velocity_m_s.length();
    let wind_direction = if wind_speed_m_s > 0.0 {
        query.wave_forcing_wind_velocity_m_s / wind_speed_m_s
    } else {
        Vector::ZERO
    };
    let upper_wind_direction = if upper_wind_speed_m_s > 0.0 {
        query.weather.upper_wind_velocity_m_s / upper_wind_speed_m_s
    } else {
        Vector::ZERO
    };
    let wind_sea_height_m = 0.05 + 0.016 * wind_speed_m_s.powi(2);
    // Swell from the upper wind stands in for swell travelling from distant storms.
    let swell_height_m = 0.006 * upper_wind_speed_m_s.powi(2);
    let unrestricted_height_m = libm::hypotf(wind_sea_height_m, swell_height_m);
    // 0.78 is McCowan's (1894) breaker limit; the cap stands in for fetch and
    // duration limits.
    let significant_wave_height_m = unrestricted_height_m
        .min(0.78 * query.water_depth_m)
        .min(MAX_SIGNIFICANT_WAVE_HEIGHT_M);
    let depth_scale = if unrestricted_height_m > 0.0 {
        significant_wave_height_m / unrestricted_height_m
    } else {
        0.0
    };

    let wave_modes = cached_ocean_wave_modes(
        query.world_geometry,
        query.match_seed,
        query.world_tick,
        query.world_tick_fraction,
    );
    let mut modes = [(0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64); OCEAN_WAVE_MODE_COUNT];
    let mut wind_weight_square_sum = 0.0;
    let mut swell_weight_square_sum = 0.0;
    for (mode_index, (mode, wave_mode)) in modes.iter_mut().zip(wave_modes).enumerate() {
        let is_wind_sea = mode_index < OCEAN_WIND_SEA_MODE_COUNT;
        let actual_direction_x = wave_mode.wave_number_x_per_m / wave_mode.wave_number_per_m;
        let actual_direction_y = wave_mode.wave_number_y_per_m / wave_mode.wave_number_per_m;
        let source_direction = if is_wind_sea {
            wind_direction
        } else {
            upper_wind_direction
        };
        let alignment = (actual_direction_x as f32 * source_direction.x
            + actual_direction_y as f32 * source_direction.y)
            .max(0.0);
        let weight = 0.20 + f64::from(alignment.powi(4));
        if is_wind_sea {
            wind_weight_square_sum += weight * weight;
        } else {
            swell_weight_square_sum += weight * weight;
        }
        let width_m = f64::from(query.world_geometry.width_m() as f32);
        let phase = wave_mode.wave_number_x_per_m
            * f64::from(query.position_m.x).rem_euclid(width_m)
            + wave_mode.wave_number_y_per_m * f64::from(query.position_m.y)
            + wave_mode.phase_rad;
        *mode = (
            actual_direction_x,
            actual_direction_y,
            wave_mode.wave_number_per_m,
            wave_mode.angular_frequency_rad_s,
            phase,
            weight,
        );
    }

    let wind_amplitude_scale = f64::from(depth_scale * wind_sea_height_m)
        / (4.0 * libm::sqrt(0.5 * wind_weight_square_sum));
    let swell_amplitude_scale =
        f64::from(depth_scale * swell_height_m) / (4.0 * libm::sqrt(0.5 * swell_weight_square_sum));
    let mut displacement_m = 0.0;
    let mut slope = Vector::ZERO;
    let mut orbital_velocity_m_s = Vector::ZERO;
    let mut energy_direction = Vector::ZERO;
    for (mode_index, mode) in modes.into_iter().enumerate() {
        let (direction_x, direction_y, wave_number, angular_frequency, phase, weight) = mode;
        let amplitude_m = weight
            * if mode_index < OCEAN_WIND_SEA_MODE_COUNT {
                wind_amplitude_scale
            } else {
                swell_amplitude_scale
            };
        let (phase_sine, phase_cosine) = libm::sincos(phase);
        displacement_m += amplitude_m * phase_cosine;
        slope += Vector::new(
            (-amplitude_m * phase_sine * wave_number * direction_x) as f32,
            (-amplitude_m * phase_sine * wave_number * direction_y) as f32,
        );
        orbital_velocity_m_s += Vector::new(
            (amplitude_m * angular_frequency * phase_cosine * direction_x) as f32,
            (amplitude_m * angular_frequency * phase_cosine * direction_y) as f32,
        );
        energy_direction += Vector::new(
            (amplitude_m * amplitude_m * direction_x) as f32,
            (amplitude_m * amplitude_m * direction_y) as f32,
        );
    }
    let nonlinear_strength = 0.5 * smoothstep_range(1.5, 3.0, f64::from(significant_wave_height_m));
    let significant_height_m = f64::from(significant_wave_height_m);
    let bound_displacement_m = if significant_height_m > 0.0 {
        nonlinear_strength
            * (displacement_m * displacement_m - significant_height_m * significant_height_m / 16.0)
            / significant_height_m
    } else {
        0.0
    };
    let bound_response = if significant_height_m > 0.0 {
        (1.0 + 2.0 * nonlinear_strength * displacement_m / significant_height_m) as f32
    } else {
        1.0
    };
    displacement_m += bound_displacement_m;
    slope *= bound_response;
    let dominant_wave_direction = if energy_direction == Vector::ZERO {
        Vector::ZERO
    } else {
        energy_direction.normalize()
    };
    let surface_displacement_m = displacement_m as f32;
    OceanState {
        current_velocity_m_s: Vector::ZERO,
        wave_orbital_velocity_m_s: orbital_velocity_m_s,
        significant_wave_height_m,
        surface_displacement_m,
        surface_slope: slope,
        dominant_wave_direction,
        rogue_crest_excess_m: (surface_displacement_m - significant_wave_height_m).max(0.0),
    }
}

pub fn ocean_wave_modes(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
) -> [OceanWaveMode; OCEAN_WAVE_MODE_COUNT] {
    let rotation = std::f64::consts::TAU * hash_unit(match_seed ^ 0x3c6e_f372_fe94_f82b);
    let width_m = f64::from(world_geometry.width_m() as f32);
    std::array::from_fn(|mode_index| {
        let mode_key = match_seed
            ^ 0xa409_3822_299f_31d0
            ^ (mode_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let direction_angle =
            rotation + std::f64::consts::TAU * mode_index as f64 / OCEAN_WAVE_MODE_COUNT as f64;
        let (direction_y, direction_x) = libm::sincos(direction_angle);
        let wavelength_fraction = hash_unit(mode_key ^ 0xe703_7ed1_a0b4_28db);
        let (minimum_wavelength_m, maximum_wavelength_m) = if mode_index < OCEAN_WIND_SEA_MODE_COUNT
        {
            (8.0_f64, 180.0_f64)
        } else {
            (120.0_f64, 600.0_f64)
        };
        let wavelength_m = minimum_wavelength_m
            * libm::pow(
                maximum_wavelength_m / minimum_wavelength_m,
                wavelength_fraction,
            );
        let mut mode_x = libm::round(direction_x * width_m / wavelength_m) as i64;
        if mode_x == 0 && direction_y.abs() < 1.0e-6 {
            mode_x = if direction_x < 0.0 { -1 } else { 1 };
        }
        let wave_number_x_per_m = std::f64::consts::TAU * mode_x as f64 / width_m;
        let wave_number_y_per_m = std::f64::consts::TAU * direction_y / wavelength_m;
        let wave_number_per_m = libm::hypot(wave_number_x_per_m, wave_number_y_per_m);
        let angular_frequency_rad_s = libm::sqrt(GRAVITY_M_S2 * wave_number_per_m);
        OceanWaveMode {
            wave_number_x_per_m,
            wave_number_y_per_m,
            wave_number_per_m,
            angular_frequency_rad_s,
            phase_rad: std::f64::consts::TAU * hash_unit(mode_key ^ 0xbf58_476d_1ce4_e5b9)
                - tick_phase_radians(world_tick, angular_frequency_rad_s * TICK_SECONDS)
                - f64::from(world_tick_fraction) * angular_frequency_rad_s * TICK_SECONDS,
        }
    })
}

type WaveModeInputs = (WorldGeometry, u64, u64, u32);

thread_local! {
    static LATEST_WAVE_MODES: Cell<Option<(WaveModeInputs, [OceanWaveMode; OCEAN_WAVE_MODE_COUNT])>> =
        const { Cell::new(None) };
}

// Keyed on every input of `ocean_wave_modes`, which depends on world, seed,
// and clock alone, so all samples at one clock share one table.
fn cached_ocean_wave_modes(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
) -> [OceanWaveMode; OCEAN_WAVE_MODE_COUNT] {
    let inputs = (
        world_geometry,
        match_seed,
        world_tick,
        world_tick_fraction.to_bits(),
    );
    LATEST_WAVE_MODES.with(|latest| {
        if let Some((latest_inputs, modes)) = latest.get()
            && latest_inputs == inputs
        {
            return modes;
        }
        let modes = ocean_wave_modes(world_geometry, match_seed, world_tick, world_tick_fraction);
        latest.set(Some((inputs, modes)));
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
