//! The HTTP builtins must not hang forever on a stalled server, and must
//! report a failed body read instead of returning an empty successful body.

#![cfg(not(target_arch = "wasm32"))]

use lumen_runtime::vm::values::Value;
use lumen_runtime::vm::vm::VM;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

fn get(url: &str) -> Value {
    let src = format!("cell main() -> Any\n  return http_get(\"{url}\")\nend\n");
    let module = lumen_compiler::compile_raw(&src).expect("compiles");
    let mut vm = VM::new();
    vm.load(module);
    vm.execute("main", vec![]).expect("http_get does not raise")
}

fn field(v: &Value, name: &str) -> Value {
    match v {
        Value::Map(m) => m.get(name).cloned().unwrap_or(Value::Null),
        other => panic!("expected a map, got {other:?}"),
    }
}

#[test]
fn stalled_server_times_out_and_truncated_body_is_an_error() {
    // One test so the env var is set before any request is made.
    std::env::set_var("LUMEN_HTTP_TIMEOUT_SECS", "1");

    // 1. Accepts the connection, reads the request, then never answers.
    let stall = TcpListener::bind("127.0.0.1:0").unwrap();
    let stall_port = stall.local_addr().unwrap().port();
    let _stall_thread = std::thread::spawn(move || {
        if let Ok((mut sock, _)) = stall.accept() {
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf);
            std::thread::sleep(Duration::from_secs(10));
        }
    });
    let started = Instant::now();
    let res = get(&format!("http://127.0.0.1:{stall_port}/"));
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "request should time out, took {:?}",
        started.elapsed()
    );
    assert_eq!(field(&res, "ok"), Value::Bool(false), "{res:?}");
    assert!(
        matches!(field(&res, "error"), Value::String(_)),
        "timeout must surface an error: {res:?}"
    );

    // 2. Promises 100 bytes, sends 5, closes: the body read fails.
    let short = TcpListener::bind("127.0.0.1:0").unwrap();
    let short_port = short.local_addr().unwrap().port();
    let _short_thread = std::thread::spawn(move || {
        if let Ok((mut sock, _)) = short.accept() {
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf);
            let _ = sock.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nhello",
            );
        }
    });
    let res = get(&format!("http://127.0.0.1:{short_port}/"));
    assert_eq!(
        field(&res, "ok"),
        Value::Bool(false),
        "a truncated body must not look like success: {res:?}"
    );
    assert!(matches!(field(&res, "error"), Value::String(_)), "{res:?}");
}
