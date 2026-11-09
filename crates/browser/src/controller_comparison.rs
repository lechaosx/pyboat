use game::{computed_yaw_rate_targets_in_environment, dinghy_parameters, match_environment};
use rapier2d_f64::prelude::{Rotation, Vector};
use simulation::{
    BoatMotion, DT_SECONDS, Environment, MotionIntent, SharedSimulation, SurfaceTargets,
    computed_maximum_yaw_rate_rad_s, continuous_yaw_rate_move_control, surface_control_input,
};

use crate::{BoatScene, boat_scene, player_motion_intent};

pub struct ControllerComparison {
    simulations: [SharedSimulation; 2],
    intents: [MotionIntent; 2],
    targets: [SurfaceTargets; 2],
    input: [f32; 4],
    control_mask: u32,
    preset: u32,
    tick: u64,
    accumulator_seconds: f64,
}

impl ControllerComparison {
    pub fn new(preset: u32) -> Self {
        let motion = BoatMotion {
            position_m: Vector::ZERO,
            velocity_m_s: Vector::ZERO,
            angle_rad: std::f64::consts::PI,
            angular_velocity_rad_s: 0.0001,
            sail_angle_rad: 0.0,
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: 0.0,
            rudder_angular_velocity_rad_s: 0.0,
        };
        let mut motion = motion;
        if preset == 1 || preset == 2 {
            motion.angle_rad = 135.0_f64.to_radians();
            motion.angular_velocity_rad_s = 0.0;
            motion.velocity_m_s = Rotation::new(motion.angle_rad)
                .transform_vector(Vector::new(if preset == 2 { -1.0 } else { 2.0 }, 0.0));
        }
        let simulations = std::array::from_fn(|_| {
            let mut simulation = SharedSimulation::new();
            simulation.add_boat(dinghy_parameters(), motion);
            simulation
        });
        let targets = computed_yaw_rate_targets_in_environment(
            motion,
            MotionIntent::default(),
            match_environment(),
        );
        let mut comparison = Self {
            simulations,
            intents: [MotionIntent::default(); 2],
            targets: [targets; 2],
            input: [0.0; 4],
            control_mask: 3,
            preset,
            tick: 0,
            accumulator_seconds: 0.0,
        };
        comparison.update_controls();
        comparison
    }

    pub fn set_player_input(&mut self, input: [f32; 4], control_mask: u32) {
        self.input = input;
        self.control_mask = control_mask;
    }

    pub fn advance(&mut self, elapsed_seconds: f64, playing: bool) {
        if playing {
            self.accumulator_seconds += elapsed_seconds.clamp(0.0, 0.25);
            while self.accumulator_seconds >= DT_SECONDS {
                self.update_controls();
                let environment = self.environment();
                for side in 0..2 {
                    self.simulations[side].advance_controlled_with_environments(
                        &[environment],
                        &[Some(self.targets[side])],
                    );
                }
                self.tick += 1;
                self.accumulator_seconds -= DT_SECONDS;
            }
        }
    }

    fn update_controls(&mut self) {
        let environment = self.environment();
        let parameters = dinghy_parameters();
        for side in 0..2 {
            let motion = self.motion(side);
            let input = if self.control_mask & (1 << side) != 0 {
                self.input
            } else {
                [0.0; 4]
            };
            let intent = player_motion_intent(
                Some(motion.angle_rad),
                input[0],
                input[1],
                input[2],
                input[3],
            );
            self.intents[side] = intent;
            self.targets[side] = if side == 0 {
                computed_yaw_rate_targets_in_environment(motion, intent, environment)
            } else {
                let control_input = surface_control_input(
                    parameters,
                    motion,
                    environment,
                    1,
                    f64::from(10.0_f32.to_radians()),
                );
                continuous_yaw_rate_move_control(
                    &control_input,
                    intent,
                    computed_maximum_yaw_rate_rad_s(parameters, motion, environment),
                )
            };
        }
    }

    pub fn motion(&self, side: usize) -> BoatMotion {
        self.simulations[side].motion(0)
    }

    pub fn intent(&self, side: usize) -> MotionIntent {
        self.intents[side]
    }

