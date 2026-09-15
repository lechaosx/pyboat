use std::{
    collections::{HashMap, HashSet, VecDeque},
    error::Error,
    future::{Future, pending},
    io,
    net::SocketAddr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use game::{
    computed_yaw_rate_torques_in_environment, dinghy_parameters, procedural_match_environment,
};
use protocol::{
    BoatSnapshot, ClientMessage, ServerMessage, decode_client_message, encode_server_message,
};
use rapier2d::prelude::Vector;
use simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, HullContact, MotionIntent, SharedSimulation,
    WorldGeometry,
};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{interval, timeout};
use wtransport::{
    Connection, Endpoint, Identity, ServerConfig, VarInt,
    endpoint::{IncomingSession, endpoint_side::Server},
    error::SendDatagramError,
    stream::RecvStream,
};

type ConnectionResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const PHYSICS_BUBBLE_RADIUS_M: f32 = 8.0;
const MAX_VISIBILITY_RADIUS_M: f32 = 24.0;
const MAX_PENDING_INPUT_COMMANDS: usize = 15;
const MAX_PENDING_CONNECTION_EVENTS: usize = 256;
const MAX_PENDING_CONNECTION_MESSAGES: usize = 16;
const MAX_CLIENT_MESSAGE_BYTES: usize = 1024;
const WEBTRANSPORT_PATH: &str = "/game";
const WEBTRANSPORT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECTION_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
const SPAWN_CLEARANCE_M: f32 = 8.0;
const MINIMUM_SPAWN_WATER_DEPTH_M: f32 = 1.5;
const SPAWN_BEARING_CANDIDATES: u32 = 32;
const BOAT_MAX_HEALTH: f32 = 100.0;
const SAFE_COLLISION_IMPULSE_N_S: f32 = 100.0;
const COLLISION_IMPULSE_PER_DAMAGE_N_S: f32 = 8.0;
const BOW_STRENGTH_MULTIPLIER: f32 = 2.0;
const SIDE_STRENGTH_MULTIPLIER: f32 = 1.0;
const STERN_STRENGTH_MULTIPLIER: f32 = 0.6;

enum ConnectionEvent {
    Upgraded {
        connection: Connection,
    },
    Connected {
        player_id: String,
        player_number: u64,
        outbound: watch::Sender<Option<OutboundMessage>>,
        registered: oneshot::Sender<()>,
    },
    Input {
        player_id: String,
        sequence: i64,
        intent: MotionIntent,
    },
    Viewport {
        player_id: String,
        sequence: i64,
        radius_m: f32,
    },
    Disconnected {
        player_id: String,
    },
}

enum IncomingMessage {
    Datagram(ClientMessage),
    Reliable(ClientMessage),
}

#[derive(Clone)]
enum OutboundMessage {
    Snapshot(Vec<u8>),
    Close,
}

struct Player {
    id: String,
    intent: MotionIntent,
    acknowledged_input: i64,
    last_received_input: i64,
    pending_inputs: VecDeque<(i64, MotionIntent)>,
    last_received_viewport: Option<i64>,
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
    world_geometry: WorldGeometry,
    spawn_origin_m: Option<Vector>,
    tick: u64,
}

impl GameMatch {
    #[cfg(test)]
    fn new(match_seed: u64) -> Self {
        Self::new_with_world_geometry(match_seed, WorldGeometry::default())
    }

    fn new_with_world_geometry(match_seed: u64, world_geometry: WorldGeometry) -> Self {
        Self {
            parameters: dinghy_parameters(),
            simulation: SharedSimulation::new_with_world_geometry(world_geometry),
            players: Vec::new(),
            active_hull_contacts: HashSet::new(),
            match_seed,
            world_geometry,
            spawn_origin_m: None,
            tick: 0,
        }
    }

