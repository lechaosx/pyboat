use std::collections::HashMap;

use pyboat_protocol::{BoatSnapshot, ClientMessage, ServerMessage};
use pyboat_simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, DT_SECONDS, Environment, HullContact,
    HullParameters, MotionIntent, SharedSimulation, apparent_flow, computed_maximum_yaw_rate_rad_s,
    hull_outline_m, surface_control_input, surface_target_control_torques, yaw_rate_move_control,
};
use rapier2d::prelude::{Rotation, Vector};

#[cfg(target_arch = "wasm32")]
mod web;

pub const VIEW_HEIGHT_M: f32 = 15.0;
const MAXIMUM_PREDICTION_SECONDS: f32 = 0.25;
const MAXIMUM_REPLAY_COMMANDS: usize = 15;
const CORRECTION_SECONDS: f32 = 0.1;

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

#[derive(Clone, Debug, PartialEq)]
pub struct ReplicatedBoat {
    pub snapshot: BoatSnapshot,
    pub stale: bool,
}

#[derive(Clone, Copy)]
struct InputCommand {
    sequence: i64,
    intent: MotionIntent,
}

#[derive(Clone, Copy)]
struct PredictedBoat {
    index: usize,
    intent: MotionIntent,
}

#[derive(Clone, Copy)]
struct MotionCorrection {
    position_m: Vector,
    velocity_m_s: Vector,
    angle_rad: f32,
    angular_velocity_rad_s: f32,
    sail_angle_rad: f32,
    sail_angular_velocity_rad_s: f32,
    rudder_angle_rad: f32,
    rudder_angular_velocity_rad_s: f32,
}

pub struct MultiplayerClient {
    player_id: Option<String>,
    boats: HashMap<String, ReplicatedBoat>,
    snapshot_received_at_seconds: HashMap<String, f32>,
    tick: Option<u64>,
    motion_intent: MotionIntent,
    next_input_sequence: i64,
    input_history: Vec<InputCommand>,
    input_accumulator_seconds: f32,
    time_seconds: f32,
    prediction: SharedSimulation,
    predicted_boats: HashMap<String, PredictedBoat>,
    corrections: HashMap<String, MotionCorrection>,
    correction_started_at_seconds: f32,
}

impl Default for MultiplayerClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiplayerClient {
    pub fn new() -> Self {
        Self {
            player_id: None,
            boats: HashMap::new(),
            snapshot_received_at_seconds: HashMap::new(),
            tick: None,
            motion_intent: MotionIntent::default(),
            next_input_sequence: 1,
            input_history: Vec::new(),
            input_accumulator_seconds: 0.0,
            time_seconds: 0.0,
            prediction: SharedSimulation::new(),
            predicted_boats: HashMap::new(),
            corrections: HashMap::new(),
            correction_started_at_seconds: 0.0,
        }
    }

