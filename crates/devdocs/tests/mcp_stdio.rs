//! End-to-end `devdocs mcp --transport stdio` tests: the spawned binary
//! serves newline-delimited JSON-RPC on stdout, and stdout carries the
//! protocol stream only (.agents/instructions/observability).

#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::must_use_candidate)]

mod common;

use std::io::{BufRead, BufReader, Read as _, Write as _};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

/// The watchdog timeout: a stuck server fails the test instead of
/// hanging the runner.
const WATCHDOG: Duration = Duration::from_secs(60);

/// One live MCP stdio session against the spawned binary.
struct Session {
    child: Arc<Mutex<Child>>,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr_text: Arc<Mutex<String>>,
}

impl Session {
    /// Spawns the binary with the isolated environment. The stderr pipe
    /// drains on a thread so a log-heavy run never blocks the child.
    fn spawn(env: &common::Env) -> Self {
        let mut cmd = env.std_cmd(&["mcp", "--transport", "stdio"]);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().expect("the mcp child spawns");
        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = BufReader::new(child.stdout.take().expect("stdout piped"));
        let mut stderr: ChildStderr = child.stderr.take().expect("stderr piped");
        let stderr_text = Arc::new(Mutex::new(String::new()));
        let sink = Arc::clone(&stderr_text);
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = stderr.read_to_end(&mut buffer);
            *sink.lock().unwrap() = String::from_utf8_lossy(&buffer).into_owned();
        });
        let child = Arc::new(Mutex::new(child));
        let watched = Arc::clone(&child);
        std::thread::spawn(move || {
            std::thread::sleep(WATCHDOG);
            let _ = watched.lock().unwrap().kill();
        });
        Self {
            child,
            stdin,
            stdout,
            stderr_text,
        }
    }

    /// Sends one JSON-RPC message as one stdout line.
    fn send(&mut self, message: &Value) {
        self.stdin
            .write_all(format!("{message}\n").as_bytes())
            .expect("stdin write");
        self.stdin.flush().expect("stdin flush");
    }

    /// Reads one stdout line and parses it strictly: every byte the
    /// server writes must form a JSON-RPC message on its own line. This
    /// assertion is the protocol-purity check.
    fn read_message(&mut self) -> Value {
        let mut line = String::new();
        let read = self.stdout.read_line(&mut line).expect("stdout read");
        assert!(
            read > 0,
            "the server closed stdout before the exchange ended"
        );
        let value: Value = serde_json::from_str(line.trim())
            .unwrap_or_else(|e| panic!("stdout carried a non-JSON line {line:?}: {e}"));
        assert_eq!(value["jsonrpc"], "2.0", "not a JSON-RPC message: {line:?}");
        value
    }

    /// Sends one request and returns the matching result: the response
    /// id must match, and no protocol-level error may ride along.
    fn call(&mut self, id: u64, method: &str, params: &Value) -> Value {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        let response = self.read_message();
        assert_eq!(
            response["id"],
            json!(id),
            "the response id does not match the request: {response}"
        );
        assert!(
            response.get("error").is_none(),
            "a protocol-level error arrived: {response}"
        );
        response["result"].clone()
    }

    /// Sends one notification: no id, no response expected.
    fn notify(&mut self, method: &str) {
        self.send(&json!({"jsonrpc": "2.0", "method": method}));
    }

    /// The initialize handshake: the server announces itself, the client
    /// confirms. Returns the initialize result.
    fn handshake(&mut self) -> Value {
        let init = self.call(
            1,
            "initialize",
            &json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "mcp-stdio-test", "version": "0.0.0"},
            }),
        );
        self.notify("notifications/initialized");
        init
    }

    /// Closes stdin and waits for the exit. The drain reads stdout to
    /// EOF; every leftover line must also parse as a JSON-RPC message,
    /// so non-JSON bytes cannot hide after the last response. Returns
    /// the exit code and the collected stderr.
    fn shutdown(mut self) -> (i32, String) {
        drop(self.stdin);
        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).expect("stdout drain");
            if read == 0 {
                break;
            }
            if line.trim().is_empty() {
                continue;
            }
            let value: Value = serde_json::from_str(line.trim())
                .unwrap_or_else(|e| panic!("stdout carried a non-JSON line {line:?}: {e}"));
            assert_eq!(value["jsonrpc"], "2.0", "not a JSON-RPC message: {line:?}");
        }
        let code = self
            .child
            .lock()
            .unwrap()
            .wait()
            .expect("the child waits")
            .code()
            .unwrap_or(-1);
        let stderr = std::mem::take(&mut *self.stderr_text.lock().unwrap());
        (code, stderr)
    }
}

