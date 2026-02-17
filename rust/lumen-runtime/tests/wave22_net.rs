//! Comprehensive tests for the `lumen_runtime::net` module.

use lumen_runtime::net::*;

// ---------------------------------------------------------------------------
// IpAddr Display tests
// ---------------------------------------------------------------------------

#[test]
fn ip_addr_v4_display() {
    let addr = IpAddr::V4(192, 168, 1, 1);
    assert_eq!(addr.to_string(), "192.168.1.1");
}

#[test]
fn ip_addr_v4_display_zeros() {
    let addr = IpAddr::V4(0, 0, 0, 0);
    assert_eq!(addr.to_string(), "0.0.0.0");
}

#[test]
fn ip_addr_v6_display() {
    let addr = IpAddr::V6("::1".to_string());
    assert_eq!(addr.to_string(), "::1");
}

#[test]
fn ip_addr_v6_display_full() {
    let addr = IpAddr::V6("2001:db8::1".to_string());
    assert_eq!(addr.to_string(), "2001:db8::1");
}

// ---------------------------------------------------------------------------
// SocketAddr Display tests
// ---------------------------------------------------------------------------

#[test]
fn socket_addr_display_v4() {
    let sa = SocketAddr {
        ip: IpAddr::V4(10, 0, 0, 1),
        port: 8080,
    };
    assert_eq!(sa.to_string(), "10.0.0.1:8080");
}

#[test]
fn socket_addr_display_v6() {
    let sa = SocketAddr {
        ip: IpAddr::V6("::1".to_string()),
        port: 443,
    };
    assert_eq!(sa.to_string(), "[::1]:443");
}

// ---------------------------------------------------------------------------
// parse_ip tests
// ---------------------------------------------------------------------------

#[test]
fn parse_ip_valid_v4() {
    let ip = parse_ip("1.2.3.4").unwrap();
    assert_eq!(ip, IpAddr::V4(1, 2, 3, 4));
}

#[test]
fn parse_ip_valid_v4_localhost() {
    let ip = parse_ip("127.0.0.1").unwrap();
    assert_eq!(ip, IpAddr::V4(127, 0, 0, 1));
}

#[test]
fn parse_ip_valid_v6_loopback() {
    let ip = parse_ip("::1").unwrap();
    assert_eq!(ip, IpAddr::V6("::1".to_string()));
}

#[test]
fn parse_ip_valid_v6_full() {
    let ip = parse_ip("2001:db8::1").unwrap();
    // std::net normalises the string representation
    if let IpAddr::V6(s) = &ip {
        assert!(s.contains("2001"));
    } else {
        panic!("expected V6 variant");
    }
}

#[test]
fn parse_ip_invalid_garbage() {
    let err = parse_ip("not-an-ip").unwrap_err();
    match err {
        NetError::InvalidAddress(msg) => assert!(msg.contains("not-an-ip")),
        other => panic!("unexpected error: {:?}", other),
    }
}

#[test]
fn parse_ip_invalid_empty() {
    assert!(parse_ip("").is_err());
}

// ---------------------------------------------------------------------------
// parse_socket_addr tests
// ---------------------------------------------------------------------------

#[test]
fn parse_socket_addr_valid_v4() {
    let sa = parse_socket_addr("1.2.3.4:8080").unwrap();
    assert_eq!(sa.ip, IpAddr::V4(1, 2, 3, 4));
    assert_eq!(sa.port, 8080);
}

#[test]
fn parse_socket_addr_valid_v6() {
    let sa = parse_socket_addr("[::1]:9090").unwrap();
    assert_eq!(sa.ip, IpAddr::V6("::1".to_string()));
    assert_eq!(sa.port, 9090);
}

#[test]
fn parse_socket_addr_invalid_no_port() {
    assert!(parse_socket_addr("1.2.3.4").is_err());
}

