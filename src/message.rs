//! Message types for rusocks communication protocol
//! Implements LinkSocks binary protocol for compatibility

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Protocol version
pub const PROTOCOL_VERSION: u8 = 0x01;

/// Binary message types
pub const BINARY_TYPE_AUTH: u8 = 0x01;
pub const BINARY_TYPE_AUTH_RESPONSE: u8 = 0x02;
pub const BINARY_TYPE_CONNECT: u8 = 0x03;
pub const BINARY_TYPE_DATA: u8 = 0x04;
pub const BINARY_TYPE_CONNECT_RESPONSE: u8 = 0x05;
pub const BINARY_TYPE_DISCONNECT: u8 = 0x06;
pub const BINARY_TYPE_CONNECTOR: u8 = 0x07;
pub const BINARY_TYPE_CONNECTOR_RESPONSE: u8 = 0x08;
pub const BINARY_TYPE_LOG: u8 = 0x09;
pub const BINARY_TYPE_PARTNERS: u8 = 0x0A;

/// Protocol types
pub const BINARY_PROTOCOL_TCP: u8 = 0x01;
pub const BINARY_PROTOCOL_UDP: u8 = 0x02;

/// Connector operations
pub const BINARY_CONNECTOR_OPERATION_ADD: u8 = 0x01;
pub const BINARY_CONNECTOR_OPERATION_REMOVE: u8 = 0x02;

/// Compression flags
pub const DATA_COMPRESSION_NONE: u8 = 0x00;
pub const DATA_COMPRESSION_GZIP: u8 = 0x01;

/// Base trait for all message types
pub trait Message: fmt::Debug + Send + Sync {
    /// Get the message type
    fn message_type(&self) -> &'static str;

    /// Pack message into binary format
    fn pack(&self) -> Result<Vec<u8>, String>;
}

/// Helper function to convert UUID to bytes
fn uuid_to_bytes(uuid: &Uuid) -> [u8; 16] {
    *uuid.as_bytes()
}

/// Helper function to convert bytes to UUID
fn bytes_to_uuid(bytes: &[u8]) -> Result<Uuid, String> {
    if bytes.len() != 16 {
        return Err(format!("Invalid UUID length: {}", bytes.len()));
    }
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(bytes);
    Ok(Uuid::from_bytes(uuid_bytes))
}

/// Helper function to convert bool to byte
fn bool_to_byte(b: bool) -> u8 {
    if b {
        1
    } else {
        0
    }
}

/// Helper function to convert byte to bool
fn byte_to_bool(b: u8) -> bool {
    b != 0
}

/// Helper function to convert protocol string to byte
fn protocol_to_byte(protocol: &str) -> u8 {
    match protocol {
        "tcp" => BINARY_PROTOCOL_TCP,
        "udp" => BINARY_PROTOCOL_UDP,
        _ => BINARY_PROTOCOL_TCP,
    }
}

/// Helper function to convert byte to protocol string
fn byte_to_protocol(b: u8) -> &'static str {
    match b {
        BINARY_PROTOCOL_TCP => "tcp",
        BINARY_PROTOCOL_UDP => "udp",
        _ => "tcp",
    }
}

/// Helper function to convert operation string to byte
fn operation_to_byte(operation: &str) -> u8 {
    match operation {
        "add" => BINARY_CONNECTOR_OPERATION_ADD,
        "remove" => BINARY_CONNECTOR_OPERATION_REMOVE,
        _ => BINARY_CONNECTOR_OPERATION_ADD,
    }
}

/// Helper function to convert byte to operation string
fn byte_to_operation(b: u8) -> &'static str {
    match b {
        BINARY_CONNECTOR_OPERATION_ADD => "add",
        BINARY_CONNECTOR_OPERATION_REMOVE => "remove",
        _ => "add",
    }
}

/// Authentication message sent by client to server
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthMessage {
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

    fn pack(&self) -> Result<Vec<u8>, String> {
        // Version(1) + Type(1) + TokenLen(1) + Token(N) + Reverse(1) + Instance(16)
        let mut buf = Vec::new();
        buf.push(PROTOCOL_VERSION);
        buf.push(BINARY_TYPE_AUTH);

        if self.token.len() > 255 {
            return Err("Token too long (max 255 bytes)".to_string());
        }
        buf.push(self.token.len() as u8);
        buf.extend_from_slice(self.token.as_bytes());
        buf.push(bool_to_byte(self.reverse));
        buf.extend_from_slice(&uuid_to_bytes(&self.instance));

        Ok(buf)
    }
}

