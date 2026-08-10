use pyboat_simulation::{
    BoatMotion, BoatParameters, ControlSettings, DT_SECONDS, Environment, HullParameters,
    MotionIntent, Simulation, SurfaceControlCommand, apparent_flow, hull_outline_m,
};
use rapier2d::prelude::{Rotation, Vector};

#[cfg(target_arch = "wasm32")]
mod web;

pub const VIEW_HEIGHT_M: f32 = 15.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatScene {
    pub hull_m: [Vector; 5],
    pub sail_m: [Vector; 2],
    pub rudder_m: [Vector; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointOfSail {
    InIrons,
    CloseHauled,
    BeamReach,
    BroadReach,
    Run,
}

impl PointOfSail {
    pub fn label(self) -> &'static str {
        match self {
            Self::InIrons => "in irons",
            Self::CloseHauled => "close-hauled",
            Self::BeamReach => "beam reach",
            Self::BroadReach => "broad reach",
            Self::Run => "run",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DemoView {
    pub speed_m_s: f32,
    pub apparent_wind_speed_m_s: f32,
    pub point_of_sail: PointOfSail,
}

pub struct Demo {
    parameters: BoatParameters,
    simulation: Simulation,
    motion_intent: MotionIntent,
    accumulator_seconds: f32,
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

impl Demo {
    pub fn new() -> Self {
        let parameters = demo_parameters();
        let simulation = Simulation::new(
            parameters,
            BoatMotion {
                position_m: Vector::ZERO,
                velocity_m_s: Vector::ZERO,
                angle_rad: 135.0_f32.to_radians(),
                angular_velocity_rad_s: 0.0,
                sail_angle_rad: 0.0,
                sail_angular_velocity_rad_s: 0.0,
                rudder_angle_rad: 0.0,
                rudder_angular_velocity_rad_s: 0.0,
            },
        );
        Self {
            parameters,
            simulation,
            motion_intent: MotionIntent::default(),
            accumulator_seconds: 0.0,
        }
    }

    pub fn set_motion_intent(&mut self, forward: f32, turn: f32) {
        self.motion_intent = MotionIntent {
            forward: forward.clamp(-1.0, 1.0),
            turn: turn.clamp(-1.0, 1.0),
        };
    }

    pub fn set_player_input(
        &mut self,
        keyboard_forward: f32,
        keyboard_turn: f32,
        gamepad_x: f32,
        gamepad_y: f32,
    ) {
        self.set_motion_intent(keyboard_forward, keyboard_turn);
        let world_motion = Vector::new(gamepad_x, gamepad_y);
        let length = world_motion.length();
        let magnitude = length.min(1.0);
        if magnitude == 0.0 {
            return;
        }

        let local_direction =
            Rotation::new(-self.motion().angle_rad).transform_vector(world_motion / length);
        let square_scale = local_direction.x.abs().max(local_direction.y.abs());
        self.motion_intent = MotionIntent {
            forward: local_direction.x / square_scale * magnitude,
            turn: local_direction.y / square_scale * magnitude,
        };
    }

    pub fn motion_intent(&self) -> MotionIntent {
        self.motion_intent
    }

    pub fn advance(&mut self, elapsed_seconds: f32) {
        self.accumulator_seconds += elapsed_seconds.min(0.25);
        while self.accumulator_seconds >= DT_SECONDS {
            self.simulation.advance_controlled(
                demo_environment(),
                SurfaceControlCommand::ComputedYawRate(self.motion_intent),
                ControlSettings {
                    angle_resolution_deg: 1,
                    sail_side_hysteresis_angle_rad: 10.0_f32.to_radians(),
                },
            );
            self.accumulator_seconds -= DT_SECONDS;
        }
    }

    pub fn motion(&self) -> BoatMotion {
        self.simulation.motion()
    }

    pub fn view(&self) -> DemoView {
        let motion = self.motion();
        let environment = demo_environment();
        let upwind_angle_rad = environment
            .true_wind_velocity_m_s
            .y
            .atan2(environment.true_wind_velocity_m_s.x)
            + std::f32::consts::PI;
        DemoView {
            speed_m_s: (motion.velocity_m_s - environment.true_water_velocity_m_s).length(),
            apparent_wind_speed_m_s: apparent_flow(
                environment.true_wind_velocity_m_s,
                motion.velocity_m_s,
            )
            .length(),
            point_of_sail: point_of_sail(motion.angle_rad - upwind_angle_rad),
        }
    }

    pub fn scene(&self) -> BoatScene {
        boat_scene(self.parameters, self.motion())
    }
}

pub fn point_of_sail(angle_from_upwind_rad: f32) -> PointOfSail {
    let positive_angle_rad = angle_from_upwind_rad.rem_euclid(std::f32::consts::TAU);
    let angle_rad = positive_angle_rad.min(std::f32::consts::TAU - positive_angle_rad);
    if angle_rad < 25.0_f32.to_radians() {
        PointOfSail::InIrons
    } else if angle_rad < 65.0_f32.to_radians() {
        PointOfSail::CloseHauled
    } else if angle_rad < 120.0_f32.to_radians() {
        PointOfSail::BeamReach
    } else if angle_rad < 155.0_f32.to_radians() {
        PointOfSail::BroadReach
    } else {
        PointOfSail::Run
    }
}

pub fn boat_scene(parameters: BoatParameters, motion: BoatMotion) -> BoatScene {
    let hull_rotation = Rotation::new(motion.angle_rad);
    let hull_m = hull_outline_m(parameters.hull)
        .map(|point| motion.position_m + hull_rotation.transform_vector(point));
    let mast_m = motion.position_m + hull_rotation.transform_vector(parameters.mast_position_m);
    let sail_end_m = mast_m
        + Rotation::new(motion.angle_rad + motion.sail_angle_rad)
            .transform_vector(Vector::new(-parameters.boom_length_m, 0.0));
    let rudder_hinge_m =
        motion.position_m + hull_rotation.transform_vector(parameters.rudder_position_m);
    let rudder_end_m = rudder_hinge_m
        + Rotation::new(motion.angle_rad + motion.rudder_angle_rad)
            .transform_vector(Vector::new(-parameters.rudder_draw_length_m, 0.0));
    BoatScene {
        hull_m,
        sail_m: [mast_m, sail_end_m],
        rudder_m: [rudder_hinge_m, rudder_end_m],
    }
}

fn demo_parameters() -> BoatParameters {
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

fn demo_environment() -> Environment {
    Environment {
        true_wind_velocity_m_s: Vector::new(6.0, 0.0),
        true_water_velocity_m_s: Vector::ZERO,
    }
}
