use std::collections::HashMap;

use game::{
    WorldEnvironment, computed_yaw_rate_torques, computed_yaw_rate_torques_in_environment,
    dinghy_parameters, match_environment, procedural_match_environment,
    procedural_world_environment,
};
use protocol::{BoatSnapshot, ClientMessage, ServerMessage};
use rapier2d::prelude::{Rotation, Vector};
use simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, DT_SECONDS, Environment, HullContact,
    MotionIntent, SharedSimulation, WorldGeometry, apparent_flow, hull_outline_m,
};

#[cfg(test)]
mod diagnostics;
#[cfg(target_arch = "wasm32")]
mod web;

pub const DEFAULT_VIEW_HEIGHT_M: f32 = 15.0;
pub const CLOUD_SHADOW_CACHE_TILE_TEXELS: u32 = 128;

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn queue_latest_snapshot(pending: &mut Option<ServerMessage>, snapshot: ServerMessage) {
    let ServerMessage::Snapshot { tick, .. } = &snapshot else {
        return;
    };
    let should_replace = pending.as_ref().is_none_or(|pending| match pending {
        ServerMessage::Snapshot {
            tick: pending_tick, ..
        } => tick >= pending_tick,
        ServerMessage::Welcome { .. } => true,
    });
    if should_replace {
        *pending = Some(snapshot);
    }
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn disconnect_and_clear_pending_snapshot(
    client: &mut MultiplayerClient,
    pending: &mut Option<ServerMessage>,
) {
    pending.take();
    client.disconnect();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloudShadowCacheLayout {
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub atlas_columns: u32,
    pub maximum_tiles: u32,
    pub table_entries: u32,
}

pub fn cloud_shadow_cache_layout(
    viewport_width: u32,
    viewport_height: u32,
    maximum_texture_dimension: u32,
) -> CloudShadowCacheLayout {
    cloud_shadow_cache_layout_for_tile_size(
        viewport_width,
        viewport_height,
        maximum_texture_dimension,
        CLOUD_SHADOW_CACHE_TILE_TEXELS,
    )
}

pub fn cloud_shadow_cache_layout_for_tile_size(
    viewport_width: u32,
    viewport_height: u32,
    maximum_texture_dimension: u32,
    tile_texels: u32,
) -> CloudShadowCacheLayout {
    let screen_columns = viewport_width.div_ceil(tile_texels);
    let screen_rows = viewport_height.div_ceil(tile_texels);
    let screen_footprint_tiles = screen_columns
        .saturating_add(1)
        .saturating_mul(screen_rows.saturating_add(1));
    let desired_tiles = 6u32.saturating_mul(screen_footprint_tiles).max(1);
    let physical_tile_texels = tile_texels + 2;
    let maximum_axis_tiles = (maximum_texture_dimension / physical_tile_texels).max(1);
    let maximum_tiles = desired_tiles.min(maximum_axis_tiles.saturating_pow(2));
    let square_columns = (maximum_tiles as f64).sqrt().ceil() as u32;
    let atlas_columns = square_columns.min(maximum_axis_tiles).max(1);
    let atlas_rows = maximum_tiles.div_ceil(atlas_columns);
    // Discovery retains eighteen cloud-disc and eight landform footprints, including
    // singletons that never consume atlas space.
    let marker_tiles = 26u32.saturating_mul(screen_footprint_tiles).max(1);
    let table_entries = marker_tiles.saturating_mul(2).next_power_of_two();

    CloudShadowCacheLayout {
        atlas_width: atlas_columns * physical_tile_texels,
        atlas_height: atlas_rows * physical_tile_texels,
        atlas_columns,
        maximum_tiles,
        table_entries,
    }
}
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) const SCENE_SHADER: &str =
    include_str!(concat!(env!("OUT_DIR"), "/scene-production.wgsl"));
#[cfg(any(target_arch = "wasm32", test))]
pub(crate) const CLOUD_SHADOW_CACHE_SHADER: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/scene-production-cloud-shadow-cache.wgsl"
));

