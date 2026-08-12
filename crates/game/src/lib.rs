use pyboat_simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, DT_SECONDS, Environment, HullParameters,
    MotionIntent, computed_maximum_yaw_rate_rad_s, surface_control_input,
    surface_target_control_torques, yaw_rate_move_control,
};
use rapier2d::prelude::Vector;

const WIND_WEIBULL_SCALE_M_S: f64 = 9.0;
const WIND_MODES_PER_BAND: usize = 16;
const TERRAIN_OCTAVE_COUNT: usize = 5;

#[derive(Clone, Copy)]
struct WindBand {
    minimum_wavelength_m: f64,
    maximum_wavelength_m: f64,
    variance_fraction: f64,
}

const WIND_BANDS: [WindBand; 3] = [
    WindBand {
        minimum_wavelength_m: 2048.0,
        maximum_wavelength_m: 8192.0,
        variance_fraction: 0.85,
    },
    WindBand {
        minimum_wavelength_m: 256.0,
        maximum_wavelength_m: 2048.0,
        variance_fraction: 0.10,
    },
    WindBand {
        minimum_wavelength_m: 64.0,
        maximum_wavelength_m: 512.0,
        variance_fraction: 0.05,
    },
];

pub fn dinghy_parameters() -> BoatParameters {
    BoatParameters {
        hull: HullParameters {
            mass_kg: 222.0,
            length_m: 4.0,
            beam_m: 1.5,
            bow_length_m: 0.7,
        },
        sail_mass_kg: 12.0,
        boom_mass_kg: 10.0,
        boom_length_m: 2.0,
        mast_position_m: Vector::new(0.4, 0.0),
        max_sail_angle_rad: std::f32::consts::FRAC_PI_2,
        air_density_kg_m3: 1.225,
        sail_area_m2: 8.0,
        sail_center_of_effort_from_mast_m: 0.7,
        sail_base_drag_coefficient: 0.02,
        water_density_kg_m3: 1025.0,
        keel_area_m2: 0.6,
        keel_position_m: Vector::ZERO,
        keel_base_drag_coefficient: 0.02,
        keel_stall_angle_rad: 15.0_f32.to_radians(),
        keel_stall_lift_coefficient: 0.8,
        hull_reference_area_m2: 0.3,
        hull_drag_coefficient: 0.08,
        hull_lateral_drag_coefficient: 0.02,
        rudder_mass_kg: 4.0,
        tiller_mass_kg: 2.0,
        rudder_draw_length_m: 0.5,
        tiller_length_m: 1.0,
        rudder_position_m: Vector::new(-1.8, 0.0),
        max_rudder_angle_rad: 30.0_f32.to_radians(),
        rudder_area_m2: 0.12,
        rudder_base_drag_coefficient: 0.02,
        rudder_stall_angle_rad: 15.0_f32.to_radians(),
        rudder_stall_lift_coefficient: 0.8,
        max_sail_actuator_torque_n_m: 375.0,
        max_rudder_actuator_torque_n_m: 60.0,
        sail_bearing_damping_n_m_s_rad: 20.0,
        rudder_bearing_damping_n_m_s_rad: 2.0,
        draft_m: 0.6,
        ground_static_friction_coefficient: 1.0,
        ground_kinetic_friction_coefficient: 0.6,
    }
}

pub fn match_environment() -> Environment {
    Environment {
        true_wind_velocity_m_s: Vector::new(6.0, 0.0),
        true_water_velocity_m_s: Vector::ZERO,
        water_depth_m: f32::INFINITY,
    }
}

pub fn procedural_match_environment(match_seed: u64, tick: u64, position_m: Vector) -> Environment {
    Environment {
        true_wind_velocity_m_s: procedural_wind(match_seed, tick, position_m),
        true_water_velocity_m_s: Vector::ZERO,
        water_depth_m: (-procedural_terrain_elevation_m(match_seed, position_m)).max(0.0),
    }
}

pub fn procedural_terrain_elevation_m(match_seed: u64, position_m: Vector) -> f32 {
    let mut noise = 0.0;
    let mut amplitude = 1.0;
    let mut amplitude_sum = 0.0;
    let mut wavelength_m = 128.0;
    for octave in 0..TERRAIN_OCTAVE_COUNT {
        noise += amplitude
            * terrain_value_noise(
                match_seed ^ (octave as u64).wrapping_mul(0xd1b5_4a32_d192_ed03),
                position_m,
                wavelength_m,
            );
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.5;
    }
    (12.0 * noise / amplitude_sum - 2.0) as f32
}