#[test]
fn parse_socket_addr_invalid_garbage() {
    let err = parse_socket_addr("garbage").unwrap_err();
    match err {
        NetError::InvalidAddress(msg) => assert!(msg.contains("garbage")),
        other => panic!("unexpected error: {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// TcpConfig tests
// ---------------------------------------------------------------------------

#[test]
fn tcp_config_default_server() {
    let cfg = TcpConfig::default_server(3000);
    assert_eq!(cfg.bind_addr.ip, IpAddr::V4(0, 0, 0, 0));
    assert_eq!(cfg.bind_addr.port, 3000);
    assert_eq!(cfg.backlog, 128);
    assert!(cfg.reuse_addr);
    assert!(cfg.nodelay);
    assert_eq!(cfg.keepalive, Some(60_000));
}

#[test]
fn tcp_config_default_client() {
    let cfg = TcpConfig::default_client();
    assert_eq!(cfg.bind_addr.ip, IpAddr::V4(0, 0, 0, 0));
    assert_eq!(cfg.bind_addr.port, 0);
    assert_eq!(cfg.backlog, 0);
    assert!(!cfg.reuse_addr);
    assert!(cfg.nodelay);
    assert_eq!(cfg.keepalive, None);
}

// ---------------------------------------------------------------------------
// ConnectionState tests
// ---------------------------------------------------------------------------

#[test]
fn connection_state_variants_exist() {
    let states = vec![
        ConnectionState::Connecting,
        ConnectionState::Connected,
        ConnectionState::Closing,
        ConnectionState::Closed,
        ConnectionState::Error("boom".to_string()),
    ];
    assert_eq!(states.len(), 5);
}

#[test]
fn connection_state_equality() {
    assert_eq!(ConnectionState::Connected, ConnectionState::Connected);
    assert_ne!(ConnectionState::Connecting, ConnectionState::Connected);
    assert_eq!(
        ConnectionState::Error("x".to_string()),
        ConnectionState::Error("x".to_string())
    );
}

// ---------------------------------------------------------------------------
// ConnectionInfo tests
// ---------------------------------------------------------------------------

#[test]
fn connection_info_construction() {
    let info = ConnectionInfo {
        local_addr: SocketAddr {
            ip: IpAddr::V4(127, 0, 0, 1),
            port: 12345,
        },
        remote_addr: SocketAddr {
            ip: IpAddr::V4(93, 184, 216, 34),
            port: 443,
        },
        state: ConnectionState::Connected,
        bytes_sent: 1024,
        bytes_received: 4096,
    };
    assert_eq!(info.state, ConnectionState::Connected);
    assert_eq!(info.bytes_sent, 1024);
    assert_eq!(info.bytes_received, 4096);
    assert_eq!(info.local_addr.port, 12345);
    assert_eq!(info.remote_addr.port, 443);
}

// ---------------------------------------------------------------------------
// UdpConfig tests
// ---------------------------------------------------------------------------

#[test]
fn udp_config_default_on() {
    let cfg = UdpConfig::default_on(5353);
    assert_eq!(cfg.bind_addr.ip, IpAddr::V4(0, 0, 0, 0));
    assert_eq!(cfg.bind_addr.port, 5353);
    assert!(!cfg.broadcast);
    assert_eq!(cfg.multicast, None);
}

#[test]
fn udp_config_custom() {
    let cfg = UdpConfig {
        bind_addr: SocketAddr {
            ip: IpAddr::V4(0, 0, 0, 0),
            port: 9000,
        },
        broadcast: true,
        multicast: Some("239.0.0.1".to_string()),
    };
    assert!(cfg.broadcast);
    assert_eq!(cfg.multicast, Some("239.0.0.1".to_string()));
}

// ---------------------------------------------------------------------------
// Datagram tests
// ---------------------------------------------------------------------------

#[test]
fn datagram_construction() {
    let dg = Datagram {
        data: vec![0x48, 0x65, 0x6c, 0x6c, 0x6f],
        source: SocketAddr {
            ip: IpAddr::V4(10, 0, 0, 1),
            port: 4000,
        },
        destination: SocketAddr {
            ip: IpAddr::V4(10, 0, 0, 2),
            port: 5000,
        },
    };
    assert_eq!(dg.data, b"Hello");
    assert_eq!(dg.source.port, 4000);
    assert_eq!(dg.destination.port, 5000);
}

// ---------------------------------------------------------------------------
// DNS tests
// ---------------------------------------------------------------------------

#[test]
fn dns_record_type_all_variants() {
    let types = vec![
        DnsRecordType::A,
        DnsRecordType::AAAA,
        DnsRecordType::CNAME,
        DnsRecordType::MX,
        DnsRecordType::TXT,
        DnsRecordType::NS,
        DnsRecordType::SRV,
    ];
    assert_eq!(types.len(), 7);
}

#[test]
fn dns_record_type_display() {
    assert_eq!(DnsRecordType::A.to_string(), "A");
    assert_eq!(DnsRecordType::AAAA.to_string(), "AAAA");
    assert_eq!(DnsRecordType::CNAME.to_string(), "CNAME");
    assert_eq!(DnsRecordType::MX.to_string(), "MX");
    assert_eq!(DnsRecordType::SRV.to_string(), "SRV");
}

#[test]
fn dns_record_construction() {
    let rec = DnsRecord {
        name: "example.com".to_string(),
        record_type: DnsRecordType::A,
        value: "93.184.216.34".to_string(),
        ttl: 300,
    };
    assert_eq!(rec.name, "example.com");
    assert_eq!(rec.record_type, DnsRecordType::A);
    assert_eq!(rec.value, "93.184.216.34");
    assert_eq!(rec.ttl, 300);
}

#[test]
fn resolve_host_localhost() {
    let addrs = resolve_host("localhost").unwrap();
    assert!(!addrs.is_empty());
    // localhost should resolve to 127.0.0.1 or ::1
    let has_loopback = addrs
        .iter()
        .any(|a| matches!(a, IpAddr::V4(127, 0, 0, 1)) || matches!(a, IpAddr::V6(s) if s == "::1"));
    assert!(has_loopback, "expected loopback address, got: {:?}", addrs);
}

#[test]
fn resolve_host_invalid() {
    let result = resolve_host("this.host.definitely.does.not.exist.invalid");
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Protocol detection tests
// ---------------------------------------------------------------------------

#[test]
fn detect_protocol_http() {
    assert_eq!(detect_protocol("http://example.com"), Protocol::Http);
}

#[test]
fn detect_protocol_https() {
    assert_eq!(detect_protocol("https://example.com"), Protocol::Https);
}

#[test]
fn detect_protocol_ws() {
    assert_eq!(detect_protocol("ws://localhost:8080"), Protocol::WebSocket);
}

#[test]
fn detect_protocol_wss() {
    assert_eq!(
        detect_protocol("wss://secure.example.com"),
        Protocol::WebSocket
    );
}

#[test]
fn detect_protocol_tcp() {
    assert_eq!(detect_protocol("tcp://10.0.0.1:9000"), Protocol::Tcp);
}

#[test]
fn detect_protocol_udp() {
    assert_eq!(detect_protocol("udp://10.0.0.1:5353"), Protocol::Udp);
}

#[test]
fn detect_protocol_tls() {
    assert_eq!(detect_protocol("tls://secure:443"), Protocol::Tls);
}

#[test]
fn detect_protocol_unknown() {
    let p = detect_protocol("ftp://files.example.com");
    assert_eq!(p, Protocol::Unknown("ftp".to_string()));
}

#[test]
fn detect_protocol_bare_scheme() {
    assert_eq!(detect_protocol("http"), Protocol::Http);
}

// ---------------------------------------------------------------------------
// Protocol Display test
// ---------------------------------------------------------------------------

#[test]
fn protocol_variants_display() {
    assert_eq!(Protocol::Tcp.to_string(), "tcp");
    assert_eq!(Protocol::Udp.to_string(), "udp");
    assert_eq!(Protocol::Http.to_string(), "http");
    assert_eq!(Protocol::Https.to_string(), "https");
    assert_eq!(Protocol::WebSocket.to_string(), "ws");
    assert_eq!(Protocol::Tls.to_string(), "tls");
    assert_eq!(
        Protocol::Unknown("ftp".to_string()).to_string(),
        "unknown(ftp)"
    );
}

// ---------------------------------------------------------------------------
// NetError tests
// ---------------------------------------------------------------------------

#[test]
fn net_error_display_invalid_address() {
    let err = NetError::InvalidAddress("bad addr".to_string());
    assert_eq!(err.to_string(), "invalid address: bad addr");
}

#[test]
fn net_error_display_connection_refused() {
    let err = NetError::ConnectionRefused("10.0.0.1:80".to_string());
    assert_eq!(err.to_string(), "connection refused: 10.0.0.1:80");
}

#[test]
fn net_error_display_timeout() {
    let err = NetError::Timeout {
        addr: "example.com:443".to_string(),
        ms: 5000,
    };
    assert_eq!(
        err.to_string(),
        "connection to example.com:443 timed out after 5000ms"
    );
}

#[test]
fn net_error_display_dns_failed() {
    let err = NetError::DnsResolutionFailed("bad.host".to_string());
    assert_eq!(err.to_string(), "DNS resolution failed: bad.host");
}

#[test]
fn net_error_display_port_in_use() {
    let err = NetError::PortInUse(8080);
    assert_eq!(err.to_string(), "port 8080 is already in use");
}

#[test]
fn net_error_display_io_error() {
    let err = NetError::IoError("broken pipe".to_string());
    assert_eq!(err.to_string(), "I/O error: broken pipe");
}

#[test]
fn net_error_from_io_error() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
    let net_err: NetError = io_err.into();
    match net_err {
        NetError::IoError(msg) => assert!(msg.contains("access denied")),
        other => panic!("expected IoError, got: {:?}", other),
    }
}

#[test]
fn net_error_is_std_error() {
    let err: Box<dyn std::error::Error> =
        Box::new(NetError::ConnectionRefused("refused".to_string()));
    assert!(err.to_string().contains("refused"));
}