#[cfg(any(feature = "playground", test))]
pub(crate) const PLAYGROUND_SCENE_SHADER: &str =
    include_str!(concat!(env!("OUT_DIR"), "/scene-playground.wgsl"));

#[cfg(any(feature = "playground", test))]
const PLAYGROUND_DIRECT_SCENE_SHADER: &str =
    include_str!(concat!(env!("OUT_DIR"), "/scene-playground-direct.wgsl"));

#[cfg(any(feature = "playground", test))]
pub(crate) const PLAYGROUND_CLOUD_SHADOW_CACHE_SHADER: &str = include_str!(concat!(
    env!("OUT_DIR"),
    "/scene-playground-cloud-shadow-cache.wgsl"
));

#[cfg(any(all(target_arch = "wasm32", feature = "playground"), test))]
pub(crate) const PLAYGROUND_STARTUP_SHADER: &str =
    include_str!(concat!(env!("OUT_DIR"), "/scene-playground-startup.wgsl"));

#[cfg(any(feature = "playground", test))]
include!(concat!(env!("OUT_DIR"), "/diagnostic-shaders.rs"));

#[cfg(feature = "playground")]
pub fn playground_scene_shader_source() -> &'static str {
    PLAYGROUND_SCENE_SHADER
}

#[cfg(feature = "playground")]
pub fn playground_direct_scene_shader_source() -> &'static str {
    PLAYGROUND_DIRECT_SCENE_SHADER
}

#[cfg(feature = "playground")]
pub fn playground_cloud_shadow_cache_shader_source() -> &'static str {
    PLAYGROUND_CLOUD_SHADOW_CACHE_SHADER
}

