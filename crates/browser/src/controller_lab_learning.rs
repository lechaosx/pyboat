use super::{LabObservation, physical_targets};
use simulation::SurfaceTargets;

const INPUTS: usize = 18;
const HIDDEN1: usize = 32;
const HIDDEN2: usize = 16;
const WEIGHTS: usize = 1170;
const PREDICTION_SECONDS: f64 = 0.4;
const STAGES: usize = 24;
const RATE_SCALE: f64 = 0.4;

#[derive(Clone, Default, Debug)]
pub(super) struct PredictiveState {
    plan: [[f64; 2]; 2],
    initialized: bool,
}

#[derive(Clone, Copy, Default, Debug)]
struct ModelState {
    yaw_rate: f64,
    heading: f64,
    angle: [f64; 2],
    rate: [f64; 2],
}

#[derive(Clone, Copy)]
struct JointStep {
    mass: [f64; 3],
    rate: [f64; 2],
    load: [f64; 2],
    stiffness: [f64; 2],
    damping: [f64; 2],
    bearing: [f64; 2],
    error: [f64; 2],
    cap: [f64; 2],
    h: f64,
}

fn bounded_joint_rates(step: JointStep) -> [f64; 2] {
    let [a, c, cross] = step.mass;
    for first in [-1, 0, 1] {
        for second in [-1, 0, 1] {
            let status = [first, second];
            let mut diagonal = [a, c];
            let mut rhs = [
                a * step.rate[0] + cross * step.rate[1] + step.h * step.load[0],
                cross * step.rate[0] + c * step.rate[1] + step.h * step.load[1],
            ];
            for j in 0..2 {
                diagonal[j] += step.h * step.bearing[j];
                if status[j] == 0 {
                    diagonal[j] += step.h * step.damping[j] + step.h * step.h * step.stiffness[j];
                    rhs[j] += step.h * step.stiffness[j] * step.error[j];
                } else {
                    rhs[j] += step.h * status[j] as f64 * step.cap[j];
                }
            }
            let det = diagonal[0] * diagonal[1] - cross * cross;
            let rate = [
                (diagonal[1] * rhs[0] - cross * rhs[1]) / det,
                (diagonal[0] * rhs[1] - cross * rhs[0]) / det,
            ];
            let mut valid = true;
            for j in 0..2 {
                let motor = step.stiffness[j] * (step.error[j] - step.h * rate[j])
                    - step.damping[j] * rate[j];
                valid &= match status[j] {
                    -1 => motor <= -step.cap[j],
                    1 => motor >= step.cap[j],
                    _ => motor.abs() <= step.cap[j],
                };
            }
            if valid {
                return rate;
            }
        }
    }
    unreachable!("positive-definite implicit motor problem has a bounded solution")
}

struct FutureLoads {
    yaw: f64,
    generalized: [f64; 2],
    drive: f64,
}

