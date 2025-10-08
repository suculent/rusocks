//! Message types for rusocks communication protocol

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Base trait for all message types
pub trait Message: fmt::Debug + Send + Sync {
    /// Get the message type
    fn message_type(&self) -> &'static str;
}

/// Authentication message sent by client to server
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthMessage {
    /// Message type identifier
    pub message_type: String,
    
    /// Authentication token
    pub token: String,
    
    /// Whether this is a reverse proxy client
    pub reverse: bool,
    
    /// Client instance ID
    pub instance: Uuid,
}

impl Message for AuthMessage {
    fn message_type(&self) -> &'static str {
        "auth"
    }
}

impl AuthMessage {
    /// Create a new AuthMessage
    pub fn new(token: String, reverse: bool) -> Self {
        AuthMessage {
            message_type: "auth".to_string(),
            token,
            reverse,
            instance: Uuid::new_v4(),
        }
    }
}

/// Authentication response message sent by server to client
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthResponseMessage {
    /// Message type identifier
    pub message_type: String,
    
    /// Whether authentication was successful
    pub success: bool,
    
    /// Error message if authentication failed
    pub error: Option<String>,
}

impl Message for AuthResponseMessage {
    fn message_type(&self) -> &'static str {
        "auth_response"
    }
}

impl AuthResponseMessage {
    /// Create a new successful AuthResponseMessage
    pub fn success() -> Self {
        AuthResponseMessage {
            message_type: "auth_response".to_string(),
            success: true,
            error: None,
        }
    }
    
    /// Create a new failed AuthResponseMessage
    pub fn failure(error: String) -> Self {
        AuthResponseMessage {
            message_type: "auth_response".to_string(),
            success: false,
            error: Some(error),
        }
    }
}

/// Connect message sent by client to server to establish a new connection
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectMessage {
    /// Message type identifier
    pub message_type: String,
    
    /// Unique channel ID for this connection
    pub channel_id: Uuid,
    
    /// Target address to connect to (host:port)
    pub address: String,
}

impl Message for ConnectMessage {
    fn message_type(&self) -> &'static str {
        "connect"
    }
}

impl ConnectMessage {
    /// Create a new ConnectMessage
    pub fn new(address: String) -> Self {
        ConnectMessage {
            message_type: "connect".to_string(),
            channel_id: Uuid::new_v4(),
            address,
        }
    }
    
    /// Create a new ConnectMessage with a specific channel ID
    pub fn with_channel_id(address: String, channel_id: Uuid) -> Self {
        ConnectMessage {
            message_type: "connect".to_string(),
            channel_id,
            address,
        }
    }
}

/// Connect response message sent by server to client
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectResponseMessage {
    /// Message type identifier
    pub message_type: String,
    
    /// Channel ID this response is for
    pub channel_id: Uuid,
    
    /// Whether connection was successful
    pub success: bool,
    
    /// Error message if connection failed
    pub error: Option<String>,
}

impl Message for ConnectResponseMessage {
    fn message_type(&self) -> &'static str {
        "connect_response"
    }
}

impl ConnectResponseMessage {
    /// Create a new successful ConnectResponseMessage
    pub fn success(channel_id: Uuid) -> Self {
        ConnectResponseMessage {
            message_type: "connect_response".to_string(),
            channel_id,
            success: true,
            error: None,
        }
    }
    
    /// Create a new failed ConnectResponseMessage
    pub fn failure(channel_id: Uuid, error: String) -> Self {
        ConnectResponseMessage {
            message_type: "connect_response".to_string(),
            channel_id,
            success: false,
            error: Some(error),
        }
    }
}

/// Data message sent between client and server
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DataMessage {
    /// Message type identifier
    pub message_type: String,
    
    /// Channel ID this data is for
    pub channel_id: Uuid,
    
    /// Binary data payload (base64 encoded)
    pub data: String,
}

impl Message for DataMessage {
    fn message_type(&self) -> &'static str {
        "data"
    }
}

impl DataMessage {
    /// Create a new DataMessage
    pub fn new(channel_id: Uuid, data: Vec<u8>) -> Self {
        DataMessage {
            message_type: "data".to_string(),
            channel_id,
            data: base64::encode(&data),
        }
    }
    
    /// Get the decoded data
    pub fn get_data(&self) -> Result<Vec<u8>, base64::DecodeError> {
        base64::decode(&self.data)
    }
}

/// Disconnect message sent between client and server
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DisconnectMessage {
    /// Message type identifier
    pub message_type: String,
    
    /// Channel ID to disconnect
    pub channel_id: Uuid,
}

impl Message for DisconnectMessage {
    fn message_type(&self) -> &'static str {
        "disconnect"
    }
}

impl DisconnectMessage {
    /// Create a new DisconnectMessage
    pub fn new(channel_id: Uuid) -> Self {
        DisconnectMessage {
            message_type: "disconnect".to_string(),
            channel_id,
        }
    }
}

