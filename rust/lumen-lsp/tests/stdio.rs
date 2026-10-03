//! Drives the real `lumen-lsp` binary over stdio: non-ASCII documents must not
//! crash the server, and every request must get exactly one reply.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const DOC: &str = "cell greet(name: String) -> String\n  let s = \"héllo 😀 wörld\" + name\n  return s\nend\n\ncell main() -> String\n  let msg = \"é\" + greet(\"ü\")\n  return msg\nend\n";
const URI: &str = "file:///unicode.lm";

struct Client {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    next_id: i64,
}

impl Client {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lumen-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn lumen-lsp");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut len = 0usize;
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 {
                        return;
                    }
                    let header = header.trim();
                    if header.is_empty() {
                        break;
                    }
                    if let Some(v) = header.strip_prefix("Content-Length:") {
                        len = v.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0u8; len];
                if reader.read_exact(&mut body).is_err() {
                    return;
                }
                if tx.send(serde_json::from_slice(&body).unwrap()).is_err() {
                    return;
                }
            }
        });
        let mut c = Client {
            child,
            stdin,
            rx,
            next_id: 1,
        };
        let init = c.request(
            "initialize",
            json!({ "processId": null, "rootUri": null, "capabilities": {} }),
        );
        assert!(init["result"]["capabilities"].is_object(), "{init}");
        c.notify("initialized", json!({}));
        c
    }

    fn send(&mut self, msg: Value) {
        let body = msg.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }

    /// Send a request and return its response, skipping notifications.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let msg = self
                .rx
                .recv_timeout(Duration::from_secs(20))
                .unwrap_or_else(|_| {
                    panic!(
                        "no reply to {method}; server alive: {:?}",
                        self.child.try_wait()
                    )
                });
            if msg.get("id") == Some(&json!(id)) {
                return msg;
            }
        }
    }

    fn at(&mut self, method: &str, line: u32, character: u32) -> Value {
        self.request(
            method,
            json!({ "textDocument": { "uri": URI }, "position": { "line": line, "character": character } }),
        )
    }

    fn assert_alive(&mut self) {
        assert!(self.child.try_wait().unwrap().is_none(), "lumen-lsp exited");
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// UTF-16 column of the first `needle` on `line` of DOC, plus `plus`.
fn col(line: usize, needle: &str, plus: u32) -> u32 {
    let l = DOC.lines().nth(line).unwrap();
    let byte = l.find(needle).unwrap();
    l[..byte].encode_utf16().count() as u32 + plus
}

fn open(c: &mut Client) {
    c.notify(
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": URI, "languageId": "lumen", "version": 1, "text": DOC } }),
    );
    // Wait for the open to be processed (diagnostics are published for every open).
    loop {
        let msg =
            c.rx.recv_timeout(Duration::from_secs(20))
                .expect("diagnostics after didOpen");
        if msg["method"] == "textDocument/publishDiagnostics" {
            break;
        }
    }
}

#[test]
fn non_ascii_lines_do_not_crash_hover_definition_signature_help_or_rename() {
    let mut c = Client::start();
    open(&mut c);

    // `greet` on line 6 sits after an `é`, so its byte offset differs from its UTF-16 column.
    let greet = col(6, "greet", 2);

    let hover = c.at("textDocument/hover", 6, greet);
    assert!(hover["error"].is_null(), "{hover}");
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .unwrap_or("")
            .contains("greet"),
        "{hover}"
    );

    let def = c.at("textDocument/definition", 6, greet);
    assert!(def["error"].is_null(), "{def}");
    assert_eq!(def["result"]["range"]["start"]["line"], 0, "{def}");

    let sig = c.request(
        "textDocument/signatureHelp",
        json!({ "textDocument": { "uri": URI }, "position": { "line": 6, "character": col(6, "\")", 0) } }),
    );
    assert!(sig["error"].is_null(), "{sig}");
    assert!(
        sig["result"]["signatures"][0]["label"]
            .as_str()
            .unwrap_or("")
            .contains("greet"),
        "{sig}"
    );

    let prep = c.at("textDocument/prepareRename", 6, greet);
    assert!(prep["error"].is_null(), "{prep}");
    assert_eq!(
        prep["result"]["start"]["character"],
        col(6, "greet", 0),
        "{prep}"
    );
    assert_eq!(
        prep["result"]["end"]["character"],
        col(6, "greet", 5),
        "{prep}"
    );

    // Every column of every line, including mid-surrogate and past-the-end positions.
    for method in [
        "textDocument/hover",
        "textDocument/definition",
        "textDocument/prepareRename",
        "textDocument/implementation",
    ] {
        for line in 0..12 {
            for character in 0..45 {
                let r = c.at(method, line, character);
                assert!(r["error"].is_null(), "{method} {line}:{character}: {r}");
            }
        }
    }
    for line in 0..9 {
        for character in 0..45 {
            let r = c.request(
                "textDocument/signatureHelp",
                json!({ "textDocument": { "uri": URI }, "position": { "line": line, "character": character } }),
            );
            assert!(
                r["error"].is_null(),
                "signatureHelp {line}:{character}: {r}"
            );
        }
    }

    c.assert_alive();
    // And it still answers afterwards.
    assert!(c.at("textDocument/hover", 6, greet)["result"].is_object());
}

#[test]
fn every_request_gets_a_reply() {
    let mut c = Client::start();
    open(&mut c);

    let unknown = c.request("textDocument/noSuchMethod", json!({}));
    assert_eq!(unknown["error"]["code"], -32601, "{unknown}");

    let bad = c.request("textDocument/hover", json!({ "nonsense": true }));
    assert_eq!(bad["error"]["code"], -32602, "{bad}");

    c.assert_alive();
    assert!(c.at("textDocument/hover", 0, 6)["error"].is_null());
}

#[test]
fn edits_beyond_the_line_end_are_applied_and_close_clears_the_document() {
    let mut c = Client::start();
    open(&mut c);

    // Replace from column 1 to a column far past the end of line 0.
    c.notify(
        "textDocument/didChange",
        json!({
            "textDocument": { "uri": URI, "version": 2 },
            "contentChanges": [{
                "range": { "start": { "line": 0, "character": 5 }, "end": { "line": 0, "character": 500 } },
                "text": "welcome(name: String) -> String"
            }]
        }),
    );
    // Wait for the re-published diagnostics, then hover the renamed cell.
    loop {
        let msg =
            c.rx.recv_timeout(Duration::from_secs(20))
                .expect("diagnostics after didChange");
        if msg["method"] == "textDocument/publishDiagnostics" {
            break;
        }
    }
    let hover = c.at("textDocument/hover", 0, 7);
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .unwrap_or("")
            .contains("welcome"),
        "{hover}"
    );

    // Closing publishes empty diagnostics and forgets the text.
    c.notify(
        "textDocument/didClose",
        json!({ "textDocument": { "uri": URI } }),
    );
    loop {
        let msg =
            c.rx.recv_timeout(Duration::from_secs(20))
                .expect("diagnostics after didClose");
        if msg["method"] == "textDocument/publishDiagnostics" {
            assert_eq!(msg["params"]["diagnostics"], json!([]), "{msg}");
            break;
        }
    }
    let after = c.at("textDocument/hover", 0, 7);
    assert!(after["result"].is_null(), "{after}");
    c.assert_alive();
}