    pub fn accept(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome { player_id } => self.player_id = Some(player_id),
            ServerMessage::Snapshot {
                tick,
                boats,
                removed_player_ids,
            } => {
                if self.tick.is_some_and(|current| tick < current) {
                    return;
                }
                let previous_motion = self
                    .boats
                    .keys()
                    .filter_map(|player_id| {
                        self.render_motion(player_id)
                            .map(|motion| (player_id.clone(), motion))
                    })
                    .collect::<HashMap<_, _>>();
                for boat in self.boats.values_mut() {
                    boat.stale = true;
                }
                for player_id in removed_player_ids {
                    self.boats.remove(&player_id);
                    self.snapshot_received_at_seconds.remove(&player_id);
                }
                let visible_player_ids = boats
                    .iter()
                    .map(|snapshot| snapshot.id.clone())
                    .collect::<Vec<_>>();
                for snapshot in boats {
                    self.snapshot_received_at_seconds
                        .insert(snapshot.id.clone(), self.time_seconds);
                    self.boats.insert(
                        snapshot.id.clone(),
                        ReplicatedBoat {
                            snapshot,
                            stale: false,
                        },
                    );
                }
                self.tick = Some(tick);
                self.rebuild_prediction(&visible_player_ids);
                self.start_correction(previous_motion, &visible_player_ids);
            }
        }
    }

    pub fn player_id(&self) -> Option<&str> {
        self.player_id.as_deref()
    }

    pub fn tick(&self) -> Option<u64> {
        self.tick
    }

    pub fn boat(&self, player_id: &str) -> Option<&ReplicatedBoat> {
        self.boats.get(player_id)
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
        let Some(local_motion) = self.local_motion() else {
            return;
        };

        let local_direction =
            Rotation::new(-local_motion.angle_rad).transform_vector(world_motion / length);
        let square_scale = local_direction.x.abs().max(local_direction.y.abs());
        self.motion_intent = MotionIntent {
            forward: local_direction.x / square_scale * magnitude,
            turn: local_direction.y / square_scale * magnitude,
        };
    }

    pub fn take_inputs(&mut self, elapsed_seconds: f32) -> Vec<ClientMessage> {
        let elapsed_seconds = elapsed_seconds.max(0.0);
        self.time_seconds += elapsed_seconds;
        if self.player_id.is_none() {
            self.input_accumulator_seconds = 0.0;
            return Vec::new();
        }
        self.input_accumulator_seconds += elapsed_seconds.min(MAXIMUM_PREDICTION_SECONDS);
        let mut inputs = Vec::new();
        while self.input_accumulator_seconds >= DT_SECONDS {
            let command = InputCommand {
                sequence: self.next_input_sequence,
                intent: self.motion_intent,
            };
            self.input_history.push(command);
            inputs.push(ClientMessage::Input {
                sequence: command.sequence,
                forward: command.intent.forward,
                turn: command.intent.turn,
            });
            self.next_input_sequence += 1;
            self.advance_prediction(command.intent);
            self.input_accumulator_seconds -= DT_SECONDS;
        }
        inputs
    }

    pub fn camera_m(&self) -> Option<Vector> {
        Some(self.local_motion()?.position_m)
    }

    pub fn view(&self) -> Option<DemoView> {
        let motion = self.local_motion()?;
        let environment = demo_environment();
        let upwind_angle_rad = environment
            .true_wind_velocity_m_s
            .y
            .atan2(environment.true_wind_velocity_m_s.x)
            + std::f32::consts::PI;
        Some(DemoView {
            speed_m_s: (motion.velocity_m_s - environment.true_water_velocity_m_s).length(),
            apparent_wind_speed_m_s: apparent_flow(
                environment.true_wind_velocity_m_s,
                motion.velocity_m_s,
            )
            .length(),
            point_of_sail: point_of_sail(motion.angle_rad - upwind_angle_rad),
        })
    }

    pub fn scenes(&self) -> Vec<BoatScene> {
        let mut boats = self
            .boats
            .values()
            .filter(|boat| !boat.stale)
            .collect::<Vec<_>>();
        boats.sort_by(|first, second| {
            let first_is_local = self.player_id() == Some(first.snapshot.id.as_str());
            let second_is_local = self.player_id() == Some(second.snapshot.id.as_str());
            second_is_local
                .cmp(&first_is_local)
                .then_with(|| first.snapshot.id.cmp(&second.snapshot.id))
        });
        boats
            .into_iter()
            .filter_map(|boat| {
                self.render_motion(&boat.snapshot.id)
                    .map(|motion| boat_scene(demo_parameters(), motion))
            })
            .collect()
    }

    fn local_motion(&self) -> Option<BoatMotion> {
        let player_id = self.player_id.as_ref()?;
        self.render_motion(player_id)
    }

    fn rebuild_prediction(&mut self, visible_player_ids: &[String]) {
        let Some(local_id) = self.player_id.as_ref() else {
            return;
        };
        let Some(local) = self.boats.get(local_id) else {
            return;
        };
        let Some(acknowledged_input) = local.snapshot.acknowledged_input else {
            return;
        };

        let mut prediction = SharedSimulation::new();
        let mut predicted_boats = HashMap::new();
        for player_id in visible_player_ids {
            let boat = &self.boats[player_id];
            let (Some(forward), Some(turn)) = (boat.snapshot.forward, boat.snapshot.turn) else {
                continue;
            };
            let index = prediction.add_boat(demo_parameters(), snapshot_motion(&boat.snapshot));
            predicted_boats.insert(
                player_id.clone(),
                PredictedBoat {
                    index,
                    intent: MotionIntent { forward, turn },
                },
            );
        }
        self.prediction = prediction;
        self.predicted_boats = predicted_boats;

        let replay = self
            .input_history
            .iter()
            .filter(|command| command.sequence > acknowledged_input)
            .rev()
            .take(MAXIMUM_REPLAY_COMMANDS)
            .copied()
            .collect::<Vec<_>>();
        for command in replay.into_iter().rev() {
            self.advance_prediction(command.intent);
        }
        self.input_history
            .retain(|command| command.sequence > acknowledged_input);
    }

    fn advance_prediction(&mut self, local_intent: MotionIntent) {
        let Some(local_id) = self.player_id.as_ref() else {
            return;
        };
        let Some(local) = self.predicted_boats.get_mut(local_id) else {
            return;
        };
        local.intent = local_intent;

        let mut actuator_torques = vec![ActuatorTorques::default(); self.predicted_boats.len()];
        for boat in self.predicted_boats.values() {
            let motion = self.prediction.motion(boat.index);
            actuator_torques[boat.index] = controlled_torques(motion, boat.intent);
        }
        self.prediction
            .advance(demo_environment(), &actuator_torques);
    }

    fn start_correction(
        &mut self,
        previous_motion: HashMap<String, BoatMotion>,
        visible_player_ids: &[String],
    ) {
        self.corrections.clear();
        self.correction_started_at_seconds = self.time_seconds;
        for player_id in visible_player_ids {
            let Some(previous) = previous_motion.get(player_id) else {
                continue;
            };
            let Some(current) = self.predicted_motion(player_id) else {
                continue;
            };
            self.corrections.insert(
                player_id.clone(),
                MotionCorrection {
                    position_m: previous.position_m - current.position_m,
                    velocity_m_s: previous.velocity_m_s - current.velocity_m_s,
                    angle_rad: shortest_angle(previous.angle_rad - current.angle_rad),
                    angular_velocity_rad_s: previous.angular_velocity_rad_s
                        - current.angular_velocity_rad_s,
                    sail_angle_rad: shortest_angle(
                        previous.sail_angle_rad - current.sail_angle_rad,
                    ),
                    sail_angular_velocity_rad_s: previous.sail_angular_velocity_rad_s
                        - current.sail_angular_velocity_rad_s,
                    rudder_angle_rad: shortest_angle(
                        previous.rudder_angle_rad - current.rudder_angle_rad,
                    ),
                    rudder_angular_velocity_rad_s: previous.rudder_angular_velocity_rad_s
                        - current.rudder_angular_velocity_rad_s,
                },
            );
        }
    }

    fn predicted_motion(&self, player_id: &str) -> Option<BoatMotion> {
        if let Some(boat) = self.predicted_boats.get(player_id) {
            return Some(self.prediction.motion(boat.index));
        }
        let boat = self.boats.get(player_id)?;
        let mut motion = snapshot_motion(&boat.snapshot);
        if !boat.stale {
            let received_at = self.snapshot_received_at_seconds[player_id];
            let seconds = (self.time_seconds - received_at).clamp(0.0, MAXIMUM_PREDICTION_SECONDS);
            motion.position_m += motion.velocity_m_s * seconds;
            motion.angle_rad += motion.angular_velocity_rad_s * seconds;
            motion.sail_angle_rad += motion.sail_angular_velocity_rad_s * seconds;
            motion.rudder_angle_rad += motion.rudder_angular_velocity_rad_s * seconds;
        }
        Some(motion)
    }

    fn render_motion(&self, player_id: &str) -> Option<BoatMotion> {
        let mut motion = self.predicted_motion(player_id)?;
        if let Some(correction) = self.corrections.get(player_id) {
            let elapsed_seconds = self.time_seconds - self.correction_started_at_seconds;
            let (position_m, velocity_m_s) = cubic_vector_correction(
                correction.position_m,
                correction.velocity_m_s,
                elapsed_seconds,
            );
            let (angle_rad, angular_velocity_rad_s) = cubic_correction(
                correction.angle_rad,
                correction.angular_velocity_rad_s,
                elapsed_seconds,
            );
            let (sail_angle_rad, sail_angular_velocity_rad_s) = cubic_correction(
                correction.sail_angle_rad,
                correction.sail_angular_velocity_rad_s,
                elapsed_seconds,
            );
            let (rudder_angle_rad, rudder_angular_velocity_rad_s) = cubic_correction(
                correction.rudder_angle_rad,
                correction.rudder_angular_velocity_rad_s,
                elapsed_seconds,
            );
            motion.position_m += position_m;
            motion.velocity_m_s += velocity_m_s;
            motion.angle_rad += angle_rad;
            motion.angular_velocity_rad_s += angular_velocity_rad_s;
            motion.sail_angle_rad += sail_angle_rad;
            motion.sail_angular_velocity_rad_s += sail_angular_velocity_rad_s;
            motion.rudder_angle_rad += rudder_angle_rad;
            motion.rudder_angular_velocity_rad_s += rudder_angular_velocity_rad_s;
        }
        Some(motion)
    }
}

