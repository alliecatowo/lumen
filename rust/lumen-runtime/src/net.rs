//! Network abstractions for the Lumen runtime.
//!
//! This module provides typed network primitives — IP addresses, socket
//! addresses, TCP/UDP configuration, DNS records, protocol detection, and
//! structured error types. These are *type abstractions only*; actual socket
//! I/O will be wired through tool providers at a higher layer.

use std::fmt;
use std::net::ToSocketAddrs;

// ---------------------------------------------------------------------------
// IpAddr
// ---------------------------------------------------------------------------

/// An IP address, either IPv4 or IPv6.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IpAddr {
    /// An IPv4 address represented as four octets.
    V4(u8, u8, u8, u8),
    /// An IPv6 address in string form (e.g. `"::1"`).
    V6(String),
}

impl fmt::Display for IpAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IpAddr::V4(a, b, c, d) => write!(f, "{}.{}.{}.{}", a, b, c, d),
            IpAddr::V6(s) => f.write_str(s),
        }
    }
}

// ---------------------------------------------------------------------------
// SocketAddr
// ---------------------------------------------------------------------------

/// A socket address combining an IP address and a port number.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SocketAddr {
    /// The IP address.
    pub ip: IpAddr,
    /// The port number.
    pub port: u16,
}

impl fmt::Display for SocketAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.ip {
            IpAddr::V4(..) => write!(f, "{}:{}", self.ip, self.port),
            IpAddr::V6(s) => write!(f, "[{}]:{}", s, self.port),
        }
    }
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Parse an IP address string into an [`IpAddr`].
///
/// Supports IPv4 dotted-decimal (e.g. `"1.2.3.4"`) and IPv6 (e.g. `"::1"`).
pub fn parse_ip(s: &str) -> Result<IpAddr, NetError> {
    // Try parsing as std::net::IpAddr which handles both v4 and v6.
    match s.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => {
            let octets = v4.octets();
            Ok(IpAddr::V4(octets[0], octets[1], octets[2], octets[3]))
        }
        Ok(std::net::IpAddr::V6(v6)) => Ok(IpAddr::V6(v6.to_string())),
        Err(_) => Err(NetError::InvalidAddress(format!(
            "invalid IP address: {}",
            s
        ))),
    }
}

/// Parse a socket address string into a [`SocketAddr`].
///
/// Expects the form `"ip:port"` for IPv4 (e.g. `"1.2.3.4:8080"`) or
/// `"[ip]:port"` for IPv6 (e.g. `"[::1]:8080"`).
pub fn parse_socket_addr(s: &str) -> Result<SocketAddr, NetError> {
    match s.parse::<std::net::SocketAddr>() {
        Ok(std::net::SocketAddr::V4(v4)) => {
            let octets = v4.ip().octets();
            Ok(SocketAddr {
                ip: IpAddr::V4(octets[0], octets[1], octets[2], octets[3]),
                port: v4.port(),
            })
        }
        Ok(std::net::SocketAddr::V6(v6)) => Ok(SocketAddr {
            ip: IpAddr::V6(v6.ip().to_string()),
            port: v6.port(),
        }),
        Err(_) => Err(NetError::InvalidAddress(format!(
            "invalid socket address: {}",
            s
        ))),
    }
}

// ---------------------------------------------------------------------------
// TcpConfig
// ---------------------------------------------------------------------------

/// Configuration for a TCP listener or connection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct TcpConfig {
    /// The local address to bind to.
    pub bind_addr: SocketAddr,
    /// Listen backlog size (server only).
    pub backlog: u32,
    /// Whether to set `SO_REUSEADDR`.
    pub reuse_addr: bool,
    /// Whether to set `TCP_NODELAY`.
    pub nodelay: bool,
    /// Optional TCP keepalive interval in milliseconds.
    pub keepalive: Option<u64>,
}

