use std::{
    collections::{HashMap, HashSet, VecDeque},
    error::Error,
    future::{Future, pending},
    io,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::{SinkExt, StreamExt};
use pyboat_game::{
    computed_yaw_rate_torques_in_environment, dinghy_parameters, procedural_match_environment,
};
use pyboat_protocol::{
    BoatSnapshot, ClientMessage, ServerMessage, decode_client_message, encode_server_message,
};
use pyboat_simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, HullContact, MotionIntent, SharedSimulation,
};
use rapier2d::prelude::Vector;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{interval, timeout};
use tokio_tungstenite::{
    WebSocketStream, accept_async_with_config,
    tungstenite::{
        Message,
        protocol::{CloseFrame, WebSocketConfig, frame::coding::CloseCode},
    },
};

type ConnectionResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const PHYSICS_BUBBLE_RADIUS_M: f32 = 8.0;
const MAX_VISIBILITY_RADIUS_M: f32 = 24.0;
const MAX_PENDING_INPUT_COMMANDS: usize = 15;
const MAX_CLIENT_MESSAGE_BYTES: usize = 1024;
const WEBSOCKET_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECTION_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
const BOAT_MAX_HEALTH: f32 = 100.0;
const SAFE_COLLISION_IMPULSE_N_S: f32 = 100.0;
const COLLISION_IMPULSE_PER_DAMAGE_N_S: f32 = 8.0;
const BOW_STRENGTH_MULTIPLIER: f32 = 2.0;
const SIDE_STRENGTH_MULTIPLIER: f32 = 1.0;
const STERN_STRENGTH_MULTIPLIER: f32 = 0.6;

enum ConnectionEvent {
    Upgraded {
        connection: Box<WebSocketStream<TcpStream>>,
    },
    Connected {
        player_id: String,
        player_number: u64,
        outbound: watch::Sender<Option<Message>>,
        registered: oneshot::Sender<()>,
    },
    Input {
        player_id: String,
        sequence: i64,
        intent: MotionIntent,
    },
    Viewport {
        player_id: String,
        radius_m: f32,
    },
    Disconnected {
        player_id: String,
    },
}

struct Player {
    id: String,
    intent: MotionIntent,
    acknowledged_input: i64,
    last_received_input: i64,
    pending_inputs: VecDeque<(i64, MotionIntent)>,
    visibility_radius_m: f32,
    health: f32,
    last_hit_tick: Option<u64>,
    wreck: bool,
}

struct GameMatch {
    parameters: BoatParameters,
    simulation: SharedSimulation,
    players: Vec<Player>,
    active_hull_contacts: HashSet<(String, String)>,
    match_seed: u64,
    tick: u64,
}

