//! The MCP adapter contract tests: the tool surface, the annotations,
//! the server instructions, and the envelope identity through an
//! in-process rmcp client (docs/implementation/04: contract tests live
//! here; docs/implementation/08 §MCP adapter owns the shape).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use devdocs_mcp::DevdocsServer;
use devdocs_model::config::{Config, SearchConfig};
use devdocs_model::envelope::ResultEnvelope;
use devdocs_model::error::ErrorEnvelope;
use devdocs_store::Store;
use rmcp::ServerHandler;
use rmcp::ServiceError;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, ErrorCode, JsonObject};
use serde_json::{Value, json};

/// The byte ceiling for the whole tool surface, the compact `tools/list`
/// JSON. Three tools with compact schemas must stay far below the
/// 10%-of-context deferral threshold (docs/implementation/08 §MCP
/// adapter, gates).
const TOOL_SURFACE_MAX_BYTES: usize = 6000;

/// The measured tool-surface byte count docs/implementation/08 quotes.
/// Update the doc and this constant together.
const TOOL_SURFACE_MEASURED_BYTES: usize = 2582;

/// A seeded index in a temp dir with a config pointing at it. The store
/// carries one `acme` source at `approved_index` with one chunk under
/// `Guide > Routing`, the shape the retrieval tests seed.
struct TestEnv {
    _dir: tempfile::TempDir,
    config: Config,
    store: Arc<Store>,
}

/// Builds the seeded env. The chunk text names the concept the search
/// test queries; the seeded source keeps `approved_index`, the ingest
/// bootstrap stage, so the search serves verbatim text.
fn setup() -> TestEnv {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("docs.db");
    let (store, _source_id) = devdocs_testkit::seeded_store(
        &db,
        "acme",
        r#"["Guide","Routing"]"#,
        "The routing layer maps requests.",
    )
    .unwrap();
    let mut config = devdocs_testkit::base_config();
    config.index.path = db;
    config.search = SearchConfig {
        max_tokens_default: 2000,
        limit_default: 8,
        rrf_k: 60,
    };
    TestEnv {
        _dir: dir,
        config,
        store: Arc::new(store),
    }
}

/// Connects an in-process rmcp client to a spawned server over a tokio
/// duplex pair, the same shape rmcp's own initialization tests use. The
/// server side runs through [`rmcp::ServiceExt::serve`] and waits for
/// the client to disconnect.
async fn connect(env: &TestEnv) -> rmcp::service::RunningService<rmcp::RoleClient, ()> {
    let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
    let server = DevdocsServer::new(Arc::clone(&env.store), env.config.clone());
    let _server_task = tokio::spawn(async move {
        let running = server.serve(server_transport).await.expect("server start");
        running.waiting().await.expect("server loop")
    });
    // The unit type implements `ClientHandler`, so the default client
    // behavior needs no custom handler type (the workspace declares
    // rmcp 3.8.0, the lockfile resolves 3.9.0 — docs/implementation/08).
    ().serve(client_transport).await.expect("client start")
}

/// Calls one tool by name with the given arguments and returns the call
/// result.
async fn call_tool(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    name: &'static str,
    arguments: Value,
) -> rmcp::model::CallToolResult {
    let args: JsonObject = arguments.as_object().unwrap().clone();
    client
        .call_tool(CallToolRequestParams::new(name).with_arguments(args))
        .await
        .unwrap()
}

/// The tools/list result names exactly the three documented tools. The
/// router serves them in its own order, so the comparison sorts both
/// sides.
#[tokio::test]
async fn tools_list_returns_exactly_three_named_tools() {
    let env = setup();
    let client = connect(&env).await;
    let listing = client.list_tools(None).await.unwrap();
    let mut names: Vec<&str> = listing
        .tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect();
    names.sort_unstable();
    assert_eq!(names, vec!["open_doc", "search_docs", "verify_doc"]);
}

/// The full tools/list result matches the snapshot: the schemas must
/// match docs/implementation/08 §MCP adapter table.
#[tokio::test]
async fn the_tools_list_matches_the_schema_snapshot() {
    let env = setup();
    let client = connect(&env).await;
    let listing = client.list_tools(None).await.unwrap();
    let value = serde_json::to_value(&listing).unwrap();
    insta::assert_json_snapshot!("tools_list", value);
}