fn future_loads(o: &LabObservation, state: ModelState) -> FutureLoads {
    use rapier2d_f64::prelude::{Rotation, Vector};
    let p = o.parameters;
    let rotation = Rotation::new(state.heading);
    let rotate = |v: Vector| rotation.transform_vector(v);
    let perpendicular = |v: Vector| Vector::new(-v.y, v.x);
    let center = rotate(o.center_of_mass_from_hull_m);
    let mut total_force = Vector::ZERO;
    let mut yaw = 0.;
    let mut hinge_torque = [0.; 2];
    let mut drive = 0.;
    let sail_arm = Vector::new(
        -p.boom_length_m * (p.sail_mass_kg / 3. + p.boom_mass_kg / 2.)
            / (p.sail_mass_kg + p.boom_mass_kg),
        0.,
    );
    let rudder_arm = Vector::new(
        (-p.rudder_mass_kg * p.rudder_draw_length_m / 2.
            + p.tiller_mass_kg * p.tiller_length_m / 2.)
            / (p.rudder_mass_kg + p.tiller_mass_kg),
        0.,
    );
    let arms = [sail_arm, rudder_arm];
    let mut world_arms = [Vector::ZERO; 2];
    for j in 0..2 {
        let hinge = rotate(if j == 0 {
            p.mast_position_m
        } else {
            p.rudder_position_m
        });
        let surface_rotation = Rotation::new(state.heading + state.angle[j]);
        world_arms[j] = surface_rotation.transform_vector(arms[j]);
        let samples = if j == 0 {
            simulation::sail_foil_samples(p)
        } else {
            simulation::rudder_foil_samples(p)
        };
        for (offset, area) in samples {
            let offset = surface_rotation.transform_vector(offset);
            let point = hinge + offset;
            let velocity = o.motion.velocity_m_s
                + perpendicular(hinge) * state.yaw_rate
                + perpendicular(offset) * (state.yaw_rate + state.rate[j]);
            let flow = if j == 0 {
                o.environment.true_wind_velocity_m_s
            } else {
                o.environment.true_water_velocity_m_s
            } - velocity;
            let force = if j == 0 {
                simulation::flat_plate_force(
                    flow,
                    state.heading + state.angle[j],
                    p.air_density_kg_m3,
                    area,
                    p.sail_base_drag_coefficient,
                )
            } else {
                simulation::rudder_foil_force(
                    flow,
                    state.heading + state.angle[j],
                    p.water_density_kg_m3,
                    area,
                    p.rudder_area_m2,
                    p.rudder_draw_length_m,
                    p.rudder_base_drag_coefficient,
                )
            };
            total_force += force;
            drive += force.dot(rotate(Vector::X));
            yaw += (point - center).perp_dot(force);
            hinge_torque[j] += offset.perp_dot(force);
        }
    }
    for (offset, area) in simulation::keel_foil_samples(p) {
        let point = rotate(offset);
        let velocity = o.motion.velocity_m_s + perpendicular(point) * state.yaw_rate;
        let force = simulation::streamlined_foil_force(
            o.environment.true_water_velocity_m_s - velocity,
            state.heading,
            p.water_density_kg_m3,
            area,
            p.keel_base_drag_coefficient,
            p.keel_stall_angle_rad,
            p.keel_stall_lift_coefficient,
        );
        total_force += force;
        yaw += (point - center).perp_dot(force);
    }
    for sign in [-1., 1.] {
        let point = rotate(Vector::new(sign * p.hull.length_m / 2., 0.));
        let velocity = o.motion.velocity_m_s + perpendicular(point) * state.yaw_rate;
        let force = simulation::hull_resistance_force(
            o.environment.true_water_velocity_m_s - velocity,
            state.heading,
            p.water_density_kg_m3,
            p.hull_reference_area_m2 / 2.,
            p.hull_drag_coefficient,
            p.hull_lateral_drag_coefficient,
        );
        total_force += force;
        yaw += (point - center).perp_dot(force);
    }
    let mass = [
        p.sail_mass_kg + p.boom_mass_kg,
        p.rudder_mass_kg + p.tiller_mass_kg,
    ];
    let generalized = [0, 1]
        .map(|j| hinge_torque[j] - mass[j] / o.total_mass_kg * world_arms[j].perp_dot(total_force));
    FutureLoads {
        yaw,
        generalized,
        drive,
    }
}

