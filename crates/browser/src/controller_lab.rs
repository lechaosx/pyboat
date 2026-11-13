#[path = "controller_lab_learning.rs"]
mod learning;

use simulation::{
    BoatMotion, BoatParameters, ControlInput, DT_SECONDS, Environment, MotionIntent,
    SharedSimulation, SurfaceEffect, SurfaceTargets, computed_maximum_yaw_rate_rad_s,
    surface_control_input, surface_control_input_about_point, yaw_rate_move_control,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum LabControllerKind {
    Current,
    Physical,
    Classical,
    Learned = 4,
    CurrentRefined = 5,
}

impl LabControllerKind {
    pub const ALL: [Self; 5] = [
        Self::Current,
        Self::Physical,
        Self::Classical,
        Self::Learned,
        Self::CurrentRefined,
    ];
}

#[derive(Clone, Debug)]
pub struct LabController {
    kind: LabControllerKind,
    classical: ClassicalState,
}

#[derive(Clone, Debug, Default)]
struct ClassicalState {
    integral_rad_s2: f64,
    previous_rate_rad_s: Option<f64>,
    filtered_acceleration_rad_s2: f64,
}

pub(super) struct LabObservation {
    pub parameters: BoatParameters,
    pub motion: BoatMotion,
    pub environment: Environment,
    pub intent: MotionIntent,
    pub input: ControlInput,
    pub maximum_yaw_rate_rad_s: f64,
    pub locked_inertia_kg_m2: f64,
    pub total_mass_kg: f64,
    pub sail_hinge_inertia_kg_m2: f64,
    pub rudder_hinge_inertia_kg_m2: f64,
    pub fluid_hinge_torques_n_m: [f64; 2],
    pub actual_fluid_yaw_moments_n_m: [f64; 2],
    pub passive_yaw_moment_n_m: f64,
}

impl LabController {
    pub fn new(kind: LabControllerKind) -> Self {
        Self {
            kind,
            classical: ClassicalState::default(),
        }
    }

    pub fn kind(&self) -> LabControllerKind {
        self.kind
    }

    pub fn select(&mut self, kind: LabControllerKind) {
        if self.kind != kind {
            self.kind = kind;
            self.reset();
        }
    }

    pub fn reset(&mut self) {
        self.classical = ClassicalState::default();
    }

    pub fn targets(
        &mut self,
        simulation: &SharedSimulation,
        boat: usize,
        parameters: BoatParameters,
        environment: Environment,
        intent: MotionIntent,
    ) -> SurfaceTargets {
        let motion = simulation.motion(boat);
        let maximum_yaw_rate_rad_s =
            computed_maximum_yaw_rate_rad_s(parameters, motion, environment);
        let hysteresis = f64::from(10.0_f32.to_radians());
        if self.kind == LabControllerKind::Current {
            let input = surface_control_input(parameters, motion, environment, 1, hysteresis);
            return yaw_rate_move_control(&input, intent, maximum_yaw_rate_rad_s);
        }
        let dynamics = simulation.control_dynamics(boat, environment);
        let reference = dynamics.center_of_mass_from_hull_m;
        let mut input = surface_control_input_about_point(
            parameters,
            motion,
            environment,
            1,
            hysteresis,
            reference,
        );
        if self.kind == LabControllerKind::CurrentRefined && intent.forward < 0.0 {
            // Reverse and forward power share the same side-retained zero-power origin.
            input.free_sail_angle_rad =
                yaw_rate_move_control(&input, MotionIntent::default(), maximum_yaw_rate_rad_s)
                    .sheet_target_angle_rad;
        }
        let observation = LabObservation {
            parameters,
            motion,
            environment,
            intent,
            input,
            maximum_yaw_rate_rad_s,
            locked_inertia_kg_m2: dynamics.locked_inertia_kg_m2,
            total_mass_kg: dynamics.total_mass_kg,
            sail_hinge_inertia_kg_m2: dynamics.sail_hinge_inertia_kg_m2,
            rudder_hinge_inertia_kg_m2: dynamics.rudder_hinge_inertia_kg_m2,
            fluid_hinge_torques_n_m: dynamics.fluid_hinge_torques_n_m,
            actual_fluid_yaw_moments_n_m: dynamics.fluid_yaw_moments_n_m,
            passive_yaw_moment_n_m: dynamics.passive_yaw_moment_n_m,
        };
        let mut targets = match self.kind {
            LabControllerKind::Classical => classical_targets(&observation, &mut self.classical),
            LabControllerKind::Learned => learning::learned_targets(&observation),
            LabControllerKind::Physical | LabControllerKind::CurrentRefined => {
                physical_rudder_targets(&observation)
            }
            LabControllerKind::Current => unreachable!(),
        };
        targets.sheet_target_angle_rad = targets.sheet_target_angle_rad.clamp(
            -parameters.max_sail_angle_rad,
            parameters.max_sail_angle_rad,
        );
        targets.sail_target_angle_rad = targets.sail_target_angle_rad.map(|angle| {
            angle.clamp(
                -parameters.max_sail_angle_rad,
                parameters.max_sail_angle_rad,
            )
        });
        targets.rudder_target_angle_rad = targets.rudder_target_angle_rad.clamp(
            -parameters.max_rudder_angle_rad,
            parameters.max_rudder_angle_rad,
        );
        targets
    }
}

pub(super) fn interpolated_effect(effects: &[SurfaceEffect], angle: f64) -> SurfaceEffect {
    let angle = angle.clamp(effects[0].angle_rad, effects.last().unwrap().angle_rad);
    for pair in effects.windows(2) {
        if angle <= pair[1].angle_rad {
            let fraction = (angle - pair[0].angle_rad) / (pair[1].angle_rad - pair[0].angle_rad);
            return SurfaceEffect {
                angle_rad: angle,
                drive_force_n: pair[0].drive_force_n
                    + fraction * (pair[1].drive_force_n - pair[0].drive_force_n),
                yaw_moment_n_m: pair[0].yaw_moment_n_m
                    + fraction * (pair[1].yaw_moment_n_m - pair[0].yaw_moment_n_m),
            };
        }
    }
    *effects.last().unwrap()
}

fn legacy_sail_effect(observation: &LabObservation, power: f64) -> SurfaceEffect {
    let targets = yaw_rate_move_control(
        &observation.input,
        MotionIntent {
            forward: power,
            turn: observation.intent.turn,
        },
        observation.maximum_yaw_rate_rad_s,
    );
    let angle = targets
        .sail_target_angle_rad
        .unwrap_or(targets.sheet_target_angle_rad);
    observation
        .input
        .sail_effects
        .iter()
        .copied()
        .reduce(|selected, effect| {
            if (effect.angle_rad - angle).abs() + 1e-6 < (selected.angle_rad - angle).abs() {
                effect
            } else {
                selected
            }
        })
        .unwrap()
}

pub(super) fn requested_drive_force(observation: &LabObservation) -> f64 {
    let power = observation.intent.forward.clamp(-1.0, 1.0);
    let free = legacy_sail_effect(observation, 0.0).drive_force_n;
    let end = legacy_sail_effect(observation, power.signum()).drive_force_n;
    free + power.abs() * (end - free)
        + interpolated_effect(&observation.input.rudder_effects, 0.0).drive_force_n
}

fn requested_total_moment(observation: &LabObservation) -> f64 {
    let sail = legacy_sail_effect(observation, observation.intent.forward.clamp(-1.0, 1.0));
    let mut error = observation.intent.turn.clamp(-1.0, 1.0);
    if observation.motion.angular_velocity_rad_s != 0.0 {
        error -= observation.motion.angular_velocity_rad_s / observation.maximum_yaw_rate_rad_s;
    }
    error = error.clamp(-1.0, 1.0);
    if error < 0.0 {
        -error
            * observation
                .input
                .rudder_effects
                .iter()
                .map(|rudder| rudder.yaw_moment_n_m + sail.yaw_moment_n_m)
                .fold(0.0, f64::min)
    } else {
        error
            * observation
                .input
                .rudder_effects
                .iter()
                .map(|rudder| rudder.yaw_moment_n_m + sail.yaw_moment_n_m)
                .fold(0.0, f64::max)
    }
}

struct JointObjective {
    drive: f64,
    moment: f64,
    force_weight: f64,
    moment_weight: f64,
    movement_weights: [f64; 2],
    actual: [f64; 2],
}

impl JointObjective {
    fn cost(&self, sail: SurfaceEffect, rudder: SurfaceEffect) -> f64 {
        self.force_weight * (sail.drive_force_n + rudder.drive_force_n - self.drive).powi(2)
            + self.moment_weight
                * (sail.yaw_moment_n_m + rudder.yaw_moment_n_m - self.moment).powi(2)
            + self.movement_weights[0] * (sail.angle_rad - self.actual[0]).powi(2)
            + self.movement_weights[1] * (rudder.angle_rad - self.actual[1]).powi(2)
    }

    fn best_angle(&self, effects: &[SurfaceEffect], other: SurfaceEffect, surface: usize) -> f64 {
        let mut best = self.actual[surface];
        let actual = interpolated_effect(effects, best);
        let mut best_cost = if surface == 0 {
            self.cost(actual, other)
        } else {
            self.cost(other, actual)
        };
        for pair in effects.windows(2) {
            let left = pair[0];
            let span = pair[1].angle_rad - left.angle_rad;
            let force_slope = (pair[1].drive_force_n - left.drive_force_n) / span;
            let moment_slope = (pair[1].yaw_moment_n_m - left.yaw_moment_n_m) / span;
            let weight = self.movement_weights[surface];
            let denominator = self.force_weight * force_slope.powi(2)
                + self.moment_weight * moment_slope.powi(2)
                + weight;
            let offset = (self.force_weight
                * force_slope
                * (self.drive - other.drive_force_n - left.drive_force_n)
                + self.moment_weight
                    * moment_slope
                    * (self.moment - other.yaw_moment_n_m - left.yaw_moment_n_m)
                + weight * (self.actual[surface] - left.angle_rad))
                / denominator;
            let angle = (left.angle_rad + offset).clamp(left.angle_rad, pair[1].angle_rad);
            let fraction = (angle - left.angle_rad) / span;
            let effect = SurfaceEffect {
                angle_rad: angle,
                drive_force_n: left.drive_force_n
                    + fraction * (pair[1].drive_force_n - left.drive_force_n),
                yaw_moment_n_m: left.yaw_moment_n_m
                    + fraction * (pair[1].yaw_moment_n_m - left.yaw_moment_n_m),
            };
            let cost = if surface == 0 {
                self.cost(effect, other)
            } else {
                self.cost(other, effect)
            };
            if cost < best_cost
                || (cost == best_cost
                    && (angle - self.actual[surface]).abs() < (best - self.actual[surface]).abs())
            {
                best = angle;
                best_cost = cost;
            }
        }
        best
    }
}

pub(super) fn joint_targets(observation: &LabObservation, moment: f64) -> SurfaceTargets {
    // These energy-dimensioned terms express a control preference, not measured hand work.
    const TRADEOFF_SECONDS: f64 = 0.25;
    let time_squared = TRADEOFF_SECONDS.powi(2);
    let objective = JointObjective {
        drive: requested_drive_force(observation),
        moment,
        force_weight: time_squared / observation.total_mass_kg,
        moment_weight: time_squared / observation.locked_inertia_kg_m2,
        movement_weights: [
            observation.sail_hinge_inertia_kg_m2 / time_squared,
            observation.rudder_hinge_inertia_kg_m2 / time_squared,
        ],
        actual: [
            observation.motion.sail_angle_rad,
            observation.motion.rudder_angle_rad,
        ],
    };
    let input = &observation.input;
    let actual = interpolated_effect(&input.sail_effects, observation.motion.sail_angle_rad);
    let trim = legacy_sail_effect(observation, observation.intent.forward.clamp(-1.0, 1.0));
    let mut best = [actual.angle_rad, observation.motion.rudder_angle_rad];
    let mut best_cost = f64::INFINITY;
    // A bounded coarse scan captures competing sail branches; coordinate refinements remove grid steps.
    for sail in input
        .sail_effects
        .iter()
        .step_by(5)
        .copied()
        .chain([actual, trim])
    {
        let rudder_angle = objective.best_angle(&input.rudder_effects, sail, 1);
        let rudder = interpolated_effect(&input.rudder_effects, rudder_angle);
        let cost = objective.cost(sail, rudder);
        if cost < best_cost {
            best = [sail.angle_rad, rudder_angle];
            best_cost = cost;
        }
    }
    for _ in 0..2 {
        let rudder = interpolated_effect(&input.rudder_effects, best[1]);
        best[0] = objective.best_angle(&input.sail_effects, rudder, 0);
        let sail = interpolated_effect(&input.sail_effects, best[0]);
        best[1] = objective.best_angle(&input.rudder_effects, sail, 1);
    }
    SurfaceTargets {
        sheet_target_angle_rad: best[0].clamp(
            -observation.parameters.max_sail_angle_rad,
            observation.parameters.max_sail_angle_rad,
        ),
        rudder_target_angle_rad: best[1].clamp(
            -observation.parameters.max_rudder_angle_rad,
            observation.parameters.max_rudder_angle_rad,
        ),
        sail_target_angle_rad: Some(best[0].clamp(
            -observation.parameters.max_sail_angle_rad,
            observation.parameters.max_sail_angle_rad,
        )),
    }
}

pub(super) fn physical_targets(observation: &LabObservation) -> SurfaceTargets {
    // The learned weights are calibrated against this joint policy.
    joint_targets(observation, requested_total_moment(observation))
}

fn physical_rudder_targets(observation: &LabObservation) -> SurfaceTargets {
    let mut targets = yaw_rate_move_control(
        &observation.input,
        observation.intent,
        observation.maximum_yaw_rate_rad_s,
    );
    let sail = legacy_sail_effect(observation, observation.intent.forward.clamp(-1.0, 1.0));
    // Penalizing sail displacement from its loaded angle can prevent useful trim indefinitely.
    // The rudder's energy-dimensioned preference is not measured hand work or physical resistance.
    const TRADEOFF_SECONDS: f64 = 0.25;
    targets.rudder_target_angle_rad = allocate_moment(
        &observation.input,
        observation.motion.rudder_angle_rad,
        requested_total_moment(observation) - sail.yaw_moment_n_m,
        observation.locked_inertia_kg_m2,
        observation.rudder_hinge_inertia_kg_m2,
        TRADEOFF_SECONDS,
    );
    targets
}

fn classical_targets(observation: &LabObservation, state: &mut ClassicalState) -> SurfaceTargets {
    // A global 0.5 s scale improved tracking and matched-state release over 1 s and 2 s.
    const RESPONSE_SECONDS: f64 = 0.5;
    const INTEGRAL_SECONDS: f64 = 2.0 * RESPONSE_SECONDS;
    const DERIVATIVE_SECONDS: f64 = 0.1 * RESPONSE_SECONDS;
    const FILTER_SECONDS: f64 = 0.1;
    let rate = observation.motion.angular_velocity_rad_s;
    let measured_acceleration = state
        .previous_rate_rad_s
        .map_or(0.0, |previous| (rate - previous) / DT_SECONDS);
    state.previous_rate_rad_s = Some(rate);
    state.filtered_acceleration_rad_s2 += DT_SECONDS / (FILTER_SECONDS + DT_SECONDS)
        * (measured_acceleration - state.filtered_acceleration_rad_s2);
    let requested_rate =
        observation.intent.turn.clamp(-1.0, 1.0) * observation.maximum_yaw_rate_rad_s;
    let error = requested_rate - rate;
    let acceleration = error / RESPONSE_SECONDS + state.integral_rad_s2
        - DERIVATIVE_SECONDS / RESPONSE_SECONDS * state.filtered_acceleration_rad_s2;
    let moment =
        observation.locked_inertia_kg_m2 * acceleration - observation.passive_yaw_moment_n_m;
    let targets = joint_targets(observation, moment);
    // Actual fluid moments include sweep. This tracks available model response, not measured motor saturation.
    let delivered_acceleration = (observation.actual_fluid_yaw_moments_n_m.iter().sum::<f64>()
        + observation.passive_yaw_moment_n_m)
        / observation.locked_inertia_kg_m2;
    state.integral_rad_s2 += DT_SECONDS
        * (error / (RESPONSE_SECONDS * INTEGRAL_SECONDS)
            + (delivered_acceleration - acceleration) / RESPONSE_SECONDS);
    targets
}

fn allocate_moment(
    input: &ControlInput,
    actual_angle: f64,
    requested_rudder_moment: f64,
    locked_inertia: f64,
    hinge_inertia: f64,
    tradeoff_seconds: f64,
) -> f64 {
    let squared_time = tradeoff_seconds.powi(2);
    let objective = JointObjective {
        drive: 0.0,
        moment: requested_rudder_moment,
        force_weight: 0.0,
        moment_weight: squared_time / locked_inertia,
        movement_weights: [0.0, hinge_inertia / squared_time],
        actual: [0.0, actual_angle],
    };
    objective.best_angle(
        &input.rudder_effects,
        SurfaceEffect {
            angle_rad: 0.0,
            drive_force_n: 0.0,
            yaw_moment_n_m: 0.0,
        },
        1,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapier2d_f64::prelude::Vector;

    #[test]
    fn allocation_balances_tracking_against_physical_movement_scale() {
        let mut input = surface_control_input(
            game::dinghy_parameters(),
            BoatMotion {
                position_m: Vector::ZERO,
                velocity_m_s: Vector::ZERO,
                angle_rad: 0.0,
                angular_velocity_rad_s: 0.0,
                sail_angle_rad: 0.0,
                sail_angular_velocity_rad_s: 0.0,
                rudder_angle_rad: 0.0,
                rudder_angular_velocity_rad_s: 0.0,
            },
            Environment::default(),
            1,
            0.1,
        );
        input.rudder_effects = vec![
            SurfaceEffect {
                angle_rad: -1.0,
                drive_force_n: 0.0,
                yaw_moment_n_m: -1.0,
            },
            SurfaceEffect {
                angle_rad: 1.0,
                drive_force_n: 0.0,
                yaw_moment_n_m: 1.0,
            },
        ];
        assert_eq!(allocate_moment(&input, 0.0, 1.0, 1.0, 1.0, 1.0), 0.5);
        for effect in &mut input.rudder_effects {
            effect.yaw_moment_n_m = 0.0;
        }
        assert_eq!(allocate_moment(&input, 0.3, 100.0, 1.0, 1.0, 1.0), 0.3);
    }
    #[test]
    fn classical_model_tracking_decays_unavailable_turn_memory_after_release() {
        let mut parameters = game::dinghy_parameters();
        parameters.air_density_kg_m3 = 0.0;
        parameters.water_density_kg_m3 = 0.0;
        let mut simulation = SharedSimulation::new();
        let motion = BoatMotion {
            position_m: Vector::ZERO,
            velocity_m_s: Vector::new(2.0, 0.0),
            angle_rad: 0.0,
            angular_velocity_rad_s: 0.0,
            sail_angle_rad: 0.0,
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: 0.0,
            rudder_angular_velocity_rad_s: 0.0,
        };
        simulation.add_boat(parameters, motion);
        let mut controller = LabController::new(LabControllerKind::Classical);
        for _ in 0..600 {
            controller.targets(
                &simulation,
                0,
                parameters,
                Environment::default(),
                MotionIntent {
                    forward: 0.0,
                    turn: 1.0,
                },
            );
        }
        let at_release = controller.classical.integral_rad_s2;
        assert!(at_release.abs() > 0.1);
        for _ in 0..600 {
            controller.targets(
                &simulation,
                0,
                parameters,
                Environment::default(),
                MotionIntent::default(),
            );
        }
        let expected = at_release * (1.0 - DT_SECONDS / 0.5).powi(600);
        assert!((controller.classical.integral_rad_s2 - expected).abs() < expected.abs() * 1e-12);
    }
}
