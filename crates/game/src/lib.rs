use pyboat_simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, Environment, HullParameters, MotionIntent,
    computed_maximum_yaw_rate_rad_s, surface_control_input, surface_target_control_torques,
    yaw_rate_move_control,
};
use rapier2d::prelude::Vector;

const WIND_SPATIAL_CELL_M: f32 = 32.0;
const WIND_TIME_CELL_TICKS: u64 = 600;

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
    }
}

pub fn match_environment() -> Environment {
    Environment {
        true_wind_velocity_m_s: Vector::new(6.0, 0.0),
        true_water_velocity_m_s: Vector::ZERO,
    }
}

pub fn procedural_match_environment(match_seed: u64, tick: u64, position_m: Vector) -> Environment {
    Environment {
        true_wind_velocity_m_s: Vector::new(
            6.0 + wind_noise(match_seed, tick, position_m, 0),
            1.5 * wind_noise(match_seed, tick, position_m, 1),
        ),
        true_water_velocity_m_s: Vector::ZERO,
    }
}

fn wind_noise(match_seed: u64, tick: u64, position_m: Vector, channel: u64) -> f32 {
    let cell_x = position_m.x / WIND_SPATIAL_CELL_M;
    let cell_y = position_m.y / WIND_SPATIAL_CELL_M;
    let x0 = cell_x.floor() as i64;
    let y0 = cell_y.floor() as i64;
    let time0 = tick / WIND_TIME_CELL_TICKS;
    let x_fraction = smoothstep(cell_x - x0 as f32);
    let y_fraction = smoothstep(cell_y - y0 as f32);
    let time_fraction =
        smoothstep((tick % WIND_TIME_CELL_TICKS) as f32 / WIND_TIME_CELL_TICKS as f32);

    let mut time_samples = [0.0; 2];
    for (time_offset, sample) in time_samples.iter_mut().enumerate() {
        let time = time0 + time_offset as u64;
        let lower = lerp(
            wind_lattice(match_seed, x0, y0, time, channel),
            wind_lattice(match_seed, x0 + 1, y0, time, channel),
            x_fraction,
        );
        let upper = lerp(
            wind_lattice(match_seed, x0, y0 + 1, time, channel),
            wind_lattice(match_seed, x0 + 1, y0 + 1, time, channel),
            x_fraction,
        );
        *sample = lerp(lower, upper, y_fraction);
    }
    lerp(time_samples[0], time_samples[1], time_fraction)
}

fn wind_lattice(match_seed: u64, x: i64, y: i64, time: u64, channel: u64) -> f32 {
    let mut value = match_seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ time.wrapping_mul(0x94d0_49bb_1331_11eb)
        ^ channel.wrapping_mul(0xd6e8_feb8_6659_fd93);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    let unit = (value >> 40) as f32 / ((1_u32 << 24) - 1) as f32;
    unit * 2.0 - 1.0
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn lerp(first: f32, second: f32, amount: f32) -> f32 {
    first + (second - first) * amount
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