impl TcpConfig {
    /// Return sensible defaults for a TCP server listening on the given port.
    ///
    /// Binds to `0.0.0.0:<port>`, backlog 128, reuse_addr enabled, nodelay
    /// enabled, keepalive at 60 000 ms.
    pub fn default_server(port: u16) -> Self {
        Self {
            bind_addr: SocketAddr {
                ip: IpAddr::V4(0, 0, 0, 0),
                port,
            },
            backlog: 128,
            reuse_addr: true,
            nodelay: true,
            keepalive: Some(60_000),
        }
    }

    /// Return sensible defaults for a TCP client.
    ///
    /// Binds to `0.0.0.0:0` (OS-assigned port), backlog 0, reuse_addr false,
    /// nodelay true, no keepalive.
    pub fn default_client() -> Self {
        Self {
            bind_addr: SocketAddr {
                ip: IpAddr::V4(0, 0, 0, 0),
                port: 0,
            },
            backlog: 0,
            reuse_addr: false,
            nodelay: true,
            keepalive: None,
        }
    }
}

// ---------------------------------------------------------------------------
// ConnectionState / ConnectionInfo
// ---------------------------------------------------------------------------

/// The state of a TCP connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connection attempt in progress.
    Connecting,
    /// Connection is established and active.
    Connected,
    /// Graceful shutdown initiated.
    Closing,
    /// Connection is fully closed.
    Closed,
    /// An error has occurred on the connection.
    Error(String),
}

/// Metadata about an active TCP connection.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct ConnectionInfo {
    /// The local socket address.
    pub local_addr: SocketAddr,
    /// The remote socket address.
    pub remote_addr: SocketAddr,
    /// Current connection state.
    pub state: ConnectionState,
    /// Total bytes sent on this connection.
    pub bytes_sent: u64,
    /// Total bytes received on this connection.
    pub bytes_received: u64,
}

// ---------------------------------------------------------------------------
// UdpConfig
// ---------------------------------------------------------------------------

/// Configuration for a UDP socket.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct UdpConfig {
    /// The local address to bind to.
    pub bind_addr: SocketAddr,
    /// Whether to enable `SO_BROADCAST`.
    pub broadcast: bool,
    /// Optional multicast group address to join.
    pub multicast: Option<String>,
}

impl UdpConfig {
    /// Return sensible defaults for a UDP socket bound to the given port.
    ///
    /// Binds to `0.0.0.0:<port>`, broadcast disabled, no multicast.
    pub fn default_on(port: u16) -> Self {
        Self {
            bind_addr: SocketAddr {
                ip: IpAddr::V4(0, 0, 0, 0),
                port,
            },
            broadcast: false,
            multicast: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Datagram
// ---------------------------------------------------------------------------

/// A UDP datagram with source and destination information.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct Datagram {
    /// The raw payload bytes.
    pub data: Vec<u8>,
    /// The source socket address.
    pub source: SocketAddr,
    /// The destination socket address.
    pub destination: SocketAddr,
}

// ---------------------------------------------------------------------------
// DNS
// ---------------------------------------------------------------------------

/// DNS record types.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DnsRecordType {
    /// IPv4 address record.
    A,
    /// IPv6 address record.
    AAAA,
    /// Canonical name (alias) record.
    CNAME,
    /// Mail exchange record.
    MX,
    /// Text record.
    TXT,
    /// Name server record.
    NS,
    /// Service locator record.
    SRV,
}

impl fmt::Display for DnsRecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DnsRecordType::A => "A",
            DnsRecordType::AAAA => "AAAA",
            DnsRecordType::CNAME => "CNAME",
            DnsRecordType::MX => "MX",
            DnsRecordType::TXT => "TXT",
            DnsRecordType::NS => "NS",
            DnsRecordType::SRV => "SRV",
        };
        f.write_str(s)
    }
}

/// A single DNS record.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub struct DnsRecord {
    /// The domain name this record belongs to.
    pub name: String,
    /// The type of DNS record.
    pub record_type: DnsRecordType,
    /// The record value (e.g. an IP address or hostname).
    pub value: String,
    /// Time-to-live in seconds.
    pub ttl: u32,
}