fn advance_model(o: &LabObservation, state: ModelState, target: [f64; 2], h: f64) -> ModelState {
    // Freeze translation, COM and the articulated mass matrix over the horizon;
    // omit Coriolis and contacts. Both motors use an implicit stage approximation,
    // whereas production holds the sail's explicit torque for each physical tick.
    let loads = future_loads(o, state);
    let external = loads.yaw;
    let i = o.locked_inertia_kg_m2;
    let b = [o.sail_yaw_coupling_kg_m2, o.rudder_yaw_coupling_kg_m2];
    let a = o.sail_relative_inertia_kg_m2 - b[0] * b[0] / i;
    let c = o.rudder_relative_inertia_kg_m2 - b[1] * b[1] / i;
    let cross = o.sail_rudder_coupling_kg_m2 - b[0] * b[1] / i;
    let sail_stiffness =
        o.parameters.max_sail_actuator_torque_n_m / (o.parameters.max_sail_angle_rad / 10.0);
    let sail_dry_inertia = (o.parameters.sail_mass_kg / 6.0 + o.parameters.boom_mass_kg / 3.0)
        * o.parameters.boom_length_m.powi(2);
    let sail_damping = 2.0 * (sail_stiffness * sail_dry_inertia).sqrt();
    let stiffness = [sail_stiffness, o.rudder_motor_stiffness_n_m_rad];
    let damping = [sail_damping, o.rudder_motor_damping_n_m_s_rad];
    let bearing = [
        o.parameters.sail_bearing_damping_n_m_s_rad,
        o.parameters.rudder_bearing_damping_n_m_s_rad,
    ];
    let cap = [
        o.parameters.max_sail_actuator_torque_n_m,
        o.parameters.max_rudder_actuator_torque_n_m(),
    ];
    let limit = [
        o.parameters.max_sail_angle_rad,
        o.parameters.max_rudder_angle_rad,
    ];
    let load = [
        loads.generalized[0] - b[0] * external / i,
        loads.generalized[1] - b[1] * external / i,
    ];
    let rates = bounded_joint_rates(JointStep {
        mass: [a, c, cross],
        rate: state.rate,
        load,
        stiffness,
        damping,
        bearing,
        error: [target[0] - state.angle[0], target[1] - state.angle[1]],
        cap,
        h,
    });
    let mut next = state;
    for j in 0..2 {
        next.rate[j] = rates[j];
        let angle = state.angle[j] + h * next.rate[j];
        next.angle[j] = angle.clamp(-limit[j], limit[j]);
        if angle != next.angle[j] {
            next.rate[j] = 0.0;
        }
    }
    next.yaw_rate += h * external / i
        - (b[0] * (next.rate[0] - state.rate[0]) + b[1] * (next.rate[1] - state.rate[1])) / i;
    next.heading += h * next.yaw_rate;
    next
}

fn plan_cost(o: &LabObservation, plan: [[f64; 2]; 2]) -> f64 {
    let desired_rate = o.intent.turn.clamp(-1.0, 1.0) * o.maximum_yaw_rate_rad_s;
    let desired_drive = super::requested_drive_force(o);
    let force_scale = o
        .input
        .sail_effects
        .iter()
        .map(|e| e.drive_force_n.abs())
        .fold(1.0, f64::max);
    let limits = [
        o.parameters.max_sail_angle_rad,
        o.parameters.max_rudder_angle_rad,
    ];
    let mut state = ModelState {
        yaw_rate: o.motion.angular_velocity_rad_s,
        heading: o.motion.angle_rad,
        angle: [o.motion.sail_angle_rad, o.motion.rudder_angle_rad],
        rate: [
            o.motion.sail_angular_velocity_rad_s,
            o.motion.rudder_angular_velocity_rad_s,
        ],
    };
    let mut cost = 0.0;
    for stage in 0..STAGES {
        let target = plan[stage / (STAGES / 2)];
        state = advance_model(o, state, target, PREDICTION_SECONDS / STAGES as f64);
        let drive = future_loads(o, state).drive;
        cost += ((state.yaw_rate - desired_rate) / RATE_SCALE).powi(2)
            + ((drive - desired_drive) / force_scale).powi(2)
            + 0.02 * ((state.rate[0] / limits[0]).powi(2) + (state.rate[1] / limits[1]).powi(2));
    }
    cost /= STAGES as f64;
    cost += ((state.yaw_rate - desired_rate) / RATE_SCALE).powi(2);
    cost += 0.002 * ((plan[0][0] - o.motion.sail_angle_rad) / limits[0]).powi(2)
        + 0.002 * ((plan[0][1] - o.motion.rudder_angle_rad) / limits[1]).powi(2);
    cost
}