impl AuthMessage {
    /// Create a new AuthMessage
    pub fn new(token: String, reverse: bool) -> Self {
        AuthMessage {
            token,
            reverse,
            instance: Uuid::new_v4(),
        }
    }
}

/// Authentication response message sent by server to client
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuthResponseMessage {
    /// Whether authentication was successful
    pub success: bool,

    /// Error message if authentication failed
    pub error: Option<String>,
}

impl Message for AuthResponseMessage {
    fn message_type(&self) -> &'static str {
        "auth_response"
    }

    fn pack(&self) -> Result<Vec<u8>, String> {
        // Version(1) + Type(1) + Success(1) + [ErrorLen(1) + Error(N) if !Success]
        let mut buf = Vec::new();
        buf.push(PROTOCOL_VERSION);
        buf.push(BINARY_TYPE_AUTH_RESPONSE);
        buf.push(bool_to_byte(self.success));

        if !self.success {
            if let Some(error) = &self.error {
                if error.len() > 255 {
                    return Err("Error message too long (max 255 bytes)".to_string());
                }
                buf.push(error.len() as u8);
                buf.extend_from_slice(error.as_bytes());
            }
        }

        Ok(buf)
    }
}

impl AuthResponseMessage {
    /// Create a new successful AuthResponseMessage
    pub fn success() -> Self {
        AuthResponseMessage {
            success: true,
            error: None,
        }
    }

    /// Create a new failed AuthResponseMessage
    pub fn failure(error: String) -> Self {
        AuthResponseMessage {
            success: false,
            error: Some(error),
        }
    }
}

/// Connect message sent to establish a new connection
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectMessage {
    /// Protocol (tcp or udp)
    pub protocol: String,

    /// Unique channel ID for this connection
    pub channel_id: Uuid,

    /// Target address to connect to
    pub address: String,

    /// Target port
    pub port: u16,
}

impl Message for ConnectMessage {
    fn message_type(&self) -> &'static str {
        "connect"
    }

    fn pack(&self) -> Result<Vec<u8>, String> {
        // Version(1) + Type(1) + Protocol(1) + ChannelID(16) + [AddrLen(1) + Addr(N) + Port(2) if TCP]
        let mut buf = Vec::new();
        buf.push(PROTOCOL_VERSION);
        buf.push(BINARY_TYPE_CONNECT);
        buf.push(protocol_to_byte(&self.protocol));
        buf.extend_from_slice(&uuid_to_bytes(&self.channel_id));

        if self.protocol == "tcp" {
            if self.address.len() > 255 {
                return Err("Address too long (max 255 bytes)".to_string());
            }
            buf.push(self.address.len() as u8);
            buf.extend_from_slice(self.address.as_bytes());
            buf.push((self.port >> 8) as u8);
            buf.push(self.port as u8);
        }

        Ok(buf)
    }
}

impl ConnectMessage {
    /// Create a new ConnectMessage from address string (host:port)
    pub fn new(address: String) -> Self {
        // Parse address into host and port
        let (host, port) = if let Some(pos) = address.rfind(':') {
            let host = address[..pos].to_string();
            let port = address[pos + 1..].parse::<u16>().unwrap_or(80);
            (host, port)
        } else {
            (address, 80)
        };

        ConnectMessage {
            protocol: "tcp".to_string(),
            channel_id: Uuid::new_v4(),
            address: host,
            port,
        }
    }

    /// Create a new ConnectMessage with a specific channel ID
    pub fn with_channel_id(address: String, channel_id: Uuid) -> Self {
        let (host, port) = if let Some(pos) = address.rfind(':') {
            let host = address[..pos].to_string();
            let port = address[pos + 1..].parse::<u16>().unwrap_or(80);
            (host, port)
        } else {
            (address, 80)
        };

        ConnectMessage {
            protocol: "tcp".to_string(),
            channel_id,
            address: host,
            port,
        }
    }