/// Resolve a hostname to a list of IP addresses using the system resolver.
///
/// Uses [`std::net::ToSocketAddrs`] under the hood. Returns at least one
/// address on success or a [`NetError::DnsResolutionFailed`] on failure.
pub fn resolve_host(hostname: &str) -> Result<Vec<IpAddr>, NetError> {
    // ToSocketAddrs requires a "host:port" pair; use port 0 as a dummy.
    let addr_str = format!("{}:0", hostname);
    match addr_str.to_socket_addrs() {
        Ok(iter) => {
            let addrs: Vec<IpAddr> = iter
                .map(|sa| match sa.ip() {
                    std::net::IpAddr::V4(v4) => {
                        let o = v4.octets();
                        IpAddr::V4(o[0], o[1], o[2], o[3])
                    }
                    std::net::IpAddr::V6(v6) => IpAddr::V6(v6.to_string()),
                })
                .collect();
            if addrs.is_empty() {
                Err(NetError::DnsResolutionFailed(format!(
                    "no addresses found for: {}",
                    hostname
                )))
            } else {
                Ok(addrs)
            }
        }
        Err(e) => Err(NetError::DnsResolutionFailed(format!(
            "failed to resolve {}: {}",
            hostname, e
        ))),
    }
}

// ---------------------------------------------------------------------------
// Protocol detection
// ---------------------------------------------------------------------------

/// Network protocols that can be inferred from a URL or scheme.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Protocol {
    /// Transmission Control Protocol.
    Tcp,
    /// User Datagram Protocol.
    Udp,
    /// Hypertext Transfer Protocol.
    Http,
    /// Hypertext Transfer Protocol Secure.
    Https,
    /// WebSocket protocol.
    WebSocket,
    /// Transport Layer Security.
    Tls,
    /// Unrecognised protocol.
    Unknown(String),
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Protocol::Tcp => "tcp",
            Protocol::Udp => "udp",
            Protocol::Http => "http",
            Protocol::Https => "https",
            Protocol::WebSocket => "ws",
            Protocol::Tls => "tls",
            Protocol::Unknown(s) => return write!(f, "unknown({})", s),
        };
        f.write_str(s)
    }
}

/// Detect the network protocol from a URL or scheme string.
///
/// Inspects the part before `://` (if present) and maps known scheme names
/// to [`Protocol`] variants. Falls back to [`Protocol::Unknown`].
pub fn detect_protocol(url: &str) -> Protocol {
    let scheme = if let Some(idx) = url.find("://") {
        &url[..idx]
    } else {
        url
    };

    match scheme.to_lowercase().as_str() {
        "http" => Protocol::Http,
        "https" => Protocol::Https,
        "ws" | "wss" => Protocol::WebSocket,
        "tcp" => Protocol::Tcp,
        "udp" => Protocol::Udp,
        "tls" | "ssl" => Protocol::Tls,
        other => Protocol::Unknown(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// NetError
// ---------------------------------------------------------------------------

/// Errors that can occur in the network abstraction layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The provided address string could not be parsed.
    InvalidAddress(String),
    /// A connection was refused by the remote host.
    ConnectionRefused(String),
    /// A network operation exceeded its timeout.
    Timeout {
        /// The address that was being connected to.
        addr: String,
        /// The timeout limit in milliseconds.
        ms: u64,
    },
    /// DNS resolution failed for the given hostname.
    DnsResolutionFailed(String),
    /// The requested port is already in use.
    PortInUse(u16),
    /// A wrapped I/O error.
    IoError(String),
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::InvalidAddress(addr) => write!(f, "invalid address: {}", addr),
            NetError::ConnectionRefused(addr) => write!(f, "connection refused: {}", addr),
            NetError::Timeout { addr, ms } => {
                write!(f, "connection to {} timed out after {}ms", addr, ms)
            }
            NetError::DnsResolutionFailed(host) => {
                write!(f, "DNS resolution failed: {}", host)
            }
            NetError::PortInUse(port) => write!(f, "port {} is already in use", port),
            NetError::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for NetError {}

impl From<std::io::Error> for NetError {
    fn from(err: std::io::Error) -> Self {
        NetError::IoError(err.to_string())
    }
}