/// Every tool carries all four annotations with the honest values
/// (docs/implementation/08 §MCP adapter): read-only, not destructive,
/// idempotent, closed world. The spec defaults are the inverse, so a
/// missing field would flip the meaning.
#[tokio::test]
async fn every_tool_carries_all_four_annotations() {
    let env = setup();
    let client = connect(&env).await;
    let listing = client.list_tools(None).await.unwrap();
    assert_eq!(listing.tools.len(), 3);
    for tool in &listing.tools {
        let annotations = tool.annotations.as_ref().unwrap_or_else(|| {
            panic!("tool {} carries no annotations", tool.name);
        });
        assert_eq!(annotations.read_only_hint, Some(true), "{}", tool.name);
        assert_eq!(annotations.destructive_hint, Some(false), "{}", tool.name);
        assert_eq!(annotations.idempotent_hint, Some(true), "{}", tool.name);
        assert_eq!(annotations.open_world_hint, Some(false), "{}", tool.name);
    }
}

/// The server instructions match the snapshot; the text is verbatim from
/// docs/implementation/08 §MCP adapter.
#[tokio::test]
async fn the_server_instructions_match_the_snapshot() {
    let env = setup();
    let server = DevdocsServer::new(Arc::clone(&env.store), env.config.clone());
    let info = server.get_info();
    let instructions = info.instructions.clone().unwrap_or_default();
    insta::assert_snapshot!("server_instructions", instructions);
}

/// The `search_docs` tool returns a success result whose text content
/// parses as the result envelope, and whose structured content is the
/// same JSON.
#[tokio::test]
async fn search_docs_returns_the_result_envelope() {
    let env = setup();
    let client = connect(&env).await;
    let result = call_tool(
        &client,
        "search_docs",
        json!({"query": "routing", "library": "acme"}),
    )
    .await;
    assert_eq!(result.is_error, Some(false));
    let structured = result
        .structured_content
        .clone()
        .expect("structured content");
    let text = &result.content.first().unwrap().as_text().unwrap().text;
    let envelope: ResultEnvelope = serde_json::from_str(text).unwrap();
    assert_eq!(envelope.results.len(), 1);
    assert_eq!(
        envelope.results[0].doc_ref,
        "docs://acme/v1.0.0/guide/routing"
    );
    assert_eq!(envelope.results[0].served_version, "1.0.0");
    assert_eq!(envelope.results[0].title, "Routing");
    // The structured content is the same JSON the text carries (the same
    // comparison rule the identical-results gate applies).
    assert_eq!(
        structured,
        serde_json::from_str::<Value>(text).expect("the text parses"),
        "the structured content is the text JSON"
    );
}

/// The `open_doc` tool with an unparseable ref returns a tool-level error
/// whose text content parses as the CLI error envelope JSON with the
/// USAGE code — the exact single-line shape the CLI prints on stderr. A
/// ref that parses but names no section maps to `NO_RESULT` with its
/// miss reason, the same split the CLI makes.
#[tokio::test]
async fn open_doc_with_a_bad_ref_returns_the_cli_error_envelope() {
    let env = setup();
    let client = connect(&env).await;
    let result = call_tool(&client, "open_doc", json!({"ref": "not-a-ref"})).await;
    assert_eq!(result.is_error, Some(true));
    let text = &result.content.first().unwrap().as_text().unwrap().text;
    let envelope: ErrorEnvelope = serde_json::from_str(text).unwrap();
    assert_eq!(envelope.error, "USAGE");
    assert!(envelope.detail.is_some());

    let miss = call_tool(
        &client,
        "open_doc",
        json!({"ref": "docs://acme/v1.0.0/no-such-section"}),
    )
    .await;
    assert_eq!(miss.is_error, Some(true));
    let text = &miss.content.first().unwrap().as_text().unwrap().text;
    let envelope: ErrorEnvelope = serde_json::from_str(text).unwrap();
    assert_eq!(envelope.error, "NO_RESULT");
    assert!(envelope.miss_reason.is_some());
}