    /// Get the full address as string (host:port)
    pub fn get_address(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}

/// Connect response message sent by server to client
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectResponseMessage {
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

    fn pack(&self) -> Result<Vec<u8>, String> {
        // Version(1) + Type(1) + Success(1) + ChannelID(16) + [ErrorLen(1) + Error(N) if !Success]
        let mut buf = Vec::new();
        buf.push(PROTOCOL_VERSION);
        buf.push(BINARY_TYPE_CONNECT_RESPONSE);
        buf.push(bool_to_byte(self.success));
        buf.extend_from_slice(&uuid_to_bytes(&self.channel_id));

        if !self.success {
            if let Some(error) = &self.error {
                if error.len() > 255 {
                    return Err("Error message too long (max 255 bytes)".to_string());
                }
                buf.push(error.len() as u8);
                buf.extend_from_slice(error.as_bytes());
            }
        }

        Ok(buf)
    }
}

impl ConnectResponseMessage {
    /// Create a new successful ConnectResponseMessage
    pub fn success(channel_id: Uuid) -> Self {
        ConnectResponseMessage {
            channel_id,
            success: true,
            error: None,
        }
    }

    /// Create a new failed ConnectResponseMessage
    pub fn failure(channel_id: Uuid, error: String) -> Self {
        ConnectResponseMessage {
            channel_id,
            success: false,
            error: Some(error),
        }
    }
}

/// Data message sent between client and server
#[derive(Debug, Clone)]
pub struct DataMessage {
    /// Protocol (tcp or udp)
    pub protocol: String,

    /// Channel ID this data is for
    pub channel_id: Uuid,

    /// Binary data payload
    pub data: Vec<u8>,

    /// Compression type
    pub compression: u8,
}

impl Message for DataMessage {
    fn message_type(&self) -> &'static str {
        "data"
    }

    fn pack(&self) -> Result<Vec<u8>, String> {
        // Version(1) + Type(1) + Protocol(1) + ChannelID(16) + Compression(1) + DataLen(4) + Data(N)
        let mut buf = Vec::new();
        buf.push(PROTOCOL_VERSION);
        buf.push(BINARY_TYPE_DATA);
        buf.push(protocol_to_byte(&self.protocol));
        buf.extend_from_slice(&uuid_to_bytes(&self.channel_id));
        buf.push(self.compression);

        let data_len = self.data.len() as u32;
        buf.push((data_len >> 24) as u8);
        buf.push((data_len >> 16) as u8);
        buf.push((data_len >> 8) as u8);
        buf.push(data_len as u8);
        buf.extend_from_slice(&self.data);

        Ok(buf)
    }
}

impl DataMessage {
    /// Create a new DataMessage
    pub fn new(channel_id: Uuid, data: Vec<u8>) -> Self {
        DataMessage {
            protocol: "tcp".to_string(),
            channel_id,
            data,
            compression: DATA_COMPRESSION_NONE,
        }
    }

    /// Get the decoded data
    pub fn get_data(&self) -> Result<Vec<u8>, String> {
        Ok(self.data.clone())
    }
}

/// Disconnect message sent between client and server
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DisconnectMessage {
    /// Channel ID to disconnect
    pub channel_id: Uuid,

    /// Optional error message
    pub error: Option<String>,
}

impl Message for DisconnectMessage {
    fn message_type(&self) -> &'static str {
        "disconnect"
    }

    fn pack(&self) -> Result<Vec<u8>, String> {
        // Version(1) + Type(1) + ChannelID(16) + [ErrorLen(1) + Error(N) if error]
        let mut buf = Vec::new();
        buf.push(PROTOCOL_VERSION);
        buf.push(BINARY_TYPE_DISCONNECT);
        buf.extend_from_slice(&uuid_to_bytes(&self.channel_id));

        if let Some(error) = &self.error {
            if error.len() > 255 {
                return Err("Error message too long (max 255 bytes)".to_string());
            }
            buf.push(error.len() as u8);
            buf.extend_from_slice(error.as_bytes());
        }

        Ok(buf)
    }
}

impl DisconnectMessage {
    /// Create a new DisconnectMessage
    pub fn new(channel_id: Uuid) -> Self {
        DisconnectMessage {
            channel_id,
            error: None,
        }
    }
}