fn terrain_value_noise(match_seed: u64, position_m: Vector, wavelength_m: f64) -> f64 {
    let x = f64::from(position_m.x) / wavelength_m;
    let y = f64::from(position_m.y) / wavelength_m;
    let x0 = libm::floor(x) as i64;
    let y0 = libm::floor(y) as i64;
    let blend_x = smoothstep(x - x0 as f64);
    let blend_y = smoothstep(y - y0 as f64);
    let bottom = lerp(
        terrain_lattice_value(match_seed, x0, y0),
        terrain_lattice_value(match_seed, x0 + 1, y0),
        blend_x,
    );
    let top = lerp(
        terrain_lattice_value(match_seed, x0, y0 + 1),
        terrain_lattice_value(match_seed, x0 + 1, y0 + 1),
        blend_x,
    );
    lerp(bottom, top, blend_y)
}

fn terrain_lattice_value(match_seed: u64, x: i64, y: i64) -> f64 {
    let key = match_seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xe703_7ed1_a0b4_28db);
    2.0 * hash_unit(key) - 1.0
}

fn smoothstep(value: f64) -> f64 {
    value * value * (3.0 - 2.0 * value)
}

fn lerp(first: f64, second: f64, amount: f64) -> f64 {
    first + amount * (second - first)
}

fn procedural_wind(match_seed: u64, tick: u64, position_m: Vector) -> Vector {
    let time_seconds = tick as f64 * f64::from(DT_SECONDS);
    let transport_direction_rad =
        std::f64::consts::TAU * hash_unit(match_seed ^ 0x243f_6a88_85a3_08d3);
    let transported_x_m =
        f64::from(position_m.x) - 6.0 * libm::cos(transport_direction_rad) * time_seconds;
    let transported_y_m =
        f64::from(position_m.y) - 6.0 * libm::sin(transport_direction_rad) * time_seconds;
    let mut velocity_x_m_s = 0.0;
    let mut velocity_y_m_s = 0.0;
    for (band_index, band) in WIND_BANDS.iter().enumerate() {
        let band_key = match_seed ^ (band_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let rotation = hash_unit(band_key ^ 0xa076_1d64_78bd_642f);
        let amplitude_m_s = WIND_WEIBULL_SCALE_M_S
            * libm::sqrt(band.variance_fraction / WIND_MODES_PER_BAND as f64);
        for mode in 0..WIND_MODES_PER_BAND {
            let mode_key = band_key ^ (mode as u64).wrapping_mul(0xe703_7ed1_a0b4_28db);
            let direction_rad =
                std::f64::consts::TAU * (mode as f64 / WIND_MODES_PER_BAND as f64 + rotation);
            let direction_x = libm::cos(direction_rad);
            let direction_y = libm::sin(direction_rad);
            let wavelength_fraction = hash_unit(mode_key ^ 0x8ebc_6af0_9c88_c6e3);
            let wavelength_m = band.minimum_wavelength_m
                * libm::exp(
                    libm::log(band.maximum_wavelength_m / band.minimum_wavelength_m)
                        * wavelength_fraction,
                );
            let phase_rad = std::f64::consts::TAU
                * ((transported_x_m * direction_x + transported_y_m * direction_y) / wavelength_m);
            let (cosine_coefficient, sine_coefficient) = normal_pair(mode_key);
            let wave =
                cosine_coefficient * libm::cos(phase_rad) + sine_coefficient * libm::sin(phase_rad);
            velocity_x_m_s -= amplitude_m_s * direction_y * wave;
            velocity_y_m_s += amplitude_m_s * direction_x * wave;
        }
    }
    Vector::new(velocity_x_m_s as f32, velocity_y_m_s as f32)
}

fn normal_pair(key: u64) -> (f64, f64) {
    let radius = libm::sqrt(-2.0 * libm::log(hash_unit(key ^ 0x5899_65cc_7537_4cc3)));
    let angle = std::f64::consts::TAU * hash_unit(key ^ 0x1d8e_4e27_c47d_124f);
    (radius * libm::cos(angle), radius * libm::sin(angle))
}

fn hash_unit(value: u64) -> f64 {
    let hash = wind_hash(value);
    ((hash >> 11) as f64 + 0.5) / (1_u64 << 53) as f64
}

fn wind_hash(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    value
}

pub fn computed_yaw_rate_torques(motion: BoatMotion, intent: MotionIntent) -> ActuatorTorques {
    computed_yaw_rate_torques_in_environment(motion, intent, match_environment())
}

pub fn computed_yaw_rate_torques_in_environment(
    motion: BoatMotion,
    intent: MotionIntent,
    environment: Environment,
) -> ActuatorTorques {
    let parameters = dinghy_parameters();
    let input = surface_control_input(parameters, motion, environment, 1, 10.0_f32.to_radians());
    let maximum_yaw_rate_rad_s = computed_maximum_yaw_rate_rad_s(parameters, motion, environment);
    let targets = yaw_rate_move_control(&input, intent, maximum_yaw_rate_rad_s);
    surface_target_control_torques(parameters, motion, targets)
}