impl GameMatch {
    fn new(match_seed: u64) -> Self {
        Self {
            parameters: dinghy_parameters(),
            simulation: SharedSimulation::new(),
            players: Vec::new(),
            active_hull_contacts: HashSet::new(),
            match_seed,
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
            visibility_radius_m: MAX_VISIBILITY_RADIUS_M,
            health: BOAT_MAX_HEALTH,
            last_hit_tick: None,
            wreck: false,
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
        if self.players[player].wreck {
            return;
        }
        self.players.remove(player);
        self.simulation.remove_boat(player);
        self.active_hull_contacts
            .retain(|(first, second)| first != player_id && second != player_id);
    }

    fn set_input(&mut self, player_id: &str, sequence: i64, intent: MotionIntent) {
        let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        else {
            return;
        };
        if player.wreck {
            return;
        }
        if sequence <= player.last_received_input {
            return;
        }
        if player.pending_inputs.len() == MAX_PENDING_INPUT_COMMANDS {
            player.pending_inputs.pop_front();
        }
        player.pending_inputs.push_back((sequence, intent));
        player.last_received_input = sequence;
    }

    fn set_viewport(&mut self, player_id: &str, radius_m: f32) {
        let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        else {
            return;
        };
        player.visibility_radius_m =
            radius_m.clamp(PHYSICS_BUBBLE_RADIUS_M, MAX_VISIBILITY_RADIUS_M);
    }

    fn advance(&mut self) {
        let mut environments = Vec::with_capacity(self.players.len());
        let mut actuator_torques = Vec::with_capacity(self.players.len());
        for (boat, player) in self.players.iter_mut().enumerate() {
            let motion = self.simulation.motion(boat);
            let environment =
                procedural_match_environment(self.match_seed, self.tick, motion.position_m);
            environments.push(environment);
            if player.wreck {
                actuator_torques.push(ActuatorTorques::default());
                continue;
            }
            if let Some((sequence, intent)) = player.pending_inputs.pop_front() {
                player.intent = intent;
                player.acknowledged_input = sequence;
            }
            actuator_torques.push(computed_yaw_rate_torques_in_environment(
                motion,
                player.intent,
                environment,
            ));
        }
        let contacts = self
            .simulation
            .advance_with_environments(&environments, &actuator_torques);
        self.apply_hull_contacts(&contacts);
        self.tick += 1;
    }

    fn apply_collision_damage(&mut self, player_id: &str, impulse_n_s: f32, contact_x_m: f32) {
        let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        else {
            return;
        };
        if player.wreck {
            return;
        }
        let hull = self.parameters.hull;
        let strength = if contact_x_m >= hull.length_m / 2.0 - hull.bow_length_m {
            BOW_STRENGTH_MULTIPLIER
        } else if contact_x_m <= -hull.length_m / 4.0 {
            STERN_STRENGTH_MULTIPLIER
        } else {
            SIDE_STRENGTH_MULTIPLIER
        };
        let damage = (impulse_n_s - SAFE_COLLISION_IMPULSE_N_S).max(0.0)
            / (COLLISION_IMPULSE_PER_DAMAGE_N_S * strength);
        if damage == 0.0 {
            return;
        }
        player.health = (player.health - damage).max(0.0);
        player.last_hit_tick = Some(self.tick);
        if player.health == 0.0 {
            player.wreck = true;
            player.intent = MotionIntent::default();
            player.pending_inputs.clear();
        }
    }

    fn apply_hull_contacts(&mut self, contacts: &[HullContact]) {
        let mut active_hull_contacts = HashSet::with_capacity(contacts.len());
        for contact in contacts {
            let first_player_id = self.players[contact.first_boat].id.clone();
            let second_player_id = self.players[contact.second_boat].id.clone();
            let contact_pair = if first_player_id < second_player_id {
                (first_player_id.clone(), second_player_id.clone())
            } else {
                (second_player_id.clone(), first_player_id.clone())
            };
            let contact_began = !self.active_hull_contacts.contains(&contact_pair);
            active_hull_contacts.insert(contact_pair);
            if !contact_began {
                continue;
            }
            self.apply_collision_damage(
                &first_player_id,
                contact.impulse_magnitude_n_s,
                contact.first_contact_local_m.x,
            );
            self.apply_collision_damage(
                &second_player_id,
                contact.impulse_magnitude_n_s,
                contact.second_contact_local_m.x,
            );
        }
        self.active_hull_contacts = active_hull_contacts;
    }

    fn visible_boats(&self, player_id: &str) -> Vec<usize> {
        let viewer = self
            .players
            .iter()
            .position(|player| player.id == player_id)
            .expect("a connection must have an authoritative boat");
        let radius_m = self.players[viewer].visibility_radius_m;
        self.simulation
            .boats_within_radius(self.simulation.motion(viewer).position_m, radius_m)
    }

    fn snapshot_for(&self, player_id: &str, visible_boats: &[usize]) -> ServerMessage {
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
                    health: player.health,
                    last_hit_tick: player.last_hit_tick,
                    wreck: player.wreck,
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
            removed_player_ids: Vec::new(),
        }
    }
}

pub async fn serve(listener: TcpListener) -> ServerResult<()> {
    serve_until(listener, pending::<Result<(), io::Error>>()).await
}