/// Partners message sent from server to client
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PartnersMessage {
    /// Number of available partners
    pub count: usize,
}

impl Message for PartnersMessage {
    fn message_type(&self) -> &'static str {
        "partners"
    }

    fn pack(&self) -> Result<Vec<u8>, String> {
        // Version(1) + Type(1) + DataLen(4) + Data(JSON)
        let mut buf = Vec::new();
        buf.push(PROTOCOL_VERSION);
        buf.push(BINARY_TYPE_PARTNERS);

        let json_data = serde_json::json!({"count": self.count}).to_string();
        let data_len = json_data.len() as u32;
        buf.push((data_len >> 24) as u8);
        buf.push((data_len >> 16) as u8);
        buf.push((data_len >> 8) as u8);
        buf.push(data_len as u8);
        buf.extend_from_slice(json_data.as_bytes());

        Ok(buf)
    }
}

impl PartnersMessage {
    /// Create a new PartnersMessage
    pub fn new(count: usize) -> Self {
        PartnersMessage { count }
    }
}

/// Connector message sent from client to server
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectorMessage {
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

    fn pack(&self) -> Result<Vec<u8>, String> {
        // Version(1) + Type(1) + ChannelID(16) + TokenLen(1) + Token(N) + Operation(1)
        let mut buf = Vec::new();
        buf.push(PROTOCOL_VERSION);
        buf.push(BINARY_TYPE_CONNECTOR);
        buf.extend_from_slice(&uuid_to_bytes(&self.channel_id));

        if self.connector_token.len() > 255 {
            return Err("Connector token too long (max 255 bytes)".to_string());
        }
        buf.push(self.connector_token.len() as u8);
        buf.extend_from_slice(self.connector_token.as_bytes());
        buf.push(operation_to_byte(&self.operation));

        Ok(buf)
    }
}

impl ConnectorMessage {
    /// Create a new ConnectorMessage for adding a connector
    pub fn add(connector_token: String) -> Self {
        ConnectorMessage {
            channel_id: Uuid::new_v4(),
            operation: "add".to_string(),
            connector_token,
        }
    }

    /// Create a new ConnectorMessage for removing a connector
    pub fn remove(connector_token: String) -> Self {
        ConnectorMessage {
            channel_id: Uuid::new_v4(),
            operation: "remove".to_string(),
            connector_token,
        }
    }
}

/// Connector response message sent from server to client
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectorResponseMessage {
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

    fn pack(&self) -> Result<Vec<u8>, String> {
        // Version(1) + Type(1) + ChannelID(16) + Success(1) + [ErrorLen(1) + Error(N) if !Success] + [TokenLen(1) + Token(N) if Success && HasToken]
        let mut buf = Vec::new();
        buf.push(PROTOCOL_VERSION);
        buf.push(BINARY_TYPE_CONNECTOR_RESPONSE);
        buf.extend_from_slice(&uuid_to_bytes(&self.channel_id));
        buf.push(bool_to_byte(self.success));

        if !self.success {
            if let Some(error) = &self.error {
                if error.len() > 255 {
                    return Err("Error message too long (max 255 bytes)".to_string());
                }
                buf.push(error.len() as u8);
                buf.extend_from_slice(error.as_bytes());
            }
        } else if let Some(token) = &self.connector_token {
            if token.len() > 255 {
                return Err("Connector token too long (max 255 bytes)".to_string());
            }
            buf.push(token.len() as u8);
            buf.extend_from_slice(token.as_bytes());
        }

        Ok(buf)
    }
}

impl ConnectorResponseMessage {
    /// Create a new successful ConnectorResponseMessage for add operation
    pub fn add_success(channel_id: Uuid, connector_token: String) -> Self {
        ConnectorResponseMessage {
            channel_id,
            success: true,
            error: None,
            connector_token: Some(connector_token),
        }
    }

    /// Create a new successful ConnectorResponseMessage for remove operation
    pub fn remove_success(channel_id: Uuid) -> Self {
        ConnectorResponseMessage {
            channel_id,
            success: true,
            error: None,
            connector_token: None,
        }
    }

