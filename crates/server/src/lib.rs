use std::{
    collections::{HashMap, HashSet, VecDeque},
    error::Error,
    io,
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use pyboat_protocol::{
    BoatSnapshot, ClientMessage, ServerMessage, decode_client_message, encode_server_message,
};
use pyboat_simulation::{
    BoatMotion, BoatParameters, Environment, HullParameters, MotionIntent, SharedSimulation,
    computed_maximum_yaw_rate_rad_s, surface_control_input, surface_target_control_torques,
    yaw_rate_move_control,
};
use rapier2d::prelude::Vector;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::time::interval;
use tokio_tungstenite::{accept_async, tungstenite::Message};

type ConnectionResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const PHYSICS_BUBBLE_RADIUS_M: f32 = 8.0;
const VISIBILITY_RADIUS_M: f32 = 24.0;

#[derive(Clone)]
struct OutboundSnapshot {
    message: Message,
    removal_tick: Option<u64>,
}

enum ConnectionEvent {
    Connected {
        player_id: String,
        player_number: u64,
        outbound: watch::Sender<Option<OutboundSnapshot>>,
    },
    Input {
        player_id: String,
        sequence: i64,
        intent: MotionIntent,
    },
    RemovalsSent {
        player_id: String,
        tick: u64,
    },
    Disconnected {
        player_id: String,
    },
}

struct ConnectionState {
    outbound: watch::Sender<Option<OutboundSnapshot>>,
    known_player_ids: HashSet<String>,
    pending_removals: HashMap<String, u64>,
}

struct Player {
    id: String,
    intent: MotionIntent,
    acknowledged_input: i64,
    last_received_input: i64,
    pending_inputs: VecDeque<(i64, MotionIntent)>,
}

struct GameMatch {
    parameters: BoatParameters,
    environment: Environment,
    simulation: SharedSimulation,
    players: Vec<Player>,
    tick: u64,
}

impl GameMatch {
    fn new() -> Self {
        Self {
            parameters: boat_parameters(),
            environment: Environment {
                true_wind_velocity_m_s: Vector::new(6.0, 0.0),
                true_water_velocity_m_s: Vector::ZERO,
            },
            simulation: SharedSimulation::new(),
            players: Vec::new(),
            tick: 0,
        }
    }

    fn add_player(&mut self, player_id: String, player_number: u64) {
        self.simulation.add_boat(
            self.parameters,
            BoatMotion {
                position_m: Vector::new(0.0, (player_number - 1) as f32 * 8.0),
                velocity_m_s: Vector::ZERO,
                angle_rad: 135.0_f32.to_radians(),
                angular_velocity_rad_s: 0.0,
                sail_angle_rad: 0.0,
                sail_angular_velocity_rad_s: 0.0,
                rudder_angle_rad: 0.0,
                rudder_angular_velocity_rad_s: 0.0,
            },
        );
        self.players.push(Player {
            id: player_id,
            intent: MotionIntent::default(),
            acknowledged_input: 0,
            last_received_input: 0,
            pending_inputs: VecDeque::new(),
        });
    }

    fn remove_player(&mut self, player_id: &str) {
        let Some(player) = self
            .players
            .iter()
            .position(|player| player.id == player_id)
        else {
            return;
        };
        self.players.remove(player);
        self.simulation.remove_boat(player);
    }

    fn set_input(&mut self, player_id: &str, sequence: i64, intent: MotionIntent) {
        let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        else {
            return;
        };
        if sequence <= player.last_received_input {
            return;
        }
        player.pending_inputs.push_back((sequence, intent));
        player.last_received_input = sequence;
    }

    fn advance(&mut self) {
        let mut actuator_torques = Vec::with_capacity(self.players.len());
        for (boat, player) in self.players.iter_mut().enumerate() {
            if let Some((sequence, intent)) = player.pending_inputs.pop_front() {
                player.intent = intent;
                player.acknowledged_input = sequence;
            }
            let motion = self.simulation.motion(boat);
            let control_input = surface_control_input(
                self.parameters,
                motion,
                self.environment,
                1,
                10.0_f32.to_radians(),
            );
            let maximum_yaw_rate_rad_s =
                computed_maximum_yaw_rate_rad_s(self.parameters, motion, self.environment);
            let targets =
                yaw_rate_move_control(&control_input, player.intent, maximum_yaw_rate_rad_s);
            actuator_torques.push(surface_target_control_torques(
                self.parameters,
                motion,
                targets,
            ));
        }
        self.simulation.advance(self.environment, &actuator_torques);
        self.tick += 1;
    }

    fn visible_boats(&self, player_id: &str) -> Vec<usize> {
        let viewer = self
            .players
            .iter()
            .position(|player| player.id == player_id)
            .expect("a connection must have an authoritative boat");
        self.simulation.boats_within_radius(
            self.simulation.motion(viewer).position_m,
            VISIBILITY_RADIUS_M,
        )
    }

    fn snapshot_for(
        &self,
        player_id: &str,
        visible_boats: &[usize],
        removed_player_ids: Vec<String>,
    ) -> ServerMessage {
        let viewer = self
            .players
            .iter()
            .position(|player| player.id == player_id)
            .expect("a connection must have an authoritative boat");
        let viewer_position_m = self.simulation.motion(viewer).position_m;
        let boats = visible_boats
            .iter()
            .map(|&boat| {
                let player = &self.players[boat];
                let motion = self.simulation.motion(boat);
                let include_intent = (motion.position_m - viewer_position_m).length_squared()
                    <= PHYSICS_BUBBLE_RADIUS_M * PHYSICS_BUBBLE_RADIUS_M;
                BoatSnapshot {
                    id: player.id.clone(),
                    x: motion.position_m.x,
                    y: motion.position_m.y,
                    vx: motion.velocity_m_s.x,
                    vy: motion.velocity_m_s.y,
                    angle: motion.angle_rad,
                    angular_velocity: motion.angular_velocity_rad_s,
                    sail: motion.sail_angle_rad,
                    sail_angular_velocity: motion.sail_angular_velocity_rad_s,
                    rudder: motion.rudder_angle_rad,
                    rudder_angular_velocity: motion.rudder_angular_velocity_rad_s,
                    forward: include_intent.then_some(player.intent.forward),
                    turn: include_intent.then_some(player.intent.turn),
                    acknowledged_input: (player.id == player_id)
                        .then_some(player.acknowledged_input),
                }
            })
            .collect();
        ServerMessage::Snapshot {
            tick: self.tick,
            boats,
            removed_player_ids,
        }
    }
}

pub async fn serve(listener: TcpListener) -> ServerResult<()> {
    let (events, mut incoming_events) = mpsc::unbounded_channel();
    let mut connections: HashMap<String, ConnectionState> = HashMap::new();
    let mut game_match = GameMatch::new();
    let mut next_player_number = 1_u64;
    let mut ticker = interval(Duration::from_secs_f64(1.0 / 60.0));
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let player_number = next_player_number;
                let player_id = format!("player-{player_number}");
                next_player_number += 1;
                let events = events.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_connection(stream, player_id, player_number, events).await {
                        eprintln!("WebSocket connection failed: {error}");
                    }
                });
            }
            Some(event) = incoming_events.recv() => match event {
                ConnectionEvent::Connected { player_id, player_number, outbound } => {
                    game_match.add_player(player_id.clone(), player_number);
                    connections.insert(player_id, ConnectionState {
                        outbound,
                        known_player_ids: HashSet::new(),
                        pending_removals: HashMap::new(),
                    });
                }
                ConnectionEvent::Input { player_id, sequence, intent } => {
                    game_match.set_input(&player_id, sequence, intent);
                }
                ConnectionEvent::RemovalsSent { player_id, tick } => {
                    if let Some(connection) = connections.get_mut(&player_id) {
                        connection.pending_removals.retain(|_, first_tick| *first_tick > tick);
                    }
                }
                ConnectionEvent::Disconnected { player_id } => {
                    connections.remove(&player_id);
                    game_match.remove_player(&player_id);
                }
            },
            _ = ticker.tick() => {
                game_match.advance();
                let active_player_ids = game_match
                    .players
                    .iter()
                    .map(|player| player.id.clone())
                    .collect::<HashSet<_>>();
                for (player_id, connection) in &mut connections {
                    for removed in connection.known_player_ids.difference(&active_player_ids) {
                        connection
                            .pending_removals
                            .entry(removed.clone())
                            .or_insert(game_match.tick);
                    }
                    connection
                        .known_player_ids
                        .retain(|known| active_player_ids.contains(known));
                    let visible_boats = game_match.visible_boats(player_id);
                    connection.known_player_ids.extend(
                        visible_boats
                            .iter()
                            .map(|boat| game_match.players[*boat].id.clone()),
                    );
                    let mut removed_player_ids = connection
                        .pending_removals
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>();
                    removed_player_ids.sort();
                    let removal_tick = (!removed_player_ids.is_empty()).then_some(game_match.tick);
                    let snapshot = encode_server_message(
                        &game_match.snapshot_for(player_id, &visible_boats, removed_player_ids),
                    )?;
                    connection.outbound.send_replace(Some(OutboundSnapshot {
                        message: Message::Text(snapshot.into()),
                        removal_tick,
                    }));
                }
            }
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    player_id: String,
    player_number: u64,
    events: mpsc::UnboundedSender<ConnectionEvent>,
) -> ConnectionResult<()> {
    let mut connection = accept_async(stream).await?;
    let welcome = encode_server_message(&ServerMessage::Welcome {
        player_id: player_id.clone(),
    })?;
    connection.send(Message::Text(welcome.into())).await?;

    // A watch channel retains only the newest snapshot when a client cannot keep up.
    let (outbound, mut outbound_updates) = watch::channel(None);
    events
        .send(ConnectionEvent::Connected {
            player_id: player_id.clone(),
            player_number,
            outbound,
        })
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "server stopped"))?;
    let result = loop {
        tokio::select! {
            incoming = connection.next() => match incoming {
                Some(Ok(message)) if message.is_text() => {
                    let ClientMessage::Input { sequence, forward, turn } =
                        decode_client_message(message.to_text()?)?;
                    if events.send(ConnectionEvent::Input {
                        player_id: player_id.clone(),
                        sequence,
                        intent: MotionIntent { forward, turn },
                    }).is_err() {
                        break Ok(());
                    }
                }
                Some(Ok(message)) if !message.is_close() => {}
                Some(Ok(_)) | None => break Ok(()),
                Some(Err(error)) => break Err(error),
            },
            changed = outbound_updates.changed() => {
                if changed.is_err() {
                    break Ok(());
                }
                let snapshot = outbound_updates.borrow_and_update().clone();
                if let Some(snapshot) = snapshot {
                    if let Err(error) = connection.send(snapshot.message).await {
                        break Err(error);
                    }
                    if let Some(tick) = snapshot.removal_tick {
                        if events.send(ConnectionEvent::RemovalsSent {
                            player_id: player_id.clone(),
                            tick,
                        }).is_err() {
                            break Ok(());
                        }
                    }
                }
            }
        }
    };
    let _ = events.send(ConnectionEvent::Disconnected { player_id });
    result?;
    Ok(())
}

fn boat_parameters() -> BoatParameters {
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