/// The isError flag and the first text block of a tools/call result.
fn result_text(result: &Value) -> (bool, String) {
    let is_error = result["isError"].as_bool().unwrap_or(false);
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    (is_error, text)
}

/// The full protocol exchange over the spawned binary: initialize, the
/// initialized notification, tools/list, and one tools/call per tool.
/// Every stdout line must parse as a JSON-RPC message with the matching
/// id; the tool failures carry the CLI error envelope; the server exits
/// cleanly when stdin closes. stderr stays free to carry logs.
#[test]
fn mcp_stdio_serves_the_protocol_on_stdout_only() {
    let env = common::Env::new();
    let mut session = Session::spawn(&env);

    let init = session.handshake();
    let server_info = &init["serverInfo"];
    assert_eq!(server_info["name"], json!("devdocs"));
    let instructions = init["instructions"].as_str().unwrap_or_default();
    assert!(
        !instructions.is_empty(),
        "the initialize result carries no instructions"
    );

    let tools = session.call(2, "tools/list", &json!({}));
    let mut names: Vec<String> = tools["tools"]
        .as_array()
        .expect("the tools array")
        .iter()
        .map(|tool| tool["name"].as_str().expect("the tool name").to_owned())
        .collect();
    names.sort_unstable();
    assert_eq!(names, vec!["open_doc", "search_docs", "verify_doc"]);

    // The index is empty: the miss surfaces as a tool-level error that
    // carries the CLI error envelope.
    let search = session.call(
        3,
        "tools/call",
        &json!({"name": "search_docs", "arguments": {"query": "routing"}}),
    );
    let (search_is_error, search_text) = result_text(&search);
    assert!(search_is_error, "an empty-index search must fail: {search}");
    let envelope: Value = serde_json::from_str(&search_text).expect("the error envelope parses");
    assert!(
        envelope["error"].is_string(),
        "the envelope carries an error code: {envelope}"
    );

    for (id, tool) in [(4, "open_doc"), (5, "verify_doc")] {
        let result = session.call(
            id,
            "tools/call",
            &json!({"name": tool, "arguments": {"ref": "not-a-ref"}}),
        );
        let (is_error, text) = result_text(&result);
        assert!(is_error, "{tool} with a bad ref must fail: {result}");
        let envelope: Value = serde_json::from_str(&text).expect("the error envelope parses");
        assert_eq!(envelope["error"], json!("USAGE"), "{tool}");
    }

    let (code, _stderr) = session.shutdown();
    assert_eq!(code, 0, "the server exits cleanly when stdin closes");
}

/// A client that closes stdin before the initialize handshake ends the
/// server normally: exit 0, nothing on the protocol stream, and no error
/// envelope on stderr (docs/implementation/08 §CLI contract).
#[test]
fn mcp_pre_init_stdin_close_exits_cleanly() {
    let env = common::Env::new();
    let session = Session::spawn(&env);
    let (code, stderr) = session.shutdown();
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        !stderr.contains("UPSTREAM_ERROR"),
        "the close must carry no error envelope: {stderr}"
    );
}

/// The startup migration hook: the serve path opens the store and runs
/// the migrations before the first tool call, so the index file exists
/// under the data root with the shipped schema.
#[test]
fn mcp_startup_runs_the_store_migrations() {
    let env = common::Env::new();
    let mut session = Session::spawn(&env);

    session.handshake();
    let tools = session.call(2, "tools/list", &json!({}));
    assert_eq!(
        tools["tools"].as_array().expect("the tools array").len(),
        3,
        "tools/list answers"
    );

    let (code, stderr) = session.shutdown();
    assert_eq!(code, 0, "stderr: {stderr}");

    // The index file sits at the data root the config resolves to, and
    // the schema ran every shipped migration (the same floor `db
    // migrate` reports: user_version 4).
    let db = env.root.join("data").join("docs.db");
    assert!(db.exists(), "the index file exists under the data root");
    let conn = rusqlite::Connection::open(&db).expect("the db opens");
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("the user_version reads");
    assert_eq!(version, 4, "the schema ran every shipped migration");
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'chunks'",
            [],
            |row| row.get(0),
        )
        .expect("the table count reads");
    assert_eq!(tables, 1, "the known table exists");
}