#[cfg(feature = "playground")]
pub fn playground_diagnostic_shader_source(name: &str) -> Option<&'static str> {
    generated_playground_diagnostic_shader(name)
}
const VIEWPORT_INTEREST_MARGIN_M: f32 = 2.5;
const MAXIMUM_PREDICTION_SECONDS: f32 = 0.25;
const MAXIMUM_REPLAY_COMMANDS: usize = 15;
const CORRECTION_SECONDS: f32 = 0.1;
const WINDSOCK_FULL_EXTENSION_SPEED_M_S: f32 = 7.716_666_7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldGeometrySample {
    pub raw_center_m: [f32; 2],
    pub canonical_center_m: [f32; 2],
    pub normalized_world: [f32; 2],
    pub latitude_degrees: f32,
    pub local_solar_phase: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeriodicEffectTime {
    pub epoch: u64,
    pub age_ticks: u64,
    pub phase: f32,
}

pub fn periodic_effect_time(tick: u64, period_ticks: u64) -> PeriodicEffectTime {
    let epoch = tick / period_ticks;
    let age_ticks = tick % period_ticks;
    PeriodicEffectTime {
        epoch,
        age_ticks,
        phase: age_ticks as f32 / period_ticks as f32,
    }
}

pub fn seasonal_phase_factors(season_phase: f32) -> [f32; 4] {
    let weather_angle = std::f32::consts::TAU * (season_phase - 0.34);
    let ground_angle = std::f32::consts::TAU * (season_phase - 0.38);
    let water_angle = std::f32::consts::TAU * (season_phase - 0.425);
    [
        weather_angle.sin(),
        ground_angle.sin(),
        ground_angle.cos(),
        water_angle.sin(),
    ]
}

pub fn sample_world_geometry(
    geometry: WorldGeometry,
    raw_center_m: [f32; 2],
    global_day_phase: f32,
) -> WorldGeometrySample {
    WorldGeometrySample {
        raw_center_m,
        canonical_center_m: geometry.wrap_position_m(raw_center_m),
        normalized_world: geometry.normalized_world_coordinates(raw_center_m),
        latitude_degrees: geometry.latitude_degrees(raw_center_m[1]),
        local_solar_phase: geometry.local_solar_phase(global_day_phase, raw_center_m[0]),
    }
}

pub fn sample_production_world_environment(
    geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    center_m: Vector,
) -> WorldEnvironment {
    procedural_world_environment(geometry, match_seed, tick, center_m)
}

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
    world_geometry: Option<WorldGeometry>,
    connection_active: bool,
    boats: HashMap<String, ReplicatedBoat>,
    snapshot_received_at_seconds: HashMap<String, f32>,
    tick: Option<u64>,
    prediction_tick: u64,
    motion_intent: MotionIntent,
    next_input_sequence: i64,
    input_history: Vec<InputCommand>,
    input_accumulator_seconds: f32,
    next_viewport_sequence: i64,
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
            world_geometry: None,
            connection_active: false,
            boats: HashMap::new(),
            snapshot_received_at_seconds: HashMap::new(),
            tick: None,
            prediction_tick: 0,
            motion_intent: MotionIntent::default(),
            next_input_sequence: 1,
            input_history: Vec::new(),
            input_accumulator_seconds: 0.0,
            next_viewport_sequence: 1,
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
                world_geometry,
            } => {
                self.player_id = Some(player_id);
                self.match_seed = Some(match_seed);
                self.world_geometry = Some(world_geometry);
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

    pub fn world_geometry(&self) -> Option<WorldGeometry> {
        self.world_geometry
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
        let sequence = self.next_viewport_sequence;
        self.next_viewport_sequence += 1;
        Some(ClientMessage::Viewport {
            sequence,
            radius: radius_m,
        })
    }

    pub fn set_player_input(
        &mut self,
        keyboard_forward: f32,
        keyboard_turn: f32,
        world_x: f32,
        world_y: f32,
    ) {
        self.set_motion_intent(keyboard_forward, keyboard_turn);
        let world_motion = Vector::new(world_x, world_y);
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
        let position_m = self.local_motion()?.position_m;
        Some(Vector::from(
            self.world_geometry?.wrap_position_m(position_m.into()),
        ))
    }

    pub fn view(&self) -> Option<DemoView> {
        let motion = self.local_motion()?;
        let environment = procedural_match_environment(
            self.world_geometry?,
            self.match_seed?,
            self.prediction_tick,
            motion.position_m,
        );
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
        let local_position_m = self.local_motion().map(|motion| motion.position_m);
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
                self.render_motion(&boat.snapshot.id).map(|mut motion| {
                    if let (Some(world_geometry), Some(local_position_m)) =
                        (self.world_geometry, local_position_m)
                    {
                        motion.position_m = local_position_m
                            + Vector::from(world_geometry.minimum_image_displacement_m(
                                (motion.position_m - local_position_m).into(),
                            ));
                    }
                    let mut scene = boat_scene(dinghy_parameters(), motion);
                    scene.is_local = self.player_id() == Some(boat.snapshot.id.as_str());
                    if scene.is_local {
                        scene.windsock = self.match_seed.map(|match_seed| Windsock {
                            anchor_m: scene.hull_m[0],
                            apparent_wind_velocity_m_s: apparent_flow(
                                procedural_match_environment(
                                    self.world_geometry.unwrap_or_default(),
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

        let mut prediction =
            SharedSimulation::new_with_world_geometry(self.world_geometry.unwrap_or_default());
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
            let environment = procedural_match_environment(
                self.world_geometry.unwrap_or_default(),
                match_seed,
                self.prediction_tick,
                motion.position_m,
            );
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
                    position_m: Vector::from(
                        self.world_geometry
                            .unwrap_or_default()
                            .minimum_image_displacement_m(
                                (previous.position_m - current.position_m).into(),
                            ),
                    ),
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
        motion.position_m = Vector::from(
            self.world_geometry
                .unwrap_or_default()
                .wrap_position_m(motion.position_m.into()),
        );
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
        motion.position_m = Vector::from(
            self.world_geometry
                .unwrap_or_default()
                .wrap_position_m(motion.position_m.into()),
        );
        Some(motion)
    }
}

pub fn viewport_interest_radius_m(aspect_ratio: f32, view_height_m: f32) -> f32 {
    let view_width_m = view_height_m * aspect_ratio;
    view_width_m.hypot(view_height_m) / 2.0 + VIEWPORT_INTEREST_MARGIN_M
}

pub fn visual_pixel_footprint_m(view_height_m: f32, render_height_px: u32) -> f32 {
    view_height_m / render_height_px as f32
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
        world_x: f32,
        world_y: f32,
    ) {
        self.set_motion_intent(keyboard_forward, keyboard_turn);
        let world_motion = Vector::new(world_x, world_y);
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

    fn snapshot(tick: u64) -> ServerMessage {
        ServerMessage::Snapshot {
            tick,
            boats: Vec::new(),
            removed_player_ids: Vec::new(),
        }
    }

    #[test]
    fn pending_snapshot_keeps_the_highest_received_tick() {
        let mut pending = None;

        queue_latest_snapshot(&mut pending, snapshot(7));
        queue_latest_snapshot(&mut pending, snapshot(5));
        assert_eq!(pending, Some(snapshot(7)));

        queue_latest_snapshot(&mut pending, snapshot(9));
        assert_eq!(pending, Some(snapshot(9)));
    }

    #[test]
    fn disconnect_discards_a_queued_snapshot() {
        let mut client = MultiplayerClient::new();
        client.accept(ServerMessage::Welcome {
            player_id: "player-1".to_owned(),
            match_seed: 42,
            world_geometry: WorldGeometry::default(),
        });
        let mut pending = Some(snapshot(7));

        disconnect_and_clear_pending_snapshot(&mut client, &mut pending);

        assert_eq!(pending, None);
        assert_eq!(client.update_viewport(10.0), None);
    }

    #[test]
    fn production_shader_exposes_only_the_game_interface() {
        let module = naga::front::wgsl::parse_str(SCENE_SHADER).unwrap();
        let entry_points: Vec<_> = module
            .entry_points
            .iter()
            .map(|entry| (entry.stage, entry.name.as_str()))
            .collect();
        assert_eq!(
            entry_points,
            [
                (naga::ShaderStage::Vertex, "vertex"),
                (naga::ShaderStage::Fragment, "fragment"),
            ],
        );
        assert!(module.global_variables.iter().all(|(_, variable)| {
            variable
                .binding
                .as_ref()
                .is_none_or(|binding| binding.group == 0 || binding.group == 2)
        }));
        assert!(module.global_variables.iter().all(|(_, variable)| {
            variable
                .binding
                .as_ref()
                .is_none_or(|binding| binding.group != 2 || binding.binding <= 3)
        }));

        let cache = naga::front::wgsl::parse_str(CLOUD_SHADOW_CACHE_SHADER).unwrap();
        let cache_entry_points: Vec<_> = cache
            .entry_points
            .iter()
            .map(|entry| (entry.stage, entry.name.as_str()))
            .collect();
        assert_eq!(
            cache_entry_points,
            [
                (
                    naga::ShaderStage::Compute,
                    "discover_cloud_shadow_cache_tiles",
                ),
                (naga::ShaderStage::Vertex, "cloud_shadow_cache_vertex"),
                (naga::ShaderStage::Fragment, "cloud_shadow_cache_fragment",),
            ],
        );
    }

    #[test]
    fn cloud_shadow_cache_capacity_tracks_screen_resolution() {
        let layout = cloud_shadow_cache_layout(1920, 1080, 8192);

        assert_eq!(layout.maximum_tiles, 6 * 16 * 10);
        assert!(layout.table_entries >= 2 * 26 * 16 * 10);
        assert!(layout.table_entries.is_power_of_two());
        assert!(layout.atlas_width <= 8192);
        assert!(layout.atlas_height <= 8192);
        assert!(layout.atlas_columns > 0);

        let texture_limited_layout = cloud_shadow_cache_layout(1920, 1080, 1024);
        assert_eq!(texture_limited_layout.table_entries, layout.table_entries);
    }

    #[test]
    fn cloud_shadow_cache_capacity_tracks_selected_tile_size() {
        let layout = cloud_shadow_cache_layout_for_tile_size(1920, 1080, 8192, 64);

        assert_eq!(layout.maximum_tiles, 6 * 31 * 18);
        assert!(layout.table_entries >= 2 * 26 * 31 * 18);
        assert_eq!(layout.atlas_width % 66, 0);
        assert_eq!(layout.atlas_height % 66, 0);
        assert!(layout.atlas_width <= 8192);
        assert!(layout.atlas_height <= 8192);
    }

    #[test]
    fn playground_shaders_expose_only_their_selected_interfaces() {
        let startup = naga::front::wgsl::parse_str(PLAYGROUND_STARTUP_SHADER).unwrap();
        let startup_entry_points: Vec<_> = startup
            .entry_points
            .iter()
            .map(|entry| (entry.stage, entry.name.as_str()))
            .collect();
        assert_eq!(
            startup_entry_points,
            [
                (naga::ShaderStage::Vertex, "vertex"),
                (naga::ShaderStage::Fragment, "fragment_playground_startup"),
            ],
        );

        let scene = naga::front::wgsl::parse_str(PLAYGROUND_SCENE_SHADER).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&scene)
        .unwrap();
        let scene_entry_points: Vec<_> = scene
            .entry_points
            .iter()
            .map(|entry| (entry.stage, entry.name.as_str()))
            .collect();
        assert_eq!(
            scene_entry_points,
            [
                (naga::ShaderStage::Vertex, "vertex"),
                (naga::ShaderStage::Fragment, "fragment_playground_scene"),
            ],
        );

        let cache = naga::front::wgsl::parse_str(PLAYGROUND_CLOUD_SHADOW_CACHE_SHADER).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&cache)
        .unwrap();
        assert_eq!(cache.entry_points.len(), 3);

        let direct = naga::front::wgsl::parse_str(PLAYGROUND_DIRECT_SCENE_SHADER).unwrap();
        assert_eq!(direct.entry_points.len(), 2);

        for &name in diagnostics::DIAGNOSTIC_ENTRY_POINTS {
            let source = generated_playground_diagnostic_shader(name).unwrap();
            let diagnostic = naga::front::wgsl::parse_str(source).unwrap();
            let diagnostic_entry_points: Vec<_> = diagnostic
                .entry_points
                .iter()
                .map(|entry| (entry.stage, entry.name.as_str()))
                .collect();
            let fragment_name = format!("fragment_{name}");
            assert_eq!(
                diagnostic_entry_points,
                [
                    (naga::ShaderStage::Vertex, "vertex"),
                    (naga::ShaderStage::Fragment, fragment_name.as_str()),
                ],
            );
        }
    }

    #[test]
    fn multiplayer_input_history_keeps_only_replayable_commands() {
        let mut client = MultiplayerClient::new();
        client.accept(ServerMessage::Welcome {
            player_id: "player-1".to_owned(),
            match_seed: 42,
            world_geometry: WorldGeometry::default(),
        });

        for _ in 0..10 {
            client.take_inputs(MAXIMUM_PREDICTION_SECONDS);
        }

        assert_eq!(client.input_history.len(), MAXIMUM_REPLAY_COMMANDS);
        assert_eq!(client.input_history[0].sequence, 136);
        assert_eq!(client.input_history[14].sequence, 150);
    }

    #[test]
    fn visual_pixel_footprint_follows_view_height_and_render_height() {
        assert_eq!(visual_pixel_footprint_m(480.0, 960), 0.5);
        assert_eq!(visual_pixel_footprint_m(480.0, 1), 480.0);
    }
}