    pub fn targets(&self, side: usize) -> SurfaceTargets {
        self.targets[side]
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn environment(&self) -> Environment {
        let mut environment = match_environment();
        if self.preset == 3 {
            environment.true_water_velocity_m_s = Vector::new(0.05, 0.02);
        } else if self.preset == 4 {
            let time = self.tick as f64 * DT_SECONDS;
            environment.true_wind_velocity_m_s =
                Vector::new(6.0 + phase_sine(time * 0.7), 0.8 * phase_sine(time * 0.37));
            environment.true_water_velocity_m_s = Vector::new(
                0.05 * phase_sine(time * 0.41),
                0.04 * phase_sine(time * 0.59),
            );
        }
        environment
    }

    pub fn scene(&self, side: usize, position_m: crate::Vector) -> BoatScene {
        let mut motion = self.motion(side);
        motion.position_m = position_m.as_dvec2();
        let mut scene = boat_scene(dinghy_parameters(), motion);
        scene.id = Some(format!("controller-comparison-{side}"));
        scene.is_local = true;
        scene.apparent_wind_velocity_m_s =
            (self.environment().true_wind_velocity_m_s - motion.velocity_m_s).as_vec2();
        scene.windsock = Some(crate::Windsock {
            anchor_m: scene.hull_m[0],
            apparent_wind_velocity_m_s: scene.apparent_wind_velocity_m_s,
        });
        scene
    }
}

fn phase_sine(angle: f64) -> f64 {
    // Rapier's enhanced-determinism math also supplies the browser's native parity.
    Rotation::new(angle).transform_vector(Vector::X).y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_mapping_preserves_fractional_radius_and_exact_zero_fallback() {
        let intent = player_motion_intent(Some(0.0), 1.0, -1.0, 0.0, 0.0);
        assert_eq!(
            intent,
            MotionIntent {
                forward: 1.0,
                turn: -1.0
            }
        );
        let intent = player_motion_intent(Some(0.0), 1.0, -1.0, 0.0, 0.125);
        assert_eq!(
            intent,
            MotionIntent {
                forward: 0.0,
                turn: 0.125
            }
        );
        let intent = player_motion_intent(Some(0.0), 1.0, -1.0, 0.000_001, 0.0);
        assert_eq!(
            intent,
            MotionIntent {
                forward: f64::from(0.000_001_f32),
                turn: 0.0
            }
        );
        let intent = player_motion_intent(None, 1.0, -1.0, 0.5, 0.5);
        assert_eq!(
            intent,
            MotionIntent {
                forward: 1.0,
                turn: -1.0
            }
        );
        for radius in [0.1_f32, 0.5, 1.0] {
            let component = radius / 2.0_f32.sqrt();
            let intent = player_motion_intent(Some(0.0), 0.0, 0.0, component, component);
            assert!((intent.forward - f64::from(radius)).abs() < 1e-7);
            assert_eq!(intent.forward, intent.turn);
            let rotated =
                player_motion_intent(Some(std::f64::consts::FRAC_PI_2), 0.0, 0.0, 0.0, radius);
            assert!((rotated.forward - f64::from(radius)).abs() < 1e-7);
            assert!(rotated.turn.abs() < 1e-7);
        }
    }

    #[test]
    fn reference_is_the_production_controller_and_physics() {
        let mut comparison = ControllerComparison::new(1);
        let mut reference = SharedSimulation::new();
        reference.add_boat(dinghy_parameters(), comparison.motion(0));
        comparison.set_player_input([0.5, 0.2, 0.0, 0.0], 3);
        for _ in 0..120 {
            let targets = computed_yaw_rate_targets_in_environment(
                reference.motion(0),
                MotionIntent {
                    forward: 0.5,
                    turn: f64::from(0.2_f32),
                },
                match_environment(),
            );
            reference
                .advance_controlled_with_environments(&[match_environment()], &[Some(targets)]);
            comparison.advance(DT_SECONDS, true);
            assert_eq!(comparison.motion(0), reference.motion(0));
        }
    }

    #[test]
    fn held_world_stick_is_mapped_each_tick_independent_of_frame_grouping() {
        let mut fine = ControllerComparison::new(1);
        let mut coarse = ControllerComparison::new(1);
        fine.set_player_input([0.0, 0.0, -0.4, 0.4], 3);
        coarse.set_player_input([0.0, 0.0, -0.4, 0.4], 3);
        for _ in 0..60 {
            fine.advance(DT_SECONDS, true);
            fine.advance(DT_SECONDS, true);
            coarse.advance(DT_SECONDS * 2.0, true);
            assert_eq!(fine.tick(), coarse.tick());
            for side in 0..2 {
                assert_eq!(fine.motion(side), coarse.motion(side));
                assert_eq!(fine.intent(side), coarse.intent(side));
            }
        }
    }

    #[test]
    fn controlling_one_side_does_not_change_the_other_simulation() {
        let mut controlled = ControllerComparison::new(1);
        let mut neutral = ControllerComparison::new(1);
        controlled.set_player_input([1.0, 1.0, 0.0, 0.0], 1);
        for _ in 0..60 {
            controlled.advance(DT_SECONDS, true);
            neutral.advance(DT_SECONDS, true);
            assert_eq!(controlled.motion(1), neutral.motion(1));
        }
        assert_ne!(controlled.motion(0), neutral.motion(0));
    }

    #[test]
    fn pause_preserves_both_physical_states_and_clock() {
        let mut comparison = ControllerComparison::new(4);
        let initial = comparison.motion(0);
        comparison.set_player_input([1.0, 1.0, 0.0, 0.0], 3);
        comparison.advance(1.0, false);
        assert_eq!(comparison.tick(), 0);
        assert_eq!(comparison.motion(0), initial);
        assert_eq!(comparison.motion(1), initial);
        comparison.advance(DT_SECONDS, true);
        assert_eq!(comparison.tick(), 1);
    }
}
