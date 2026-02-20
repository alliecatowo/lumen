//! Network provider for Lumen tool dispatch.
//!
//! Implements the `ToolProvider` trait to expose TCP/UDP operations as tools:
//! - `net.tcp_connect` — Open a TCP connection
//! - `net.tcp_listen` — Bind a TCP listener
//! - `net.tcp_send` — Send data on a TCP connection
//! - `net.tcp_recv` — Receive data from a TCP connection
//! - `net.tcp_close` — Close a TCP connection/listener
//! - `net.udp_bind` — Bind a UDP socket
//! - `net.udp_send` — Send a UDP datagram
//! - `net.udp_recv` — Receive a UDP datagram

use lumen_rt::services::tools::{ToolError, ToolProvider, ToolSchema};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetOp {
    TcpConnect,
    TcpListen,
    TcpSend,
    TcpRecv,
    TcpClose,
    UdpBind,
    UdpSend,
    UdpRecv,
}

impl NetOp {
    fn tool_name(&self) -> &'static str {
        match self {
            NetOp::TcpConnect => "net.tcp_connect",
            NetOp::TcpListen => "net.tcp_listen",
            NetOp::TcpSend => "net.tcp_send",
            NetOp::TcpRecv => "net.tcp_recv",
            NetOp::TcpClose => "net.tcp_close",
            NetOp::UdpBind => "net.udp_bind",
            NetOp::UdpSend => "net.udp_send",
            NetOp::UdpRecv => "net.udp_recv",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            NetOp::TcpConnect => "Open a TCP connection to an address",
            NetOp::TcpListen => "Bind a TCP listener to an address",
            NetOp::TcpSend => "Send data on a TCP connection",
            NetOp::TcpRecv => "Receive data from a TCP connection",
            NetOp::TcpClose => "Close a TCP connection or listener",
            NetOp::UdpBind => "Bind a UDP socket to an address",
            NetOp::UdpSend => "Send a UDP datagram to an address",
            NetOp::UdpRecv => "Receive a UDP datagram",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AddressRequest {
    address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HandleRequest {
    handle: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TcpSendRequest {
    handle: String,
    data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TcpRecvRequest {
    handle: String,
    max_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UdpSendRequest {
    handle: String,
    address: String,
    data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UdpRecvRequest {
    handle: String,
    max_bytes: usize,
}

pub struct NetProvider {
    op: NetOp,
    schema: ToolSchema,
    tcp_streams: Mutex<HashMap<String, TcpStream>>,
    tcp_listeners: Mutex<HashMap<String, TcpListener>>,
    udp_sockets: Mutex<HashMap<String, UdpSocket>>,
}

impl NetProvider {
    fn new(op: NetOp) -> Self {
        let (input_schema, output_schema) = match op {
            NetOp::TcpConnect | NetOp::TcpListen | NetOp::UdpBind => (
                json!({
                    "type": "object",
                    "required": ["address"],
                    "properties": {"address": {"type": "string"}}
                }),
                json!({"type": "string"}),
            ),
            NetOp::TcpSend => (
                json!({
                    "type": "object",
                    "required": ["handle", "data"],
                    "properties": {
                        "handle": {"type": "string"},
                        "data": {"type": "string"}
                    }
                }),
                json!({"type": "number"}),
            ),
            NetOp::TcpRecv => (
                json!({
                    "type": "object",
                    "required": ["handle", "max_bytes"],
                    "properties": {
                        "handle": {"type": "string"},
                        "max_bytes": {"type": "number"}
                    }
                }),
                json!({"type": "string"}),
            ),
            NetOp::TcpClose => (
                json!({
                    "type": "object",
                    "required": ["handle"],
                    "properties": {"handle": {"type": "string"}}
                }),
                json!({"type": "boolean"}),
            ),
            NetOp::UdpSend => (
                json!({
                    "type": "object",
                    "required": ["handle", "address", "data"],
                    "properties": {
                        "handle": {"type": "string"},
                        "address": {"type": "string"},
                        "data": {"type": "string"}
                    }
                }),
                json!({"type": "number"}),
            ),
            NetOp::UdpRecv => (
                json!({
                    "type": "object",
                    "required": ["handle", "max_bytes"],
                    "properties": {
                        "handle": {"type": "string"},
                        "max_bytes": {"type": "number"}
                    }
                }),
                json!({
                    "type": "object",
                    "required": ["data", "address"],
                    "properties": {
                        "data": {"type": "string"},
                        "address": {"type": "string"}
                    }
                }),
            ),
        };

        let schema = ToolSchema {
            name: op.tool_name().to_string(),
            description: op.description().to_string(),
            input_schema,
            output_schema,
            effects: vec!["net".to_string()],
        };

        Self {
            op,
            schema,
            tcp_streams: Mutex::new(HashMap::new()),
            tcp_listeners: Mutex::new(HashMap::new()),
            udp_sockets: Mutex::new(HashMap::new()),
        }
    }

    pub fn tcp_connect() -> Self {
        Self::new(NetOp::TcpConnect)
    }

    pub fn tcp_listen() -> Self {
        Self::new(NetOp::TcpListen)
    }

    pub fn tcp_send() -> Self {
        Self::new(NetOp::TcpSend)
    }

    pub fn tcp_recv() -> Self {
        Self::new(NetOp::TcpRecv)
    }

    pub fn tcp_close() -> Self {
        Self::new(NetOp::TcpClose)
    }

    pub fn udp_bind() -> Self {
        Self::new(NetOp::UdpBind)
    }

    pub fn udp_send() -> Self {
        Self::new(NetOp::UdpSend)
    }

    pub fn udp_recv() -> Self {
        Self::new(NetOp::UdpRecv)
    }

    fn next_handle(prefix: &str) -> String {
        let mut rng = rand::thread_rng();
        let suffix: u64 = rng.gen();
        format!("{}_{}", prefix, suffix)
    }

    fn execute(&self, input: Value) -> Result<Value, ToolError> {
        match self.op {
            NetOp::TcpConnect => {
                let req: AddressRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let stream = TcpStream::connect(&req.address).map_err(|e| {
                    ToolError::InvocationFailed(format!("tcp connect failed: {}", e))
                })?;
                let handle = Self::next_handle("tcp");
                self.tcp_streams
                    .lock()
                    .map_err(|_| ToolError::InvocationFailed("lock poisoned".to_string()))?
                    .insert(handle.clone(), stream);
                Ok(json!(handle))
            }
            NetOp::TcpListen => {
                let req: AddressRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let listener = TcpListener::bind(&req.address).map_err(|e| {
                    ToolError::InvocationFailed(format!("tcp listen failed: {}", e))
                })?;
                let handle = Self::next_handle("tcp_listener");
                self.tcp_listeners
                    .lock()
                    .map_err(|_| ToolError::InvocationFailed("lock poisoned".to_string()))?
                    .insert(handle.clone(), listener);
                Ok(json!(handle))
            }
            NetOp::TcpSend => {
                let req: TcpSendRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let mut streams = self
                    .tcp_streams
                    .lock()
                    .map_err(|_| ToolError::InvocationFailed("lock poisoned".to_string()))?;
                let stream = streams
                    .get_mut(&req.handle)
                    .ok_or_else(|| ToolError::InvocationFailed("unknown tcp handle".to_string()))?;
                let bytes = stream
                    .write(req.data.as_bytes())
                    .map_err(|e| ToolError::InvocationFailed(format!("tcp send failed: {}", e)))?;
                Ok(json!(bytes as i64))
            }
            NetOp::TcpRecv => {
                let req: TcpRecvRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let mut streams = self
                    .tcp_streams
                    .lock()
                    .map_err(|_| ToolError::InvocationFailed("lock poisoned".to_string()))?;
                let stream = streams
                    .get_mut(&req.handle)
                    .ok_or_else(|| ToolError::InvocationFailed("unknown tcp handle".to_string()))?;
                let mut buf = vec![0u8; req.max_bytes.max(1)];
                let bytes = stream
                    .read(&mut buf)
                    .map_err(|e| ToolError::InvocationFailed(format!("tcp recv failed: {}", e)))?;
                buf.truncate(bytes);
                Ok(json!(String::from_utf8_lossy(&buf).to_string()))
            }
            NetOp::TcpClose => {
                let req: HandleRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let mut streams = self
                    .tcp_streams
                    .lock()
                    .map_err(|_| ToolError::InvocationFailed("lock poisoned".to_string()))?;
                let mut listeners = self
                    .tcp_listeners
                    .lock()
                    .map_err(|_| ToolError::InvocationFailed("lock poisoned".to_string()))?;
                let removed = streams.remove(&req.handle).is_some()
                    || listeners.remove(&req.handle).is_some();
                Ok(json!(removed))
            }
            NetOp::UdpBind => {
                let req: AddressRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let socket = UdpSocket::bind(&req.address)
                    .map_err(|e| ToolError::InvocationFailed(format!("udp bind failed: {}", e)))?;
                let handle = Self::next_handle("udp");
                self.udp_sockets
                    .lock()
                    .map_err(|_| ToolError::InvocationFailed("lock poisoned".to_string()))?
                    .insert(handle.clone(), socket);
                Ok(json!(handle))
            }
            NetOp::UdpSend => {
                let req: UdpSendRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let sockets = self
                    .udp_sockets
                    .lock()
                    .map_err(|_| ToolError::InvocationFailed("lock poisoned".to_string()))?;
                let socket = sockets
                    .get(&req.handle)
                    .ok_or_else(|| ToolError::InvocationFailed("unknown udp handle".to_string()))?;
                let bytes = socket
                    .send_to(req.data.as_bytes(), &req.address)
                    .map_err(|e| ToolError::InvocationFailed(format!("udp send failed: {}", e)))?;
                Ok(json!(bytes as i64))
            }
            NetOp::UdpRecv => {
                let req: UdpRecvRequest = serde_json::from_value(input)
                    .map_err(|e| ToolError::InvocationFailed(format!("invalid input: {}", e)))?;
                let sockets = self
                    .udp_sockets
                    .lock()
                    .map_err(|_| ToolError::InvocationFailed("lock poisoned".to_string()))?;
                let socket = sockets
                    .get(&req.handle)
                    .ok_or_else(|| ToolError::InvocationFailed("unknown udp handle".to_string()))?;
                let mut buf = vec![0u8; req.max_bytes.max(1)];
                let (bytes, addr) = socket
                    .recv_from(&mut buf)
                    .map_err(|e| ToolError::InvocationFailed(format!("udp recv failed: {}", e)))?;
                buf.truncate(bytes);
                Ok(
                    json!({"data": String::from_utf8_lossy(&buf).to_string(), "address": addr.to_string()}),
                )
            }
        }
    }
}

impl ToolProvider for NetProvider {
    fn name(&self) -> &str {
        self.op.tool_name()
    }

    fn version(&self) -> &str {
        "0.1.0"
    }

    fn schema(&self) -> &ToolSchema {
        &self.schema
    }

    fn call(&self, input: Value) -> Result<Value, ToolError> {
        self.execute(input)
    }
}