    /// Create a new failed ConnectorResponseMessage
    pub fn failure(channel_id: Uuid, error: String) -> Self {
        ConnectorResponseMessage {
            channel_id,
            success: false,
            error: Some(error),
            connector_token: None,
        }
    }
}

/// Parse a binary message
pub fn parse_message(data: &[u8]) -> Result<Box<dyn Message>, String> {
    if data.len() < 2 {
        return Err("Message too short".to_string());
    }

    let version = data[0];
    if version != PROTOCOL_VERSION {
        return Err(format!("Unsupported protocol version: {:#x}", version));
    }

    let msg_type = data[1];
    let payload = &data[2..];

    match msg_type {
        BINARY_TYPE_AUTH => parse_auth_message(payload),
        BINARY_TYPE_AUTH_RESPONSE => parse_auth_response_message(payload),
        BINARY_TYPE_CONNECT => parse_connect_message(payload),
        BINARY_TYPE_CONNECT_RESPONSE => parse_connect_response_message(payload),
        BINARY_TYPE_DATA => parse_data_message(payload),
        BINARY_TYPE_DISCONNECT => parse_disconnect_message(payload),
        BINARY_TYPE_CONNECTOR => parse_connector_message(payload),
        BINARY_TYPE_CONNECTOR_RESPONSE => parse_connector_response_message(payload),
        BINARY_TYPE_PARTNERS => parse_partners_message(payload),
        _ => Err(format!("Unknown message type: {:#x}", msg_type)),
    }
}

fn parse_auth_message(payload: &[u8]) -> Result<Box<dyn Message>, String> {
    if payload.len() < 1 {
        return Err("Invalid auth message".to_string());
    }

    let token_len = payload[0] as usize;
    if payload.len() < 1 + token_len + 1 + 16 {
        return Err("Invalid auth message length".to_string());
    }

    let token = String::from_utf8(payload[1..1 + token_len].to_vec())
        .map_err(|e| format!("Invalid UTF-8 in token: {}", e))?;
    let reverse = byte_to_bool(payload[1 + token_len]);
    let instance = bytes_to_uuid(&payload[1 + token_len + 1..1 + token_len + 1 + 16])?;

    Ok(Box::new(AuthMessage {
        token,
        reverse,
        instance,
    }))
}

fn parse_auth_response_message(payload: &[u8]) -> Result<Box<dyn Message>, String> {
    if payload.len() < 1 {
        return Err("Invalid auth response message".to_string());
    }

    let success = byte_to_bool(payload[0]);
    let mut error = None;

    if !success && payload.len() > 1 {
        let error_len = payload[1] as usize;
        if payload.len() >= 2 + error_len {
            error = Some(
                String::from_utf8(payload[2..2 + error_len].to_vec())
                    .map_err(|e| format!("Invalid UTF-8 in error: {}", e))?,
            );
        }
    }

    Ok(Box::new(AuthResponseMessage { success, error }))
}

pub fn parse_connect_frame(frame: &[u8]) -> Result<ConnectMessage, String> {
    if frame.len() < 2 { return Err("Message too short".to_string()); }
    if frame[0] != PROTOCOL_VERSION { return Err("Unsupported protocol version".to_string()); }
    if frame[1] != BINARY_TYPE_CONNECT { return Err("Not a connect frame".to_string()); }
    if let Ok(boxed) = parse_connect_message(&frame[2..]) { if let Ok(c) = downcast_connect(boxed) { return Ok(c); } }
    // Fallback direct parse
    let payload = &frame[2..];
    if payload.len() < 17 { return Err("Invalid connect message".to_string()); }
    let protocol = byte_to_protocol(payload[0]).to_string();
    let channel_id = bytes_to_uuid(&payload[1..17])?;
    let (address, port) = if protocol == "tcp" { let payload = &payload[17..]; if payload.len()<1 { return Err("Invalid tcp connect message".to_string()); } let addr_len = payload[0] as usize; if payload.len() < 1+addr_len+2 { return Err("Invalid tcp connect message length".to_string()); } let address = String::from_utf8(payload[1..1+addr_len].to_vec()).map_err(|e| format!("Invalid UTF-8 in address: {}", e))?; let port = (payload[1+addr_len] as u16) << 8 | payload[1+addr_len+1] as u16; (address, port)} else { (String::new(), 0)};
    Ok(ConnectMessage { protocol, channel_id, address, port })
}