pub async fn serve_until<F, E>(listener: TcpListener, shutdown: F) -> ServerResult<()>
where
    F: Future<Output = Result<(), E>>,
    E: Error + Send + Sync + 'static,
{
    let (events, mut incoming_events) = mpsc::unbounded_channel();
    let mut connections: HashMap<String, watch::Sender<Option<Message>>> = HashMap::new();
    let mut game_match = GameMatch::new(new_match_seed());
    let mut next_player_number = 1_u64;
    let mut ticker = interval(Duration::from_secs_f64(1.0 / 60.0));
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            result = &mut shutdown => {
                result.map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>)?;
                for outbound in connections.values() {
                    outbound.send_replace(Some(Message::Close(Some(CloseFrame {
                        code: CloseCode::Away,
                        reason: "Server shutting down".into(),
                    }))));
                }
                let _ = timeout(CONNECTION_SHUTDOWN_TIMEOUT, async {
                    while !connections.is_empty() {
                        let Some(event) = incoming_events.recv().await else {
                            break;
                        };
                        if let ConnectionEvent::Disconnected { player_id } = event {
                            connections.remove(&player_id);
                            game_match.remove_player(&player_id);
                        }
                    }
                })
                .await;
                return Ok(());
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let events = events.clone();
                tokio::spawn(async move {
                    match upgrade_connection(stream).await {
                        Ok(connection) => {
                            let _ = events.send(ConnectionEvent::Upgraded {
                                connection: Box::new(connection),
                            });
                        }
                        Err(error) => eprintln!("WebSocket connection failed: {error}"),
                    }
                });
            }
            Some(event) = incoming_events.recv() => match event {
                ConnectionEvent::Upgraded { connection } => {
                    let player_number = next_player_number;
                    let player_id = format!("player-{player_number}");
                    next_player_number += 1;
                    let events = events.clone();
                    tokio::spawn(async move {
                        if let Err(error) = handle_connection(
                            *connection,
                            player_id,
                            player_number,
                            game_match.match_seed,
                            events,
                        ).await {
                            eprintln!("WebSocket connection failed: {error}");
                        }
                    });
                }
                ConnectionEvent::Connected {
                    player_id,
                    player_number,
                    outbound,
                    registered,
                } => {
                    game_match.add_player(player_id.clone(), player_number);
                    connections.insert(player_id, outbound);
                    let _ = registered.send(());
                }
                ConnectionEvent::Input { player_id, sequence, intent } => {
                    game_match.set_input(&player_id, sequence, intent);
                }
                ConnectionEvent::Viewport { player_id, radius_m } => {
                    game_match.set_viewport(&player_id, radius_m);
                }
                ConnectionEvent::Disconnected { player_id } => {
                    connections.remove(&player_id);
                    game_match.remove_player(&player_id);
                }
            },
            _ = ticker.tick() => {
                game_match.advance();
                for (player_id, outbound) in &connections {
                    let visible_boats = game_match.visible_boats(player_id);
                    let snapshot = encode_server_message(
                        &game_match.snapshot_for(player_id, &visible_boats),
                    )?;
                    outbound.send_replace(Some(Message::Text(snapshot.into())));
                }
            }
        }
    }
}

async fn upgrade_connection(stream: TcpStream) -> ConnectionResult<WebSocketStream<TcpStream>> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_CLIENT_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_CLIENT_MESSAGE_BYTES));
    Ok(timeout(
        WEBSOCKET_HANDSHAKE_TIMEOUT,
        accept_async_with_config(stream, Some(config)),
    )
    .await??)
}

