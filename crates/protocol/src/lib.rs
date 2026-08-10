use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "input")]
    Input {
        sequence: i64,
        forward: f32,
        turn: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "welcome")]
    Welcome {
        #[serde(rename = "playerId")]
        player_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    Json(String),
    InvalidMotionIntent,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "protocol JSON error: {error}"),
            Self::InvalidMotionIntent => {
                formatter.write_str("motion intent values must be finite and between -1 and 1")
            }
        }
    }
}

impl Error for ProtocolError {}

pub fn decode_client_message(message: &str) -> Result<ClientMessage, ProtocolError> {
    let message: ClientMessage =
        serde_json::from_str(message).map_err(|error| ProtocolError::Json(error.to_string()))?;
    validate_client_message(&message)?;
    Ok(message)
}

pub fn encode_client_message(message: &ClientMessage) -> Result<String, ProtocolError> {
    validate_client_message(message)?;
    serde_json::to_string(message).map_err(|error| ProtocolError::Json(error.to_string()))
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
        _ => Ok(()),
    }
}

pub fn decode_server_message(message: &str) -> Result<ServerMessage, ProtocolError> {
    serde_json::from_str(message).map_err(|error| ProtocolError::Json(error.to_string()))
}

pub fn encode_server_message(message: &ServerMessage) -> Result<String, serde_json::Error> {
    serde_json::to_string(message)
}