pub(super) fn predictive_targets(
    o: &LabObservation,
    state: &mut PredictiveState,
) -> SurfaceTargets {
    let mut targets = physical_targets(o);
    let seed = [
        targets
            .sail_target_angle_rad
            .unwrap_or(targets.sheet_target_angle_rad),
        targets.rudder_target_angle_rad,
    ];
    let mut plan = if state.initialized {
        state.plan
    } else {
        [seed; 2]
    };
    let limits = [
        o.parameters.max_sail_angle_rad,
        o.parameters.max_rudder_angle_rad,
    ];
    let mut best_cost = plan_cost(o, plan);
    for block in 0..2 {
        for surface in 0..2 {
            for sample in 0..9 {
                let mut candidate = plan;
                candidate[block][surface] = limits[surface] * (sample as f64 / 4.0 - 1.0);
                let cost = plan_cost(o, candidate);
                if cost < best_cost {
                    plan = candidate;
                    best_cost = cost;
                }
            }
        }
    }
    for block in 0..2 {
        for surface in 0..2 {
            let center = plan[block][surface];
            for sign in [-1.0, 1.0] {
                let mut candidate = plan;
                candidate[block][surface] = (center + sign * limits[surface] / 12.0)
                    .clamp(-limits[surface], limits[surface]);
                let cost = plan_cost(o, candidate);
                if cost < best_cost {
                    plan = candidate;
                    best_cost = cost;
                }
            }
        }
    }
    state.plan = plan;
    state.initialized = true;
    targets.sheet_target_angle_rad = plan[0][0];
    targets.sail_target_angle_rad = Some(plan[0][0]);
    targets.rudder_target_angle_rad = plan[0][1];
    targets
}

fn softsign(x: f64) -> f64 {
    x / (1.0 + x.abs())
}

fn features_with_seed(o: &LabObservation, physical: SurfaceTargets) -> [f64; INPUTS] {
    let rotation = rapier2d_f64::prelude::Rotation::new(-o.motion.angle_rad);
    let water = o.environment.true_water_velocity_m_s - o.motion.velocity_m_s;
    let wind = o.environment.true_wind_velocity_m_s - o.motion.velocity_m_s;
    let body = |x: f64, y: f64| {
        let local = rotation.transform_vector(rapier2d_f64::prelude::Vector::new(x, y));
        [local.x, local.y]
    };
    let water = body(water.x, water.y);
    let wind = body(wind.x, wind.y);
    [
        o.intent.forward,
        o.intent.turn,
        water[0] / 4.0,
        water[1] / 4.0,
        wind[0] / 8.0,
        wind[1] / 8.0,
        o.motion.angular_velocity_rad_s / RATE_SCALE,
        o.motion.sail_angle_rad / o.parameters.max_sail_angle_rad,
        o.motion.sail_angular_velocity_rad_s / 2.0,
        o.motion.rudder_angle_rad / o.parameters.max_rudder_angle_rad,
        o.motion.rudder_angular_velocity_rad_s / 4.0,
        physical.rudder_target_angle_rad / o.parameters.max_rudder_angle_rad,
        physical
            .sail_target_angle_rad
            .unwrap_or(physical.sheet_target_angle_rad)
            / o.parameters.max_sail_angle_rad,
        o.fluid_hinge_torques_n_m[1] / o.parameters.max_rudder_actuator_torque_n_m(),
        o.fluid_hinge_torques_n_m[0] / o.parameters.max_sail_actuator_torque_n_m,
        o.maximum_yaw_rate_rad_s / 0.8,
        o.rudder_hinge_inertia_kg_m2 / 10.0,
        o.locked_inertia_kg_m2 / 500.0,
    ]
    .map(|x| x.clamp(-4.0, 4.0))
}

fn network_output(x: &[f64; INPUTS], weights: &[f64; WEIGHTS]) -> [f64; 2] {
    let mut offset = 0;
    let mut first = [0.0; HIDDEN1];
    for out in &mut first {
        let mut sum = weights[offset + INPUTS];
        for j in 0..INPUTS {
            sum += x[j] * weights[offset + j];
        }
        *out = softsign(sum);
        offset += INPUTS + 1;
    }
    let mut second = [0.0; HIDDEN2];
    for out in &mut second {
        let mut sum = weights[offset + HIDDEN1];
        for j in 0..HIDDEN1 {
            sum += first[j] * weights[offset + j];
        }
        *out = softsign(sum);
        offset += HIDDEN1 + 1;
    }
    let mut result = [0.0; 2];
    for out in &mut result {
        let mut sum = weights[offset + HIDDEN2];
        for j in 0..HIDDEN2 {
            sum += second[j] * weights[offset + j];
        }
        *out = softsign(sum);
        offset += HIDDEN2 + 1;
    }
    result
}

