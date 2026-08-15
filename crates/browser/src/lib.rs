use std::collections::HashMap;

use pyboat_game::{
    computed_yaw_rate_torques, computed_yaw_rate_torques_in_environment, dinghy_parameters,
    match_environment, procedural_match_environment,
};
use pyboat_protocol::{BoatSnapshot, ClientMessage, ServerMessage};
use pyboat_simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, DT_SECONDS, Environment, HullContact,
    MotionIntent, SharedSimulation, apparent_flow, hull_outline_m,
};
use rapier2d::prelude::{Rotation, Vector};

#[cfg(target_arch = "wasm32")]
mod web;

pub const DEFAULT_VIEW_HEIGHT_M: f32 = 15.0;
#[cfg(target_arch = "wasm32")]
pub(crate) const SCENE_SHADER: &str = include_str!("scene.wgsl");
const VIEWPORT_INTEREST_MARGIN_M: f32 = 2.5;
const MAXIMUM_PREDICTION_SECONDS: f32 = 0.25;
const MAXIMUM_REPLAY_COMMANDS: usize = 15;
const CORRECTION_SECONDS: f32 = 0.1;
const WINDSOCK_FULL_EXTENSION_SPEED_M_S: f32 = 7.716_666_7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoatScene {
    pub hull_m: [Vector; 5],
    pub sail_m: [Vector; 2],
    pub rudder_m: [Vector; 2],
    pub windsock: Option<Windsock>,
    pub is_local: bool,
    pub recently_damaged: bool,
    pub wreck: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Windsock {
    pub anchor_m: Vector,
    pub apparent_wind_velocity_m_s: Vector,
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
    pub water_depth_m: f32,
    pub bottom_clearance_m: f32,
    pub bottom_contact_fraction: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplicatedBoat {
    pub snapshot: BoatSnapshot,
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
    wreck: bool,
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
    match_seed: Option<u64>,
    connection_active: bool,
    boats: HashMap<String, ReplicatedBoat>,
    snapshot_received_at_seconds: HashMap<String, f32>,
    tick: Option<u64>,
    prediction_tick: u64,
    motion_intent: MotionIntent,
    next_input_sequence: i64,
    input_history: Vec<InputCommand>,
    input_accumulator_seconds: f32,
    sent_viewport_radius_m: Option<f32>,
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
            match_seed: None,
            connection_active: false,
            boats: HashMap::new(),
            snapshot_received_at_seconds: HashMap::new(),
            tick: None,
            prediction_tick: 0,
            motion_intent: MotionIntent::default(),
            next_input_sequence: 1,
            input_history: Vec::new(),
            input_accumulator_seconds: 0.0,
            sent_viewport_radius_m: None,
            time_seconds: 0.0,
            prediction: SharedSimulation::new(),
            predicted_boats: HashMap::new(),
            corrections: HashMap::new(),
            correction_started_at_seconds: 0.0,
        }
    }

    pub fn accept(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome {
                player_id,
                match_seed,
            } => {
                self.player_id = Some(player_id);
                self.match_seed = Some(match_seed);
                self.connection_active = true;
            }
            ServerMessage::Snapshot {
                tick,
                boats,
                removed_player_ids: _,
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
                let visible_player_ids = boats
                    .iter()
                    .map(|snapshot| snapshot.id.clone())
                    .collect::<Vec<_>>();
                self.boats.clear();
                self.snapshot_received_at_seconds.clear();
                for snapshot in boats {
                    self.snapshot_received_at_seconds
                        .insert(snapshot.id.clone(), self.time_seconds);
                    self.boats
                        .insert(snapshot.id.clone(), ReplicatedBoat { snapshot });
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

    pub fn match_seed(&self) -> Option<u64> {
        self.match_seed
    }

    pub fn disconnect(&mut self) {
        self.connection_active = false;
        self.input_history.clear();
        self.sent_viewport_radius_m = None;
        let player_id = self.player_id.clone();
        for (boat_id, boat) in &mut self.boats {
            if Some(boat_id) == player_id.as_ref() {
                continue;
            }
            boat.snapshot.health = 0.0;
            boat.snapshot.wreck = true;
        }
        for (boat_id, boat) in &mut self.predicted_boats {
            if Some(boat_id) != player_id.as_ref() {
                boat.wreck = true;
            }
        }
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

    pub fn update_viewport(&mut self, radius_m: f32) -> Option<ClientMessage> {
        if !self.connection_active || self.sent_viewport_radius_m == Some(radius_m) {
            return None;
        }
        self.sent_viewport_radius_m = Some(radius_m);
        Some(ClientMessage::Viewport { radius: radius_m })
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
        self.input_accumulator_seconds += elapsed_seconds.min(MAXIMUM_PREDICTION_SECONDS);
        let local_wreck = self
            .player_id
            .as_ref()
            .and_then(|player_id| self.boats.get(player_id))
            .is_some_and(|boat| boat.snapshot.wreck);
        let mut inputs = Vec::new();
        while self.input_accumulator_seconds >= DT_SECONDS {
            if self.connection_active && !local_wreck {
                let command = InputCommand {
                    sequence: self.next_input_sequence,
                    intent: self.motion_intent,
                };
                if self.input_history.len() == MAXIMUM_REPLAY_COMMANDS {
                    self.input_history.remove(0);
                }
                self.input_history.push(command);
                inputs.push(ClientMessage::Input {
                    sequence: command.sequence,
                    forward: command.intent.forward,
                    turn: command.intent.turn,
                });
                self.next_input_sequence += 1;
            }
            self.advance_prediction(self.motion_intent);
            self.input_accumulator_seconds -= DT_SECONDS;
        }
        inputs
    }

    pub fn camera_m(&self) -> Option<Vector> {
        Some(self.local_motion()?.position_m)
    }

    pub fn view(&self) -> Option<DemoView> {
        let motion = self.local_motion()?;
        let environment =
            procedural_match_environment(self.match_seed?, self.prediction_tick, motion.position_m);
        let draft_m = dinghy_parameters().draft_m;
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
            water_depth_m: environment.water_depth_m,
            bottom_clearance_m: environment.water_depth_m - draft_m,
            bottom_contact_fraction: ((draft_m - environment.water_depth_m) / draft_m)
                .clamp(0.0, 1.0),
        })
    }

    pub fn health(&self) -> Option<f32> {
        let player_id = self.player_id.as_ref()?;
        Some(self.boats.get(player_id)?.snapshot.health)
    }

    pub fn scenes(&self) -> Vec<BoatScene> {
        let mut boats = self.boats.values().collect::<Vec<_>>();
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
                self.render_motion(&boat.snapshot.id).map(|motion| {
                    let mut scene = boat_scene(dinghy_parameters(), motion);
                    scene.is_local = self.player_id() == Some(boat.snapshot.id.as_str());
                    if scene.is_local {
                        scene.windsock = self.match_seed.map(|match_seed| Windsock {
                            anchor_m: scene.hull_m[0],
                            apparent_wind_velocity_m_s: apparent_flow(
                                procedural_match_environment(
                                    match_seed,
                                    self.prediction_tick,
                                    motion.position_m,
                                )
                                .true_wind_velocity_m_s,
                                motion.velocity_m_s,
                            ),
                        });
                    }
                    scene.recently_damaged = !boat.snapshot.wreck
                        && boat.snapshot.last_hit_tick.is_some_and(|last_hit_tick| {
                            self.tick
                                .is_some_and(|tick| tick.saturating_sub(last_hit_tick) <= 6)
                        });
                    scene.wreck = boat.snapshot.wreck;
                    scene
                })
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
            let index = prediction.add_boat(dinghy_parameters(), snapshot_motion(&boat.snapshot));
            predicted_boats.insert(
                player_id.clone(),
                PredictedBoat {
                    index,
                    intent: MotionIntent { forward, turn },
                    wreck: boat.snapshot.wreck,
                },
            );
        }
        self.prediction = prediction;
        self.predicted_boats = predicted_boats;
        self.prediction_tick = self.tick.unwrap_or(0);

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
        let Some(match_seed) = self.match_seed else {
            return;
        };
        let Some(local) = self.predicted_boats.get_mut(local_id) else {
            return;
        };
        local.intent = local_intent;

        let mut environments = vec![Environment::default(); self.predicted_boats.len()];
        let mut actuator_torques = vec![ActuatorTorques::default(); self.predicted_boats.len()];
        for boat in self.predicted_boats.values() {
            let motion = self.prediction.motion(boat.index);
            let environment =
                procedural_match_environment(match_seed, self.prediction_tick, motion.position_m);
            environments[boat.index] = environment;
            if !boat.wreck {
                actuator_torques[boat.index] =
                    computed_yaw_rate_torques_in_environment(motion, boat.intent, environment);
            }
        }
        self.prediction
            .advance_with_environments(&environments, &actuator_torques);
        self.prediction_tick += 1;
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
        let received_at = self.snapshot_received_at_seconds[player_id];
        let seconds = (self.time_seconds - received_at).clamp(0.0, MAXIMUM_PREDICTION_SECONDS);
        motion.position_m += motion.velocity_m_s * seconds;
        motion.angle_rad += motion.angular_velocity_rad_s * seconds;
        motion.sail_angle_rad += motion.sail_angular_velocity_rad_s * seconds;
        motion.rudder_angle_rad += motion.rudder_angular_velocity_rad_s * seconds;
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

pub fn viewport_interest_radius_m(aspect_ratio: f32, view_height_m: f32) -> f32 {
    let view_width_m = view_height_m * aspect_ratio;
    view_width_m.hypot(view_height_m) / 2.0 + VIEWPORT_INTEREST_MARGIN_M
}

pub fn zoom_view_height_m(view_height_m: f32, wheel_delta_y: f32) -> f32 {
    let zoomed_view_height_m = view_height_m * (wheel_delta_y * 0.001).exp();
    if zoomed_view_height_m.is_finite() && zoomed_view_height_m > 0.0 {
        zoomed_view_height_m
    } else {
        view_height_m
    }
}

pub struct ScaleBar {
    pub distance_m: f32,
    pub width_px: f32,
    pub label: String,
}

// Keep the 1-2-5 selection synchronized with `updateScaleBar` in
// `playground/src/main.ts`; gameplay and the lab share one physical-scale convention.
pub fn scale_bar(view_height_m: f32, viewport_height_px: f32) -> ScaleBar {
    let target_distance_m = view_height_m * 120.0 / viewport_height_px;
    let magnitude_m = 10.0_f32.powf(target_distance_m.log10().floor());
    let normalized_distance = target_distance_m / magnitude_m;
    let readable_multiplier = if normalized_distance < 1.5 {
        1.0
    } else if normalized_distance < 3.5 {
        2.0
    } else if normalized_distance < 7.5 {
        5.0
    } else {
        10.0
    };
    let distance_m = readable_multiplier * magnitude_m;
    let label = if distance_m >= 1_000.0 {
        format!("{:.0} km", distance_m / 1_000.0)
    } else if distance_m >= 1.0 {
        format!("{distance_m:.0} m")
    } else if distance_m >= 0.01 {
        format!("{:.0} cm", distance_m * 100.0)
    } else if distance_m >= 0.001 {
        format!("{:.0} mm", distance_m * 1_000.0)
    } else if distance_m >= 0.000_001 {
        format!("{:.0} µm", distance_m * 1_000_000.0)
    } else {
        format!("{:.0} nm", distance_m * 1_000_000_000.0)
    };
    ScaleBar {
        distance_m,
        width_px: distance_m / view_height_m * viewport_height_px,
        label,
    }
}

pub fn windsock_extension(apparent_wind_speed_m_s: f32) -> f32 {
    (apparent_wind_speed_m_s.max(0.0) / WINDSOCK_FULL_EXTENSION_SPEED_M_S).min(1.0)
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
        let parameters = dinghy_parameters();
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
            let motion = self.motion();
            let player_torques = computed_yaw_rate_torques(motion, self.motion_intent);
            self.contacts = self.simulation.advance(
                match_environment(),
                &[player_torques, ActuatorTorques::default()],
            );
            self.accumulator_seconds -= DT_SECONDS;
        }
    }

    pub fn motion(&self) -> BoatMotion {
        self.simulation.motion(self.player_boat)
    }

    pub fn view(&self) -> DemoView {
        let motion = self.motion();
        let environment = match_environment();
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
            water_depth_m: environment.water_depth_m,
            bottom_clearance_m: environment.water_depth_m - self.parameters.draft_m,
            bottom_contact_fraction: ((self.parameters.draft_m - environment.water_depth_m)
                / self.parameters.draft_m)
                .clamp(0.0, 1.0),
        }
    }

    pub fn scenes(&self) -> [BoatScene; 2] {
        let mut player = boat_scene(self.parameters, self.motion());
        player.is_local = true;
        [
            player,
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
        windsock: None,
        is_local: false,
        recently_damaged: false,
        wreck: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiplayer_input_history_keeps_only_replayable_commands() {
        let mut client = MultiplayerClient::new();
        client.accept(ServerMessage::Welcome {
            player_id: "player-1".to_owned(),
            match_seed: 42,
        });

        for _ in 0..10 {
            client.take_inputs(MAXIMUM_PREDICTION_SECONDS);
        }

        assert_eq!(client.input_history.len(), MAXIMUM_REPLAY_COMMANDS);
        assert_eq!(client.input_history[0].sequence, 136);
        assert_eq!(client.input_history[14].sequence, 150);
    }
}