async fn handle_connection(
    mut connection: WebSocketStream<TcpStream>,
    player_id: String,
    player_number: u64,
    match_seed: u64,
    events: mpsc::UnboundedSender<ConnectionEvent>,
) -> ConnectionResult<()> {
    // A watch channel retains only the newest snapshot when a client cannot keep up.
    let (outbound, mut outbound_updates) = watch::channel(None);
    let (registered, registration) = oneshot::channel();
    events
        .send(ConnectionEvent::Connected {
            player_id: player_id.clone(),
            player_number,
            outbound,
            registered,
        })
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "server stopped"))?;
    registration
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "server stopped"))?;
    let welcome = encode_server_message(&ServerMessage::Welcome {
        player_id: player_id.clone(),
        match_seed,
    })?;
    connection.send(Message::Text(welcome.into())).await?;

    let result = loop {
        tokio::select! {
            incoming = connection.next() => match incoming {
                Some(Ok(message)) if message.is_text() => {
                    match decode_client_message(message.to_text()?)? {
                        ClientMessage::Input { sequence, forward, turn } => {
                            if events.send(ConnectionEvent::Input {
                                player_id: player_id.clone(),
                                sequence,
                                intent: MotionIntent { forward, turn },
                            }).is_err() {
                                break Ok(());
                            }
                        }
                        ClientMessage::Viewport { radius } => {
                            if events.send(ConnectionEvent::Viewport {
                                player_id: player_id.clone(),
                                radius_m: radius,
                            }).is_err() {
                                break Ok(());
                            }
                        }
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
                let message = outbound_updates.borrow_and_update().clone();
                if let Some(message) = message {
                    let closes_connection = message.is_close();
                    if let Err(error) = connection.send(message).await {
                        break Err(error);
                    }
                    if closes_connection {
                        break Ok(());
                    }
                }
            }
        }
    };
    let _ = events.send(ConnectionEvent::Disconnected { player_id });
    result?;
    Ok(())
}

fn new_match_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after the Unix epoch")
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player<'a>(game_match: &'a GameMatch, player_id: &str) -> &'a Player {
        game_match
            .players
            .iter()
            .find(|player| player.id == player_id)
            .unwrap()
    }

    #[test]
    fn collision_damage_respects_bow_side_and_stern_strength() {
        let mut losses = Vec::new();
        for contact_x_m in [1.5, 0.0, -1.5] {
            let mut game_match = GameMatch::new(42);
            game_match.add_player("player-1".to_owned(), 1);
            game_match.apply_collision_damage("player-1", 580.0, contact_x_m);
            losses.push(100.0 - player(&game_match, "player-1").health);
        }

        assert!((losses[0] - 30.0).abs() < 1.0e-5);
        assert!((losses[1] - 60.0).abs() < 1.0e-5);
        assert!((losses[2] - 100.0).abs() < 1.0e-5);
    }

    #[test]
    fn collision_damage_applies_only_when_contact_begins() {
        let mut game_match = GameMatch::new(42);
        game_match.add_player("player-1".to_owned(), 1);
        game_match.add_player("player-2".to_owned(), 2);
        let contact = HullContact {
            first_boat: 0,
            second_boat: 1,
            impulse_magnitude_n_s: 180.0,
            first_contact_local_m: Vector::ZERO,
            second_contact_local_m: Vector::ZERO,
        };

        game_match.apply_hull_contacts(&[contact]);
        let after_first_contact = player(&game_match, "player-1").health;
        game_match.apply_hull_contacts(&[contact]);
        assert_eq!(player(&game_match, "player-1").health, after_first_contact);

        game_match.apply_hull_contacts(&[]);
        game_match.apply_hull_contacts(&[contact]);
        assert!(player(&game_match, "player-1").health < after_first_contact);
    }

    #[test]
    fn destroyed_boat_becomes_a_persistent_spectated_wreck() {
        let mut game_match = GameMatch::new(42);
        game_match.add_player("player-1".to_owned(), 1);
        game_match.set_input(
            "player-1",
            1,
            MotionIntent {
                forward: 1.0,
                turn: 1.0,
            },
        );
        game_match.apply_collision_damage("player-1", 2000.0, -1.5);

        let destroyed = player(&game_match, "player-1");
        assert_eq!(destroyed.health, 0.0);
        assert_eq!(destroyed.last_hit_tick, Some(0));
        assert!(destroyed.wreck);
        assert_eq!(destroyed.intent, MotionIntent::default());
        assert!(destroyed.pending_inputs.is_empty());

        game_match.remove_player("player-1");
        assert!(
            game_match
                .players
                .iter()
                .any(|player| player.id == "player-1")
        );
        let ServerMessage::Snapshot { boats, .. } = game_match.snapshot_for("player-1", &[0])
        else {
            unreachable!();
        };
        assert_eq!(boats[0].health, 0.0);
        assert_eq!(boats[0].last_hit_tick, Some(0));
        assert!(boats[0].wreck);

        game_match.advance();
        let ServerMessage::Snapshot { boats, .. } = game_match.snapshot_for("player-1", &[0])
        else {
            unreachable!();
        };
        assert_eq!(boats[0].health, 0.0);
        assert!(boats[0].wreck);
    }
}