fn learned_with_weights(o: &LabObservation, weights: &[f64; WEIGHTS]) -> SurfaceTargets {
    let mut targets = physical_targets(o);
    let output = network_output(&features_with_seed(o, targets), weights);
    let sail = targets
        .sail_target_angle_rad
        .unwrap_or(targets.sheet_target_angle_rad)
        + output[0] * 0.5 * o.parameters.max_sail_angle_rad;
    targets.sheet_target_angle_rad = sail.clamp(
        -o.parameters.max_sail_angle_rad,
        o.parameters.max_sail_angle_rad,
    );
    targets.sail_target_angle_rad = Some(targets.sheet_target_angle_rad);
    targets.rudder_target_angle_rad = (targets.rudder_target_angle_rad
        + output[1] * 0.5 * o.parameters.max_rudder_angle_rad)
        .clamp(
            -o.parameters.max_rudder_angle_rad,
            o.parameters.max_rudder_angle_rad,
        );
    targets
}

mod trained {
    include!("controller_lab_weights.rs");
}
pub(super) fn learned_targets(o: &LabObservation) -> SurfaceTargets {
    learned_with_weights(o, &trained::TRAINED_WEIGHTS)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coupled_implicit_motor_obeys_force_limit_and_momentum() {
        let rate = bounded_joint_rates(JointStep {
            mass: [1., 2., 0.],
            rate: [0.; 2],
            load: [0.; 2],
            stiffness: [100.; 2],
            damping: [0.; 2],
            bearing: [0.; 2],
            error: [1., 0.],
            cap: [1.; 2],
            h: 0.1,
        });
        assert!((rate[0] - 0.1).abs() < 1e-12);
        assert_eq!(rate[1], 0.);
        let rate = bounded_joint_rates(JointStep {
            mass: [2., 3., 0.5],
            rate: [0.; 2],
            load: [1., -2.],
            stiffness: [0.; 2],
            damping: [0.; 2],
            bearing: [0.; 2],
            error: [0.; 2],
            cap: [0.; 2],
            h: 0.1,
        });
        assert!((2. * rate[0] + 0.5 * rate[1] - 0.1).abs() < 1e-12);
        assert!((0.5 * rate[0] + 3. * rate[1] + 0.2).abs() < 1e-12);
    }
    #[test]
    fn bearing_resistance_is_independent_of_hand_force_limit() {
        let rate = bounded_joint_rates(JointStep {
            mass: [1., 2., 0.],
            rate: [10., 0.],
            load: [0.; 2],
            stiffness: [0.; 2],
            damping: [0.; 2],
            bearing: [100., 0.],
            error: [0.; 2],
            cap: [0.; 2],
            h: 0.1,
        });
        assert!((rate[0] - 10. / 11.).abs() < 1e-12);
    }

    #[test]
    fn bounded_joint_solutions_satisfy_the_motor_equations() {
        for coupling in [-0.7, 0., 0.7] {
            for error in [-2., 0., 2.] {
                let step = JointStep {
                    mass: [2., 3., coupling],
                    rate: [1.3, -0.7],
                    load: [4., -3.],
                    stiffness: [200., 400.],
                    damping: [20., 30.],
                    bearing: [4., 7.],
                    error: [error, -0.3 * error],
                    cap: [8., 12.],
                    h: 0.05,
                };
                let rate = bounded_joint_rates(JointStep { ..step });
                let torque = [0, 1].map(|j| {
                    (step.stiffness[j] * (step.error[j] - step.h * rate[j])
                        - step.damping[j] * rate[j])
                        .clamp(-step.cap[j], step.cap[j])
                        - step.bearing[j] * rate[j]
                        + step.load[j]
                });
                assert!(
                    (step.mass[0] * (rate[0] - step.rate[0]) + coupling * (rate[1] - step.rate[1])
                        - step.h * torque[0])
                        .abs()
                        < 1e-12
                );
                assert!(
                    (coupling * (rate[0] - step.rate[0]) + step.mass[1] * (rate[1] - step.rate[1])
                        - step.h * torque[1])
                        .abs()
                        < 1e-12
                );
            }
        }
    }

    #[test]
    fn predicted_initial_fluid_loads_match_authoritative_force_observer() {
        use rapier2d_f64::prelude::Vector;
        for case in 0..4 {
            let parameters = game::dinghy_parameters();
            let motion = simulation::BoatMotion {
                position_m: Vector::new(100., -230.),
                velocity_m_s: Vector::new(2., 0.3),
                angle_rad: case as f64 * 0.7,
                angular_velocity_rad_s: 0.15,
                sail_angle_rad: 0.4,
                sail_angular_velocity_rad_s: -0.3,
                rudder_angle_rad: -0.2,
                rudder_angular_velocity_rad_s: 0.7,
            };
            let environment = simulation::Environment {
                true_wind_velocity_m_s: Vector::new(0., 6.),
                true_water_velocity_m_s: Vector::new(0.2, -0.1),
                water_depth_m: f64::INFINITY,
            };
            let mut simulation = simulation::SharedSimulation::new();
            let boat = simulation.add_boat(parameters, motion);
            let d = simulation.control_dynamics(boat, environment);
            let motion = simulation.motion(boat);
            let o = LabObservation {
                parameters,
                motion,
                environment,
                intent: simulation::MotionIntent::default(),
                input: simulation::surface_control_input_about_point(
                    parameters,
                    motion,
                    environment,
                    1,
                    f64::from(10_f32.to_radians()),
                    d.center_of_mass_from_hull_m,
                ),
                maximum_yaw_rate_rad_s: simulation::computed_maximum_yaw_rate_rad_s(
                    parameters,
                    motion,
                    environment,
                ),
                locked_inertia_kg_m2: d.locked_inertia_kg_m2,
                total_mass_kg: d.total_mass_kg,
                center_of_mass_from_hull_m: d.center_of_mass_from_hull_m,
                sail_hinge_inertia_kg_m2: d.sail_hinge_inertia_kg_m2,
                sail_yaw_coupling_kg_m2: d.sail_yaw_coupling_kg_m2,
                sail_relative_inertia_kg_m2: d.sail_relative_inertia_kg_m2,
                sail_rudder_coupling_kg_m2: d.sail_rudder_coupling_kg_m2,
                rudder_hinge_inertia_kg_m2: d.rudder_hinge_inertia_kg_m2,
                rudder_yaw_coupling_kg_m2: d.rudder_yaw_coupling_kg_m2,
                rudder_relative_inertia_kg_m2: d.rudder_relative_inertia_kg_m2,
                fluid_hinge_torques_n_m: d.fluid_hinge_torques_n_m,
                actual_fluid_yaw_moments_n_m: d.fluid_yaw_moments_n_m,
                passive_yaw_moment_n_m: d.passive_yaw_moment_n_m,
                rudder_motor_stiffness_n_m_rad: d.rudder_motor_stiffness_n_m_rad,
                rudder_motor_damping_n_m_s_rad: d.rudder_motor_damping_n_m_s_rad,
            };
            let state = ModelState {
                heading: o.motion.angle_rad,
                yaw_rate: o.motion.angular_velocity_rad_s,
                angle: [o.motion.sail_angle_rad, o.motion.rudder_angle_rad],
                rate: [
                    o.motion.sail_angular_velocity_rad_s,
                    o.motion.rudder_angular_velocity_rad_s,
                ],
            };
            let actual = simulation.control_dynamics(boat, environment);
            let predicted = future_loads(&o, state);
            assert!(
                (predicted.yaw
                    - actual.passive_yaw_moment_n_m
                    - actual.fluid_yaw_moments_n_m.iter().sum::<f64>())
                .abs()
                    < 1e-8
            );
            assert!((predicted.drive - actual.foil_forward_force_n).abs() < 1e-8);
            for j in 0..2 {
                assert!(
                    (predicted.generalized[j] - actual.fluid_generalized_torques_n_m[j]).abs()
                        < 1e-8
                );
            }
        }
    }

    #[test]
    fn inference_respects_known_bias_and_is_bounded() {
        let mut weights = [0.0; WEIGHTS];
        weights[WEIGHTS - 1] = 2.0;
        assert_eq!(network_output(&[0.0; INPUTS], &weights), [0.0, 2.0 / 3.0]);
        for sign in [-1.0, 1.0] {
            weights[WEIGHTS - 1] = sign * 1e100;
            let output = network_output(&[4.0; INPUTS], &weights);
            assert!(output.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        }
    }
}