/// The `verify_doc` tool confirms the seeded citation and returns the
/// valid flag with the content hash. The seeded chunk carries a fixed
/// placeholder hash, so the test first writes the hash the verify path
/// re-derives for the seeded text — the drift path itself is covered by
/// the retrieve-layer tests.
#[tokio::test]
async fn verify_doc_returns_the_citation_check() {
    let env = setup();
    // The stored text carries no contextual header, so the verify path
    // hashes the text as-is.
    let hash_bytes = devdocs_store::content_hash("The routing layer maps requests.");
    let hash = devdocs_retrieve::hex(&hash_bytes);
    let conn = rusqlite::Connection::open(&env.config.index.path).unwrap();
    conn.execute(
        "UPDATE chunks SET content_hash = ?1",
        rusqlite::params![&hash_bytes[..]],
    )
    .unwrap();
    drop(conn);

    let client = connect(&env).await;
    let result = call_tool(
        &client,
        "verify_doc",
        json!({"ref": "docs://acme/v1.0.0/guide/routing"}),
    )
    .await;
    assert_eq!(result.is_error, Some(false));
    let structured = result.structured_content.unwrap_or_default();
    assert_eq!(structured["valid"], json!(true));
    assert_eq!(structured["content_hash"], json!(hash));
}

/// The whole tool surface stays under the byte ceiling: three tools with
/// compact schemas cost a few hundred tokens
/// (docs/implementation/08 §MCP adapter, gates). The assertion names the
/// measured cost, so a drift shows its size, and pins the exact byte
/// count the doc quotes.
#[tokio::test]
async fn the_tool_surface_stays_under_the_byte_ceiling() {
    let env = setup();
    let client = connect(&env).await;
    let listing = client.list_tools(None).await.unwrap();
    let json = serde_json::to_string(&serde_json::to_value(&listing).unwrap()).unwrap();
    assert_eq!(
        json.len(),
        TOOL_SURFACE_MEASURED_BYTES,
        "the tool surface measured {} bytes, the doc quotes {TOOL_SURFACE_MEASURED_BYTES}; update both together",
        json.len()
    );
    assert!(
        json.len() < TOOL_SURFACE_MAX_BYTES,
        "the tool surface measured {} bytes, ceiling {}",
        json.len(),
        TOOL_SURFACE_MAX_BYTES
    );
}

/// An unknown tool name stays a JSON-RPC protocol error, not a tool
/// result: the rmcp router answers `INVALID_PARAMS` before any handler
/// runs (docs/implementation/08 §MCP adapter).
#[tokio::test]
async fn an_unknown_tool_is_a_protocol_error() {
    let env = setup();
    let client = connect(&env).await;
    let err = client
        .call_tool(CallToolRequestParams::new("no_such_tool").with_arguments(JsonObject::new()))
        .await
        .expect_err("an unknown tool must fail at the protocol level");
    match err {
        ServiceError::McpError(e) => assert_eq!(e.code, ErrorCode::INVALID_PARAMS, "{e:?}"),
        other => panic!("expected the protocol error, got {other:?}"),
    }
}

/// Malformed arguments stay at the tool level, the split the rmcp router
/// draws: it turns the deserialization failure into a tool error result,
/// not a JSON-RPC error (docs/implementation/08 §MCP adapter). The test
/// pins the missing required field and the wrong-typed one. The pinned
/// `failed to deserialize parameters:` prefix is rmcp's own string, not
/// ours: an rmcp bump can change it, and the assertion fails on that bump
/// (the workspace declares rmcp 3.8.0, the lockfile resolves 3.9.0 —
/// docs/implementation/08 §MCP adapter).
#[tokio::test]
async fn malformed_arguments_stay_tool_level_errors() {
    let env = setup();
    let client = connect(&env).await;
    for args in [json!({}), json!({"query": 42})] {
        let args: JsonObject = args.as_object().unwrap().clone();
        let result = client
            .call_tool(CallToolRequestParams::new("search_docs").with_arguments(args))
            .await
            .expect("malformed arguments must not fail the protocol");
        assert_eq!(result.is_error, Some(true));
        let text = &result.content.first().unwrap().as_text().unwrap().text;
        assert!(
            text.starts_with("failed to deserialize parameters:"),
            "text: {text}"
        );
    }
}