pub fn parse_data_frame(frame: &[u8]) -> Result<DataMessage, String> {
    if frame.len() < 2 { return Err("Message too short".to_string()); }
    if frame[0] != PROTOCOL_VERSION { return Err("Unsupported protocol version".to_string()); }
    if frame[1] != BINARY_TYPE_DATA { return Err("Not a data frame".to_string()); }
    if let Ok(boxed) = parse_data_message(&frame[2..]) { if let Ok(d) = downcast_data(boxed) { return Ok(d); } }
    // Fallback direct parse
    let payload = &frame[2..];
    if payload.len() < 22 { return Err("Invalid data message".to_string()); }
    let protocol = byte_to_protocol(payload[0]).to_string();
    let channel_id = bytes_to_uuid(&payload[1..17])?;
    let compression = payload[17];
    let data_len = ((payload[18] as u32) << 24) | ((payload[19] as u32) << 16) | ((payload[20] as u32) << 8) | (payload[21] as u32);
    if payload.len() < 22 + data_len as usize { return Err("Invalid data message length".to_string()); }
    let data = payload[22..22+data_len as usize].to_vec();
    Ok(DataMessage { protocol, channel_id, data, compression })
}

pub fn parse_disconnect_frame(frame: &[u8]) -> Result<Uuid, String> {
    if frame.len() < 2 { return Err("Message too short".to_string()); }
    if frame[0] != PROTOCOL_VERSION { return Err("Unsupported protocol version".to_string()); }
    if frame[1] != BINARY_TYPE_DISCONNECT { return Err("Not a disconnect frame".to_string()); }
    let payload = &frame[2..];
    if payload.len() < 16 { return Err("Invalid disconnect message".to_string()); }
    let channel_id = bytes_to_uuid(&payload[0..16])?;
    Ok(channel_id)
}

fn parse_connect_message(payload: &[u8]) -> Result<Box<dyn Message>, String> {
    if payload.len() < 17 {
        return Err("Invalid connect message".to_string());
    }

    let protocol = byte_to_protocol(payload[0]).to_string();
    let channel_id = bytes_to_uuid(&payload[1..17])?;

    let (address, port) = if protocol == "tcp" {
        let payload = &payload[17..];
        if payload.len() < 1 {
            return Err("Invalid TCP connect message".to_string());
        }

        let addr_len = payload[0] as usize;
        if payload.len() < 1 + addr_len + 2 {
            return Err("Invalid TCP connect message length".to_string());
        }

        let address = String::from_utf8(payload[1..1 + addr_len].to_vec())
            .map_err(|e| format!("Invalid UTF-8 in address: {}", e))?;
        let port = (payload[1 + addr_len] as u16) << 8 | payload[1 + addr_len + 1] as u16;
        (address, port)
    } else {
        (String::new(), 0)
    };

    Ok(Box::new(ConnectMessage {
        protocol,
        channel_id,
        address,
        port,
    }))
}

fn parse_connect_response_message(payload: &[u8]) -> Result<Box<dyn Message>, String> {
    if payload.len() < 17 {
        return Err("Invalid connect response message".to_string());
    }

    let success = byte_to_bool(payload[0]);
    let channel_id = bytes_to_uuid(&payload[1..17])?;
    let mut error = None;

    if !success && payload.len() > 17 {
        let error_len = payload[17] as usize;
        if payload.len() >= 18 + error_len {
            error = Some(
                String::from_utf8(payload[18..18 + error_len].to_vec())
                    .map_err(|e| format!("Invalid UTF-8 in error: {}", e))?,
            );
        }
    }

    Ok(Box::new(ConnectResponseMessage {
        channel_id,
        success,
        error,
    }))
}

fn downcast_connect(_m: Box<dyn Message>) -> Result<ConnectMessage, String> { Err("downcast not supported".to_string()) }
fn downcast_data(_m: Box<dyn Message>) -> Result<DataMessage, String> { Err("downcast not supported".to_string()) }