    fn add_player(&mut self, player_id: String, player_number: u64) {
        let spawn_origin_m = self.spawn_origin_m.unwrap_or(Vector::ZERO);
        let fleet_radius_m = (0..self.players.len())
            .map(|boat| {
                Vector::from(self.world_geometry.minimum_image_displacement_m(
                    (self.simulation.motion(boat).position_m - spawn_origin_m).into(),
                ))
                .length()
            })
            .fold(0.0_f32, f32::max);
        let minimum_radius_m = if self.players.is_empty() {
            0.0
        } else {
            fleet_radius_m + SPAWN_CLEARANCE_M
        };
        let mut random_bits = self.match_seed ^ player_number.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        random_bits = (random_bits ^ (random_bits >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        random_bits = (random_bits ^ (random_bits >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        random_bits ^= random_bits >> 31;
        let initial_bearing_rad =
            std::f32::consts::TAU * ((random_bits >> 40) as f32 / (1_u32 << 24) as f32);
        let mut radius_m = minimum_radius_m;
        let position_m = 'spawn: loop {
            for candidate in 0..SPAWN_BEARING_CANDIDATES {
                let bearing_rad = initial_bearing_rad
                    + std::f32::consts::TAU * candidate as f32 / SPAWN_BEARING_CANDIDATES as f32;
                let candidate_m =
                    spawn_origin_m + Vector::new(bearing_rad.cos(), bearing_rad.sin()) * radius_m;
                let candidate_m =
                    Vector::from(self.world_geometry.wrap_position_m(candidate_m.into()));
                if procedural_match_environment(
                    self.world_geometry,
                    self.match_seed,
                    self.tick,
                    candidate_m,
                )
                .water_depth_m
                    >= MINIMUM_SPAWN_WATER_DEPTH_M
                {
                    break 'spawn candidate_m;
                }
                if radius_m == 0.0 {
                    break;
                }
            }
            radius_m = radius_m.max(minimum_radius_m) + SPAWN_CLEARANCE_M;
        };
        self.spawn_origin_m.get_or_insert(position_m);
        let wind = procedural_match_environment(
            self.world_geometry,
            self.match_seed,
            self.tick,
            position_m,
        )
        .true_wind_velocity_m_s;
        self.simulation.add_boat(
            self.parameters,
            BoatMotion {
                position_m,
                velocity_m_s: Vector::ZERO,
                angle_rad: wind.y.atan2(wind.x),
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
            last_received_viewport: None,
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

    fn set_viewport(&mut self, player_id: &str, sequence: i64, radius_m: f32) {
        let Some(player) = self
            .players
            .iter_mut()
            .find(|player| player.id == player_id)
        else {
            return;
        };
        if player
            .last_received_viewport
            .is_some_and(|last_sequence| sequence <= last_sequence)
        {
            return;
        }
        player.last_received_viewport = Some(sequence);
        player.visibility_radius_m =
            radius_m.clamp(PHYSICS_BUBBLE_RADIUS_M, MAX_VISIBILITY_RADIUS_M);
    }

    fn advance(&mut self) {
        let mut environments = Vec::with_capacity(self.players.len());
        let mut actuator_torques = Vec::with_capacity(self.players.len());
        for (boat, player) in self.players.iter_mut().enumerate() {
            let motion = self.simulation.motion(boat);
            let environment = procedural_match_environment(
                self.world_geometry,
                self.match_seed,
                self.tick,
                motion.position_m,
            );
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
                let include_intent =
                    Vector::from(self.world_geometry.minimum_image_displacement_m(
                        (motion.position_m - viewer_position_m).into(),
                    ))
                    .length_squared()
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

pub fn endpoint(address: SocketAddr, identity: Identity) -> ServerResult<Endpoint<Server>> {
    let config = ServerConfig::builder()
        .with_bind_address(address)
        .with_identity(identity)
        .keep_alive_interval(Some(Duration::from_secs(3)))
        .build();
    Ok(Endpoint::server(config)?)
}

pub async fn serve(endpoint: Endpoint<Server>) -> ServerResult<()> {
    serve_with_world_geometry(endpoint, WorldGeometry::default()).await
}

pub async fn serve_with_world_geometry(
    endpoint: Endpoint<Server>,
    world_geometry: WorldGeometry,
) -> ServerResult<()> {
    serve_with_world_geometry_until(endpoint, world_geometry, pending::<Result<(), io::Error>>())
        .await
}

pub async fn serve_until<F, E>(endpoint: Endpoint<Server>, shutdown: F) -> ServerResult<()>
where
    F: Future<Output = Result<(), E>>,
    E: Error + Send + Sync + 'static,
{
    serve_with_world_geometry_until(endpoint, WorldGeometry::default(), shutdown).await
}

pub async fn serve_with_world_geometry_until<F, E>(
    endpoint: Endpoint<Server>,
    world_geometry: WorldGeometry,
    shutdown: F,
) -> ServerResult<()>
where
    F: Future<Output = Result<(), E>>,
    E: Error + Send + Sync + 'static,
{
    let (events, mut incoming_events) = mpsc::channel(MAX_PENDING_CONNECTION_EVENTS);
    let mut connections: HashMap<String, watch::Sender<Option<OutboundMessage>>> = HashMap::new();
    let mut game_match = GameMatch::new_with_world_geometry(new_match_seed(), world_geometry);
    let mut next_player_number = 1_u64;
    let mut ticker = interval(Duration::from_secs_f64(1.0 / 60.0));
    let mut upgrades = tokio::task::JoinSet::new();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            result = &mut shutdown => {
                result.map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>)?;
                endpoint.close(VarInt::from_u32(1), b"Server shutting down");
                for outbound in connections.values() {
                    outbound.send_replace(Some(OutboundMessage::Close));
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
                    // Keep the QUIC socket alive while its connection-close
                    // frames drain; dropping the endpoint here can strand peers
                    // until their idle timeout expires.
                    endpoint.wait_idle().await;
                })
                .await;
                return Ok(());
            }
            incoming_session = endpoint.accept(), if upgrades.len() < MAX_PENDING_CONNECTION_EVENTS => {
                let events = events.clone();
                upgrades.spawn(async move {
                    match upgrade_connection(incoming_session).await {
                        Ok(connection) => {
                            let _ = events.send(ConnectionEvent::Upgraded {
                                connection,
                            }).await;
                        }
                        Err(error) => eprintln!("WebTransport connection failed: {error}"),
                    }
                });
            }
            Some(result) = upgrades.join_next(), if !upgrades.is_empty() => {
                if let Err(error) = result {
                    eprintln!("WebTransport connection upgrade task failed: {error}");
                }
            }
            Some(event) = incoming_events.recv() => match event {
                ConnectionEvent::Upgraded { connection } => {
                    let player_number = next_player_number;
                    let player_id = format!("player-{player_number}");
                    next_player_number += 1;
                    let events = events.clone();
                    tokio::spawn(async move {
                        if let Err(error) = handle_connection(
                            connection,
                            player_id,
                            player_number,
                            game_match.match_seed,
                            game_match.world_geometry,
                            events,
                        ).await {
                            eprintln!("WebTransport connection failed: {error}");
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
                ConnectionEvent::Viewport { player_id, sequence, radius_m } => {
                    game_match.set_viewport(&player_id, sequence, radius_m);
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
                    outbound.send_replace(Some(OutboundMessage::Snapshot(snapshot)));
                }
            }
        }
    }
}

async fn upgrade_connection(incoming_session: IncomingSession) -> ConnectionResult<Connection> {
    let request = timeout(WEBTRANSPORT_HANDSHAKE_TIMEOUT, incoming_session).await??;
    if request.path() != WEBTRANSPORT_PATH {
        request.not_found().await;
        return Err(io::Error::new(io::ErrorKind::NotFound, "unknown WebTransport path").into());
    }
    Ok(request.accept().await?)
}

async fn handle_connection(
    connection: Connection,
    player_id: String,
    player_number: u64,
    match_seed: u64,
    world_geometry: WorldGeometry,
    events: mpsc::Sender<ConnectionEvent>,
) -> ConnectionResult<()> {
    let welcome = encode_server_message(&ServerMessage::Welcome {
        player_id: player_id.clone(),
        match_seed,
        world_geometry,
    })?;
    send_reliable_message(&connection, &welcome).await?;
    let (outbound, mut outbound_updates) = watch::channel(None);
    let (registered, registration) = oneshot::channel();
    events
        .send(ConnectionEvent::Connected {
            player_id: player_id.clone(),
            player_number,
            outbound,
            registered,
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "server stopped"))?;
    registration
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "server stopped"))?;
    let (mut incoming_messages, receive_tasks) = receive_messages(&connection);

    let result: ConnectionResult<()> = async {
        loop {
            tokio::select! {
                incoming = incoming_messages.recv() => match incoming {
                    Some(Ok(IncomingMessage::Datagram(ClientMessage::Input {
                        sequence,
                        forward,
                        turn,
                    }))) => {
                        if events.send(ConnectionEvent::Input {
                            player_id: player_id.clone(),
                            sequence,
                            intent: MotionIntent { forward, turn },
                        }).await.is_err() {
                            break Ok(());
                        }
                    }
                    Some(Ok(IncomingMessage::Reliable(ClientMessage::Viewport {
                        sequence,
                        radius,
                    }))) => {
                        if events.send(ConnectionEvent::Viewport {
                            player_id: player_id.clone(),
                            sequence,
                            radius_m: radius,
                        }).await.is_err() {
                            break Ok(());
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => break Err(error),
                    None => break Ok(()),
                },
                changed = outbound_updates.changed() => {
                    if changed.is_err() {
                        break Ok(());
                    }
                    let message = outbound_updates.borrow_and_update().clone();
                    match message {
                        Some(OutboundMessage::Snapshot(message)) => {
                            match connection.send_datagram(&message) {
                                Ok(()) => {}
                                Err(SendDatagramError::TooLarge) => {
                                    send_reliable_message(&connection, &message).await?;
                                }
                                Err(error) => break Err(error.into()),
                            }
                        }
                        Some(OutboundMessage::Close) => {
                            connection.close(VarInt::from_u32(1), b"Server shutting down");
                            break Ok(());
                        }
                        None => {}
                    }
                }
            }
        }
    }
    .await;
    for task in receive_tasks {
        task.abort();
    }
    let _ = events
        .send(ConnectionEvent::Disconnected { player_id })
        .await;
    result?;
    Ok(())
}

fn receive_messages(
    connection: &Connection,
) -> (
    mpsc::Receiver<ConnectionResult<IncomingMessage>>,
    [tokio::task::JoinHandle<()>; 2],
) {
    let (messages, receiver) = mpsc::channel(MAX_PENDING_CONNECTION_MESSAGES);
    let datagram_connection = connection.clone();
    let datagram_messages = messages.clone();
    let datagram_task = tokio::spawn(async move {
        loop {
            let message: ConnectionResult<IncomingMessage> =
                match datagram_connection.receive_datagram().await {
                    Ok(message) if message.len() <= MAX_CLIENT_MESSAGE_BYTES => {
                        decode_client_message(&message)
                            .map(IncomingMessage::Datagram)
                            .map_err(|error| error.into())
                    }
                    Ok(_) => Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "client datagram exceeds message limit",
                    )
                    .into()),
                    Err(error) => Err(error.into()),
                };
            let failed = message.is_err();
            if datagram_messages.send(message).await.is_err() || failed {
                break;
            }
        }
    });

    let stream_connection = connection.clone();
    let stream_task = tokio::spawn(async move {
        let mut reads = tokio::task::JoinSet::new();
        loop {
            if reads.len() >= MAX_PENDING_CONNECTION_MESSAGES {
                reads.join_next().await;
            }
            match stream_connection.accept_uni().await {
                Ok(stream) => {
                    let messages = messages.clone();
                    reads.spawn(async move {
                        let message = receive_reliable_message(stream)
                            .await
                            .and_then(|message| {
                                decode_client_message(&message).map_err(|error| error.into())
                            })
                            .map(IncomingMessage::Reliable);
                        let _ = messages.send(message).await;
                    });
                }
                Err(error) => {
                    let _ = messages.send(Err(error.into())).await;
                    break;
                }
            }
        }
    });
    (receiver, [datagram_task, stream_task])
}

async fn send_reliable_message(connection: &Connection, message: &[u8]) -> ConnectionResult<()> {
    let mut stream = connection.open_uni().await?.await?;
    stream.write_all(message).await?;
    stream.finish().await?;
    Ok(())
}

async fn receive_reliable_message(mut stream: RecvStream) -> ConnectionResult<Vec<u8>> {
    let mut message = Vec::new();
    let mut buffer = [0; 256];
    while let Some(read) = stream.read(&mut buffer).await? {
        if message.len() + read > MAX_CLIENT_MESSAGE_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "client stream exceeds message limit",
            )
            .into());
        }
        message.extend_from_slice(&buffer[..read]);
    }
    Ok(message)
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

    fn game_match_with_boats(world_geometry: WorldGeometry, positions_m: &[Vector]) -> GameMatch {
        let parameters = dinghy_parameters();
        let mut simulation = SharedSimulation::new_with_world_geometry(world_geometry);
        let mut players = Vec::new();
        for (index, position_m) in positions_m.iter().copied().enumerate() {
            simulation.add_boat(
                parameters,
                BoatMotion {
                    position_m,
                    velocity_m_s: Vector::ZERO,
                    angle_rad: 0.0,
                    angular_velocity_rad_s: 0.0,
                    sail_angle_rad: 0.0,
                    sail_angular_velocity_rad_s: 0.0,
                    rudder_angle_rad: 0.0,
                    rudder_angular_velocity_rad_s: 0.0,
                },
            );
            players.push(Player {
                id: format!("player-{}", index + 1),
                intent: MotionIntent::default(),
                acknowledged_input: 0,
                last_received_input: 0,
                pending_inputs: VecDeque::new(),
                last_received_viewport: None,
                visibility_radius_m: MAX_VISIBILITY_RADIUS_M,
                health: BOAT_MAX_HEALTH,
                last_hit_tick: None,
                wreck: false,
            });
        }
        GameMatch {
            parameters,
            simulation,
            players,
            active_hull_contacts: HashSet::new(),
            match_seed: 42,
            world_geometry,
            spawn_origin_m: None,
            tick: 0,
        }
    }

    fn ram_result(
        offset_m: Vector,
        speed_m_s: f32,
        half_separation_m: f32,
    ) -> (f32, [f32; 2], [bool; 2]) {
        let world_geometry = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
        let mut parameters = dinghy_parameters();
        parameters.air_density_kg_m3 = 0.0;
        parameters.water_density_kg_m3 = 0.0;
        let mut simulation = SharedSimulation::new_with_world_geometry(world_geometry);
        for (position_m, velocity_m_s) in [
            (
                offset_m + Vector::new(-half_separation_m, 0.0),
                Vector::new(speed_m_s, 0.0),
            ),
            (
                offset_m + Vector::new(half_separation_m, 0.0),
                Vector::new(-speed_m_s, 0.0),
            ),
        ] {
            simulation.add_boat(
                parameters,
                BoatMotion {
                    position_m,
                    velocity_m_s,
                    angle_rad: 0.0,
                    angular_velocity_rad_s: 0.0,
                    sail_angle_rad: 0.0,
                    sail_angular_velocity_rad_s: 0.0,
                    rudder_angle_rad: 0.0,
                    rudder_angular_velocity_rad_s: 0.0,
                },
            );
        }
        let mut game_match = GameMatch {
            parameters,
            simulation,
            players: (1..=2)
                .map(|number| Player {
                    id: format!("player-{number}"),
                    intent: MotionIntent::default(),
                    acknowledged_input: 0,
                    last_received_input: 0,
                    pending_inputs: VecDeque::new(),
                    last_received_viewport: None,
                    visibility_radius_m: MAX_VISIBILITY_RADIUS_M,
                    health: BOAT_MAX_HEALTH,
                    last_hit_tick: None,
                    wreck: false,
                })
                .collect(),
            active_hull_contacts: HashSet::new(),
            match_seed: 42,
            world_geometry,
            spawn_origin_m: None,
            tick: 0,
        };
        let mut maximum_impulse_n_s = 0.0_f32;
        for _ in 0..30 {
            let contacts = game_match.simulation.advance(
                simulation::Environment::default(),
                &[ActuatorTorques::default(), ActuatorTorques::default()],
            );
            maximum_impulse_n_s = contacts
                .iter()
                .fold(maximum_impulse_n_s, |maximum, contact| {
                    maximum.max(contact.impulse_magnitude_n_s)
                });
            game_match.apply_hull_contacts(&contacts);
            game_match.tick += 1;
        }
        (
            maximum_impulse_n_s,
            [game_match.players[0].health, game_match.players[1].health],
            [game_match.players[0].wreck, game_match.players[1].wreck],
        )
    }

    fn player<'a>(game_match: &'a GameMatch, player_id: &str) -> &'a Player {
        game_match
            .players
            .iter()
            .find(|player| player.id == player_id)
            .unwrap()
    }

    #[test]
    fn new_players_spawn_safely_beyond_the_fleet_and_point_downwind() {
        let mut game_match = GameMatch::new(42);
        for player_number in 1..=5 {
            let spawn_origin_m = game_match.spawn_origin_m.unwrap_or(Vector::ZERO);
            let fleet_radius_m = (0..game_match.players.len())
                .map(|boat| {
                    (game_match.simulation.motion(boat).position_m - spawn_origin_m).length()
                })
                .fold(0.0_f32, f32::max);
            game_match.add_player(format!("player-{player_number}"), player_number);

            let spawned = game_match.simulation.motion(player_number as usize - 1);
            if player_number > 1 {
                assert!(
                    (spawned.position_m - game_match.spawn_origin_m.unwrap()).length()
                        >= fleet_radius_m + SPAWN_CLEARANCE_M
                );
            }
            let environment = procedural_match_environment(
                game_match.world_geometry,
                game_match.match_seed,
                game_match.tick,
                spawned.position_m,
            );
            assert!(environment.water_depth_m >= MINIMUM_SPAWN_WATER_DEPTH_M);
            let forward = Vector::new(spawned.angle_rad.cos(), spawned.angle_rad.sin());
            assert!(forward.dot(environment.true_wind_velocity_m_s.normalize()) > 0.999_99);
        }

        let mut repeated_match = GameMatch::new(42);
        repeated_match.add_player("player-1".to_owned(), 1);
        repeated_match.add_player("player-2".to_owned(), 2);
        assert_eq!(
            game_match.simulation.motion(0).position_m,
            repeated_match.simulation.motion(0).position_m,
        );
        assert_eq!(
            game_match.simulation.motion(1).position_m,
            repeated_match.simulation.motion(1).position_m,
        );

        let mut other_match = GameMatch::new(43);
        other_match.add_player("player-1".to_owned(), 1);
        other_match.add_player("player-2".to_owned(), 2);
        assert_ne!(
            game_match.simulation.motion(1).position_m,
            other_match.simulation.motion(1).position_m,
        );
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
    fn longitude_wrapped_interest_excludes_the_opposite_pole() {
        let world = WorldGeometry::new(8_000.0, 8_000.0).unwrap();
        let game_match = game_match_with_boats(
            world,
            &[
                Vector::new(3_999.0, 3_999.0),
                Vector::new(-3_999.0, 3_997.0),
                Vector::new(-3_980.0, 3_978.0),
                Vector::new(-3_999.0, -3_999.0),
            ],
        );

        assert_eq!(game_match.visible_boats("player-1"), vec![0, 1]);
        let ServerMessage::Snapshot { boats, .. } =
            game_match.snapshot_for("player-1", &[0, 1, 2, 3])
        else {
            unreachable!();
        };
        assert!(boats[1].forward.is_some());
        assert!(boats[1].turn.is_some());
        assert!(boats[2].forward.is_none());
        assert!(boats[2].turn.is_none());
        assert!(boats[3].forward.is_none());
        assert!(boats[3].turn.is_none());
    }

    #[test]
    fn collision_damage_and_wreck_outcome_match_across_the_x_seam() {
        let central = ram_result(Vector::ZERO, 6.0, 2.5);
        let seam = ram_result(Vector::new(4_000.0, 0.0), 6.0, 2.5);

        assert!((central.1[0] - seam.1[0]).abs() < 1.0);
        assert!((central.1[1] - seam.1[1]).abs() < 1.0);
        assert_eq!(central.2, seam.2);
    }

    #[test]
    fn bounded_high_speed_collision_damage_matches_across_the_x_seam() {
        let central = ram_result(Vector::ZERO, 10_000.0, 8.0);
        let seam = ram_result(Vector::new(4_000.0, 0.0), 10_000.0, 8.0);

        assert!(central.0 > 0.0);
        assert!((central.0 - seam.0).abs() < 5.0);
        assert_eq!(central.1, seam.1);
        assert_eq!(central.2, seam.2);
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