/// Partners message sent from server to client
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PartnersMessage {
    /// Message type identifier
    pub message_type: String,
    
    /// Number of available partners
    pub count: usize,
}

impl Message for PartnersMessage {
    fn message_type(&self) -> &'static str {
        "partners"
    }
}

impl PartnersMessage {
    /// Create a new PartnersMessage
    pub fn new(count: usize) -> Self {
        PartnersMessage {
            message_type: "partners".to_string(),
            count,
        }
    }
}

/// Connector message sent from client to server
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectorMessage {
    /// Message type identifier
    pub message_type: String,
    
    /// Unique channel ID for this operation
    pub channel_id: Uuid,
    
    /// Operation to perform (add, remove)
    pub operation: String,
    
    /// Connector token to manage
    pub connector_token: String,
}

impl Message for ConnectorMessage {
    fn message_type(&self) -> &'static str {
        "connector"
    }
}

impl ConnectorMessage {
    /// Create a new ConnectorMessage for adding a connector
    pub fn add(connector_token: String) -> Self {
        ConnectorMessage {
            message_type: "connector".to_string(),
            channel_id: Uuid::new_v4(),
            operation: "add".to_string(),
            connector_token,
        }
    }
    
    /// Create a new ConnectorMessage for removing a connector
    pub fn remove(connector_token: String) -> Self {
        ConnectorMessage {
            message_type: "connector".to_string(),
            channel_id: Uuid::new_v4(),
            operation: "remove".to_string(),
            connector_token,
        }
    }
}

/// Connector response message sent from server to client
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectorResponseMessage {
    /// Message type identifier
    pub message_type: String,
    
    /// Channel ID this response is for
    pub channel_id: Uuid,
    
    /// Whether operation was successful
    pub success: bool,
    
    /// Error message if operation failed
    pub error: Option<String>,
    
    /// Connector token (for add operations)
    pub connector_token: Option<String>,
}

impl Message for ConnectorResponseMessage {
    fn message_type(&self) -> &'static str {
        "connector_response"
    }
}

impl ConnectorResponseMessage {
    /// Create a new successful ConnectorResponseMessage for add operation
    pub fn add_success(channel_id: Uuid, connector_token: String) -> Self {
        ConnectorResponseMessage {
            message_type: "connector_response".to_string(),
            channel_id,
            success: true,
            error: None,
            connector_token: Some(connector_token),
        }
    }
    
    /// Create a new successful ConnectorResponseMessage for remove operation
    pub fn remove_success(channel_id: Uuid) -> Self {
        ConnectorResponseMessage {
            message_type: "connector_response".to_string(),
            channel_id,
            success: true,
            error: None,
            connector_token: None,
        }
    }
    
    /// Create a new failed ConnectorResponseMessage
    pub fn failure(channel_id: Uuid, error: String) -> Self {
        ConnectorResponseMessage {
            message_type: "connector_response".to_string(),
            channel_id,
            success: false,
            error: Some(error),
            connector_token: None,
        }
    }
}

/// Parse a JSON message into the appropriate message type
pub fn parse_message(json: &str) -> Result<Box<dyn Message>, serde_json::Error> {
    // First parse as a generic JSON object to get the message_type
    let value: serde_json::Value = serde_json::from_str(json)?;
    
    // Extract message_type
    let message_type = match value.get("message_type") {
        Some(serde_json::Value::String(s)) => s.as_str(),
        _ => {
            // Use a different approach since serde_json::Error::custom is not available in this context
            let err = serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Missing message_type field"
            ));
            return Err(err);
        },
    };
    
    // Parse into the appropriate message type
    match message_type {
        "auth" => {
            let msg: AuthMessage = serde_json::from_str(json)?;
            Ok(Box::new(msg))
        }
        "auth_response" => {
            let msg: AuthResponseMessage = serde_json::from_str(json)?;
            Ok(Box::new(msg))
        }
        "connect" => {
            let msg: ConnectMessage = serde_json::from_str(json)?;
            Ok(Box::new(msg))
        }
        "connect_response" => {
            let msg: ConnectResponseMessage = serde_json::from_str(json)?;
            Ok(Box::new(msg))
        }
        "data" => {
            let msg: DataMessage = serde_json::from_str(json)?;
            Ok(Box::new(msg))
        }
        "disconnect" => {
            let msg: DisconnectMessage = serde_json::from_str(json)?;
            Ok(Box::new(msg))
        }
        "partners" => {
            let msg: PartnersMessage = serde_json::from_str(json)?;
            Ok(Box::new(msg))
        }
        "connector" => {
            let msg: ConnectorMessage = serde_json::from_str(json)?;
            Ok(Box::new(msg))
        }
        "connector_response" => {
            let msg: ConnectorResponseMessage = serde_json::from_str(json)?;
            Ok(Box::new(msg))
        }
        _ => {
            // Use a different approach since serde_json::Error::custom is not available in this context
            let err = serde_json::Error::io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unknown message type: {}", message_type)
            ));
            Err(err)
        },
    }
}