fn parse_data_message(payload: &[u8]) -> Result<Box<dyn Message>, String> {
    if payload.len() < 22 {
        return Err("Invalid data message".to_string());
    }

    let protocol = byte_to_protocol(payload[0]).to_string();
    let channel_id = bytes_to_uuid(&payload[1..17])?;
    let compression = payload[17];
    let data_len = ((payload[18] as u32) << 24)
        | ((payload[19] as u32) << 16)
        | ((payload[20] as u32) << 8)
        | (payload[21] as u32);

    if payload.len() < 22 + data_len as usize {
        return Err("Invalid data message length".to_string());
    }

    let data = payload[22..22 + data_len as usize].to_vec();

    Ok(Box::new(DataMessage {
        protocol,
        channel_id,
        data,
        compression,
    }))
}

fn parse_disconnect_message(payload: &[u8]) -> Result<Box<dyn Message>, String> {
    if payload.len() < 16 {
        return Err("Invalid disconnect message".to_string());
    }

    let channel_id = bytes_to_uuid(&payload[0..16])?;
    let mut error = None;

    if payload.len() > 16 {
        let error_len = payload[16] as usize;
        if payload.len() >= 17 + error_len && error_len > 0 {
            error = Some(
                String::from_utf8(payload[17..17 + error_len].to_vec())
                    .map_err(|e| format!("Invalid UTF-8 in error: {}", e))?,
            );
        }
    }

    Ok(Box::new(DisconnectMessage { channel_id, error }))
}

fn parse_connector_message(payload: &[u8]) -> Result<Box<dyn Message>, String> {
    if payload.len() < 16 {
        return Err("Invalid connector message".to_string());
    }

    let channel_id = bytes_to_uuid(&payload[0..16])?;
    let payload = &payload[16..];

    if payload.len() < 1 {
        return Err("Invalid connector message length".to_string());
    }

    let token_len = payload[0] as usize;
    if payload.len() < 1 + token_len + 1 {
        return Err("Invalid connector message length".to_string());
    }

    let connector_token = String::from_utf8(payload[1..1 + token_len].to_vec())
        .map_err(|e| format!("Invalid UTF-8 in connector token: {}", e))?;
    let operation = byte_to_operation(payload[1 + token_len]).to_string();

    Ok(Box::new(ConnectorMessage {
        channel_id,
        operation,
        connector_token,
    }))
}

fn parse_connector_response_message(payload: &[u8]) -> Result<Box<dyn Message>, String> {
    if payload.len() < 17 {
        return Err("Invalid connector response message".to_string());
    }

    let channel_id = bytes_to_uuid(&payload[0..16])?;
    let success = byte_to_bool(payload[16]);
    let mut error = None;
    let mut connector_token = None;

    if !success && payload.len() > 17 {
        let error_len = payload[17] as usize;
        if payload.len() >= 18 + error_len {
            error = Some(
                String::from_utf8(payload[18..18 + error_len].to_vec())
                    .map_err(|e| format!("Invalid UTF-8 in error: {}", e))?,
            );
        }
    } else if success && payload.len() > 17 {
        let token_len = payload[17] as usize;
        if payload.len() >= 18 + token_len {
            connector_token = Some(
                String::from_utf8(payload[18..18 + token_len].to_vec())
                    .map_err(|e| format!("Invalid UTF-8 in connector token: {}", e))?,
            );
        }
    }

    Ok(Box::new(ConnectorResponseMessage {
        channel_id,
        success,
        error,
        connector_token,
    }))
}

fn parse_partners_message(payload: &[u8]) -> Result<Box<dyn Message>, String> {
    if payload.len() < 4 {
        return Err("Invalid partners message".to_string());
    }

    let data_len = ((payload[0] as u32) << 24)
        | ((payload[1] as u32) << 16)
        | ((payload[2] as u32) << 8)
        | (payload[3] as u32);

    if payload.len() < 4 + data_len as usize {
        return Err("Invalid partners message length".to_string());
    }

    let json_data = &payload[4..4 + data_len as usize];
    let json_str = String::from_utf8(json_data.to_vec())
        .map_err(|e| format!("Invalid UTF-8 in JSON: {}", e))?;

    #[derive(Deserialize)]
    struct PartnersData {
        count: usize,
    }

    let data: PartnersData =
        serde_json::from_str(&json_str).map_err(|e| format!("Invalid JSON: {}", e))?;

    Ok(Box::new(PartnersMessage { count: data.count }))
}