pub struct Demo {
    parameters: BoatParameters,
    simulation: SharedSimulation,
    player_boat: usize,
    practice_boat: usize,
    motion_intent: MotionIntent,
    contacts: Vec<HullContact>,
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
        let player_motion = BoatMotion {
            position_m: Vector::ZERO,
            velocity_m_s: Vector::ZERO,
            angle_rad: 135.0_f32.to_radians(),
            angular_velocity_rad_s: 0.0,
            sail_angle_rad: 0.0,
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: 0.0,
            rudder_angular_velocity_rad_s: 0.0,
        };
        let mut practice_motion = player_motion;
        practice_motion.position_m =
            Rotation::new(player_motion.angle_rad).transform_vector(Vector::new(6.0, 0.0));
        practice_motion.angle_rad += std::f32::consts::PI;
        let mut simulation = SharedSimulation::new();
        let player_boat = simulation.add_boat(parameters, player_motion);
        let practice_boat = simulation.add_boat(parameters, practice_motion);
        Self {
            parameters,
            simulation,
            player_boat,
            practice_boat,
            motion_intent: MotionIntent::default(),
            contacts: Vec::new(),
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
            let environment = demo_environment();
            let motion = self.motion();
            let control_input = surface_control_input(
                self.parameters,
                motion,
                environment,
                1,
                10.0_f32.to_radians(),
            );
            let maximum_yaw_rate_rad_s =
                computed_maximum_yaw_rate_rad_s(self.parameters, motion, environment);
            let targets =
                yaw_rate_move_control(&control_input, self.motion_intent, maximum_yaw_rate_rad_s);
            let player_torques = surface_target_control_torques(self.parameters, motion, targets);
            self.contacts = self
                .simulation
                .advance(environment, &[player_torques, ActuatorTorques::default()]);
            self.accumulator_seconds -= DT_SECONDS;
        }
    }

    pub fn motion(&self) -> BoatMotion {
        self.simulation.motion(self.player_boat)
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

    pub fn scenes(&self) -> [BoatScene; 2] {
        [
            boat_scene(self.parameters, self.motion()),
            boat_scene(self.parameters, self.simulation.motion(self.practice_boat)),
        ]
    }

    pub fn contacts(&self) -> &[HullContact] {
        &self.contacts
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

fn snapshot_motion(snapshot: &BoatSnapshot) -> BoatMotion {
    BoatMotion {
        position_m: Vector::new(snapshot.x, snapshot.y),
        velocity_m_s: Vector::new(snapshot.vx, snapshot.vy),
        angle_rad: snapshot.angle,
        angular_velocity_rad_s: snapshot.angular_velocity,
        sail_angle_rad: snapshot.sail,
        sail_angular_velocity_rad_s: snapshot.sail_angular_velocity,
        rudder_angle_rad: snapshot.rudder,
        rudder_angular_velocity_rad_s: snapshot.rudder_angular_velocity,
    }
}

fn controlled_torques(motion: BoatMotion, intent: MotionIntent) -> ActuatorTorques {
    let parameters = demo_parameters();
    let environment = demo_environment();
    let control_input =
        surface_control_input(parameters, motion, environment, 1, 10.0_f32.to_radians());
    let maximum_yaw_rate_rad_s = computed_maximum_yaw_rate_rad_s(parameters, motion, environment);
    let targets = yaw_rate_move_control(&control_input, intent, maximum_yaw_rate_rad_s);
    surface_target_control_torques(parameters, motion, targets)
}

fn shortest_angle(angle_rad: f32) -> f32 {
    (angle_rad + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn cubic_correction(
    displacement: f32,
    velocity_difference: f32,
    elapsed_seconds: f32,
) -> (f32, f32) {
    let progress = (elapsed_seconds / CORRECTION_SECONDS).clamp(0.0, 1.0);
    let progress_squared = progress * progress;
    let progress_cubed = progress_squared * progress;
    let displacement_basis = 2.0 * progress_cubed - 3.0 * progress_squared + 1.0;
    let velocity_basis = progress_cubed - 2.0 * progress_squared + progress;
    let displacement_derivative = (6.0 * progress_squared - 6.0 * progress) / CORRECTION_SECONDS;
    let velocity_derivative = 3.0 * progress_squared - 4.0 * progress + 1.0;
    (
        displacement * displacement_basis
            + velocity_difference * CORRECTION_SECONDS * velocity_basis,
        displacement * displacement_derivative + velocity_difference * velocity_derivative,
    )
}

fn cubic_vector_correction(
    displacement: Vector,
    velocity_difference: Vector,
    elapsed_seconds: f32,
) -> (Vector, Vector) {
    let (x, vx) = cubic_correction(displacement.x, velocity_difference.x, elapsed_seconds);
    let (y, vy) = cubic_correction(displacement.y, velocity_difference.y, elapsed_seconds);
    (Vector::new(x, y), Vector::new(vx, vy))
}
