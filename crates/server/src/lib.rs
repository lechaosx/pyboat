use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    io,
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use pyboat_game::{computed_yaw_rate_torques, dinghy_parameters, match_environment};
use pyboat_protocol::{
    BoatSnapshot, ClientMessage, ServerMessage, decode_client_message, encode_server_message,
};
use pyboat_simulation::{BoatMotion, BoatParameters, Environment, MotionIntent, SharedSimulation};
use rapier2d::prelude::Vector;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::time::{interval, timeout};
use tokio_tungstenite::{
    accept_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

type ConnectionResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const PHYSICS_BUBBLE_RADIUS_M: f32 = 8.0;
const VISIBILITY_RADIUS_M: f32 = 24.0;
const MAX_PENDING_INPUT_COMMANDS: usize = 15;
const MAX_CLIENT_MESSAGE_BYTES: usize = 1024;
const WEBSOCKET_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

enum ConnectionEvent {
    Connected {
        player_id: String,
        player_number: u64,
        outbound: watch::Sender<Option<Message>>,
    },
    Input {
        player_id: String,
        sequence: i64,
        intent: MotionIntent,
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
            parameters: dinghy_parameters(),
            environment: match_environment(),
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
        if player.pending_inputs.len() == MAX_PENDING_INPUT_COMMANDS {
            player.pending_inputs.pop_front();
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
            actuator_torques.push(computed_yaw_rate_torques(motion, player.intent));
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
    let (events, mut incoming_events) = mpsc::unbounded_channel();
    let mut connections: HashMap<String, watch::Sender<Option<Message>>> = HashMap::new();
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
                    connections.insert(player_id, outbound);
                }
                ConnectionEvent::Input { player_id, sequence, intent } => {
                    game_match.set_input(&player_id, sequence, intent);
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

async fn handle_connection(
    stream: TcpStream,
    player_id: String,
    player_number: u64,
    events: mpsc::UnboundedSender<ConnectionEvent>,
) -> ConnectionResult<()> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_CLIENT_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_CLIENT_MESSAGE_BYTES));
    let mut connection = timeout(
        WEBSOCKET_HANDSHAKE_TIMEOUT,
        accept_async_with_config(stream, Some(config)),
    )
    .await??;
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
                    if let Err(error) = connection.send(snapshot).await {
                        break Err(error);
                    }
                }
            }
        }
    };
    let _ = events.send(ConnectionEvent::Disconnected { player_id });
    result?;
    Ok(())
}
