use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};
pub use simulation::WorldGeometry;

// Postcard encodes these enums and structs positionally. Reordering or changing
// fields is a wire-contract change and requires a new version. So is changing
// the world `game` generates from a match seed, because clients predict and
// render that world from the seed in Welcome. Predicted boat geometry must also
// agree between clients and servers.
pub const PROTOCOL_VERSION: u8 = 7;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientMessage {
    Input {
        sequence: i64,
        forward: f32,
        turn: f32,
    },
    Viewport {
        sequence: i64,
        radius: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ServerMessage {
    Welcome {
        player_id: String,
        match_seed: u64,
        world_geometry: WorldGeometry,
    },
    Snapshot {
        tick: u64,
        boats: Vec<BoatSnapshot>,
        removed_player_ids: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoatSnapshot {
    pub id: String,
    pub health: f32,
    pub last_hit_tick: Option<u64>,
    pub wreck: bool,
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub angle: f32,
    pub angular_velocity: f32,
    pub sail: f32,
    pub sail_angular_velocity: f32,
    pub rudder: f32,
    pub rudder_angular_velocity: f32,
    pub forward: Option<f32>,
    pub turn: Option<f32>,
    pub acknowledged_input: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    Codec(String),
    UnsupportedVersion(u8),
    MissingVersion,
    InvalidMotionIntent,
    InvalidViewportRadius,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codec(error) => write!(formatter, "protocol codec error: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported protocol version {version}")
            }
            Self::MissingVersion => formatter.write_str("protocol message is missing its version"),
            Self::InvalidMotionIntent => {
                formatter.write_str("motion intent values must be finite and between -1 and 1")
            }
            Self::InvalidViewportRadius => {
                formatter.write_str("viewport radius must be finite and positive")
            }
        }
    }
}

impl Error for ProtocolError {}

pub fn decode_client_message(message: &[u8]) -> Result<ClientMessage, ProtocolError> {
    let message: ClientMessage = decode_message(message)?;
    validate_client_message(&message)?;
    Ok(message)
}

pub fn encode_client_message(message: &ClientMessage) -> Result<Vec<u8>, ProtocolError> {
    validate_client_message(message)?;
    encode_message(message)
}

fn validate_client_message(message: &ClientMessage) -> Result<(), ProtocolError> {
    match message {
        ClientMessage::Input { forward, turn, .. }
            if !forward.is_finite()
                || !turn.is_finite()
                || !(-1.0..=1.0).contains(forward)
                || !(-1.0..=1.0).contains(turn) =>
        {
            Err(ProtocolError::InvalidMotionIntent)
        }
        ClientMessage::Viewport { radius, .. } if !radius.is_finite() || *radius <= 0.0 => {
            Err(ProtocolError::InvalidViewportRadius)
        }
        _ => Ok(()),
    }
}

pub fn decode_server_message(message: &[u8]) -> Result<ServerMessage, ProtocolError> {
    decode_message(message)
}

pub fn encode_server_message(message: &ServerMessage) -> Result<Vec<u8>, ProtocolError> {
    encode_message(message)
}

fn encode_message<T: Serialize>(message: &T) -> Result<Vec<u8>, ProtocolError> {
    postcard::to_extend(message, vec![PROTOCOL_VERSION])
        .map_err(|error| ProtocolError::Codec(error.to_string()))
}

fn decode_message<'a, T: Deserialize<'a>>(message: &'a [u8]) -> Result<T, ProtocolError> {
    let (&version, payload) = message.split_first().ok_or(ProtocolError::MissingVersion)?;
    if version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion(version));
    }
    postcard::from_bytes(payload).map_err(|error| ProtocolError::Codec(error.to_string()))
}
