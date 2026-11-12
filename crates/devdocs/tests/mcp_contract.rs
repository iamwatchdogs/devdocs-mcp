//! The identical-results gate: the spawned CLI and the in-process MCP
//! server must serve byte-identical JSON envelopes for the same query
//! against the same index, minus the `served_at` wall-clock field.
//! Both sides run on an empty project context — the CLI runs in a clean
//! temp cwd with no lockfiles, and the MCP path never walks lockfiles —
//! and the comparison is the raw string comparison of the CLI stdout
//! (or the stderr miss envelope) against the MCP text content
//! (docs/implementation/08 §CLI contract and §MCP adapter; the corpus
//! lives in `mcp_corpus.rs`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
mod mcp_corpus;

use std::path::PathBuf;

use common::Env;
use devdocs_mcp::DevdocsServer;
use devdocs_model::config::{Config, SearchConfig};
use mcp_corpus::{CORPUS, CorpusQuery, Kind, Outcome};
use rmcp::RoleClient;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResult, JsonObject};
use rmcp::service::RunningService;
use serde_json::{Value, json};

/// The MCP-side config: the same index path the spawned CLI resolves
/// through `DEVDOCS_HOME`, the same built-in search defaults the config
/// file leaves at 2000/8/60, and the same source registry the contract
/// config carries, so the attribution join cannot drift.
fn mcp_config(env: &Env) -> Config {
    let mut config = devdocs_testkit::base_config();
    config.index.path = env.root.join("data").join("docs.db");
    config.search = SearchConfig {
        max_tokens_default: 2000,
        limit_default: 8,
        rrf_k: 60,
    };
    config.sources = common::contract_source_configs();
    config
}

/// Connects an in-process rmcp client to a spawned server over a tokio
/// duplex pair, the same shape the `devdocs-mcp` contract tests use.
/// The server opens the env index itself, so it starts on the fully
/// seeded database.
async fn connect(env: &Env) -> RunningService<RoleClient, ()> {
    let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
    let config = mcp_config(env);
    let store = devdocs_retrieve::wiring::open_store(&config, false).expect("the env index opens");
    let server = DevdocsServer::new(store, config);
    let _server_task = tokio::spawn(async move {
        let running = server.serve(server_transport).await.expect("server start");
        running.waiting().await.expect("server loop");
    });
    // The unit type implements `ClientHandler` in rmcp 3.8.0, so the
    // default client behavior needs no custom handler type.
    ().serve(client_transport).await.expect("client start")
}

/// One gate session: the isolated corpus env, the config path the CLI
/// runs take, the clean cwd the CLI runs in, and the in-process MCP
/// client. The CLI cwd is a dedicated empty directory so the lockfile
/// walk finds nothing — both sides then compare empty context against
/// empty context.
struct Gate {
    env: Env,
    config_path: PathBuf,
    cwd: PathBuf,
    client: RunningService<RoleClient, ()>,
}

impl Gate {
    /// Builds the gate over the full 50-query contract corpus.
    async fn contract() -> Self {
        let (env, config_path) = common::contract_env();
        let cwd = env.root.join("cli-cwd");
        std::fs::create_dir_all(&cwd).expect("the cli cwd creates");
        let client = connect(&env).await;
        let gate = Self {
            env,
            config_path,
            cwd,
            client,
        };
        // The comparison rule needs both sides on an empty project
        // context; a polluted host would leak lockfile deps into the CLI
        // side and break the identity this gate proves.
        assert_eq!(
            devdocs_retrieve::ProjectContext::detect(&gate.cwd),
            devdocs_retrieve::ProjectContext::empty(),
            "the cli cwd must carry no lockfiles"
        );
        gate
    }

    /// Runs the spawned CLI with the isolated environment in the clean
    /// cwd, and returns (exit code, stdout, stderr).
    fn cli(&self, args: &[&str]) -> (i32, String, String) {
        let mut cmd = self.env.std_cmd(args);
        cmd.current_dir(&self.cwd);
        let out = cmd.output().expect("the binary runs");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }

    /// The CLI arguments one corpus entry maps to: the same query, scope,
    /// budget, and config path the MCP tool call carries.
    fn cli_args(&self, entry: &CorpusQuery) -> Vec<String> {
        let mut args = match entry.kind {
            Kind::Search => {
                let mut args = vec![String::from("search"), entry.query.to_owned()];
                if let Some(library) = entry.library {
                    args.push(String::from("--library"));
                    args.push(library.to_owned());
                }
                if let Some(version) = entry.version {
                    args.push(String::from("--version"));
                    args.push(version.to_owned());
                }
                args
            }
            Kind::Open => vec![
                String::from("open"),
                entry
                    .doc_ref
                    .expect("an open entry carries a ref")
                    .to_owned(),
            ],
            Kind::Verify => vec![
                String::from("verify"),
                entry
                    .doc_ref
                    .expect("a verify entry carries a ref")
                    .to_owned(),
            ],
        };
        if let Some(max_tokens) = entry.max_tokens {
            args.push(String::from("--max-tokens"));
            args.push(max_tokens.to_string());
        }
        args.push(String::from("--json"));
        args.push(String::from("--config"));
        args.push(self.config_path.display().to_string());
        args
    }

    /// Calls the MCP tool one corpus entry maps to, with the same
    /// parameters the CLI flags carry.
    async fn mcp_call(&self, entry: &CorpusQuery) -> CallToolResult {
        let tool = match entry.kind {
            Kind::Search => "search_docs",
            Kind::Open => "open_doc",
            Kind::Verify => "verify_doc",
        };
        let arguments = match entry.kind {
            Kind::Search => {
                let mut map = json!({ "query": entry.query });
                if let Some(library) = entry.library {
                    map["library"] = json!(library);
                }
                if let Some(version) = entry.version {
                    map["version"] = json!(version);
                }
                if let Some(detail) = entry.detail {
                    map["detail"] = json!(detail);
                }
                if let Some(max_tokens) = entry.max_tokens {
                    map["max_tokens"] = json!(max_tokens);
                }
                map
            }
            Kind::Open => {
                let mut map = json!({ "ref": entry.doc_ref.expect("an open entry carries a ref") });
                if let Some(max_tokens) = entry.max_tokens {
                    map["max_tokens"] = json!(max_tokens);
                }
                map
            }
            Kind::Verify => json!({ "ref": entry.doc_ref.expect("a verify entry carries a ref") }),
        };
        let args: JsonObject = arguments.as_object().expect("a parameter object").clone();
        self.client
            .call_tool(CallToolRequestParams::new(tool).with_arguments(args))
            .await
            .expect("the tool call answers")
    }

    /// The hit comparison: the MCP text content and the CLI stdout are
    /// byte-identical minus `served_at`, and the structured content is
    /// the same JSON the text carries (compared as a value, so the key
    /// order the transport may reorder cannot hide a content drift).
    async fn compare_hit(&self, label: &str, entry: &CorpusQuery) {
        let result = self.mcp_call(entry).await;
        assert_eq!(result.is_error, Some(false), "{label}: a hit must serve");
        let text = mcp_text(&result).to_owned();
        let args = self.cli_args(entry);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, stdout, stderr) = self.cli(&arg_refs);
        assert_eq!(code, 0, "{label}: stdout {stdout:?} stderr {stderr:?}");
        assert_eq!(
            common::strip_served_at(&text),
            common::strip_served_at(&stdout),
            "{label}: the envelopes must be byte-identical except served_at"
        );
        let value = serde_json::from_str::<Value>(&text).expect("the text parses");
        let structured = result
            .structured_content
            .clone()
            .expect("structured content");
        assert_eq!(
            structured, value,
            "{label}: the structured content is the text JSON"
        );
        // The budgeted corpus entries carry a 15-token budget the rank-1
        // chunk exceeds, so the page must show the budget-and-resume gate
        // (docs/implementation/08 §CLI contract): truncated with a
        // non-empty resume cursor.
        if entry.max_tokens.is_some() {
            assert_eq!(value["truncated"], json!(true), "{label}: {value}");
            assert!(
                value["next_cursor"]
                    .as_str()
                    .is_some_and(|cursor| !cursor.is_empty()),
                "{label}: {value}"
            );
        }
    }

    /// The miss comparison: the CLI answers on stderr with the error
    /// envelope, the MCP tool answers with a tool-level error whose text
    /// content is the same single-line JSON. The error envelope carries
    /// no `served_at`, so the strings compare whole, and the `error`
    /// code is the entry's `expected_error`. Returns the MCP text so a
    /// caller can pin further envelope fields.
    async fn compare_miss(&self, label: &str, entry: &CorpusQuery) -> String {
        let result = self.mcp_call(entry).await;
        assert_eq!(result.is_error, Some(true), "{label}: a miss must fail");
        let text = mcp_text(&result).to_owned();
        let args = self.cli_args(entry);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, stdout, stderr) = self.cli(&arg_refs);
        assert_eq!(code, 2, "{label}: stdout {stdout:?} stderr {stderr:?}");
        assert!(stdout.is_empty(), "{label}: stdout must stay clean");
        assert_eq!(
            text.trim(),
            stderr.trim(),
            "{label}: the miss envelopes must be byte-identical"
        );
        let expected = entry
            .expected_error
            .expect("a miss entry carries its error code");
        let envelope: Value = serde_json::from_str(text.trim()).expect("the envelope parses");
        assert_eq!(envelope["error"], json!(expected), "{label}: {envelope}");
        text
    }

    /// The verify comparison, field-level: the MCP tool reports
    /// `{"valid", "content_hash"}` while the CLI serves the result
    /// envelope, so the gate compares `valid` against true and the two
    /// content hashes against each other.
    async fn compare_verify(&self, label: &str, entry: &CorpusQuery) {
        let result = self.mcp_call(entry).await;
        assert_eq!(result.is_error, Some(false), "{label}: a verify must serve");
        let text = mcp_text(&result).to_owned();
        let report: Value = serde_json::from_str(&text).expect("the report parses");
        assert_eq!(report["valid"], json!(true), "{label}: {report}");
        let structured = result
            .structured_content
            .clone()
            .expect("structured content");
        assert_eq!(
            structured,
            serde_json::from_str::<Value>(&text).expect("the text parses"),
            "{label}: the structured content is the text JSON"
        );

        let args = self.cli_args(entry);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let (code, stdout, stderr) = self.cli(&arg_refs);
        assert_eq!(code, 0, "{label}: stdout {stdout:?} stderr {stderr:?}");
        let envelope: Value =
            serde_json::from_str(stdout.trim()).expect("the verify envelope parses");
        let item = &envelope["results"][0];
        assert_eq!(
            report["content_hash"], item["content_hash"],
            "{label}: both edges report the same content hash"
        );
    }

    /// The detail comparison, shape-only (the CLI has no `--detail`
    /// flag): the projection text snapshots, and the projected field
    /// sets hold — `name` keeps the identity fields only, `summary`
    /// drops `examples` only.
    async fn compare_detail(&self, entry: &CorpusQuery) {
        let label = format!("detail[{}]", entry.detail.expect("a detail entry"));
        let result = self.mcp_call(entry).await;
        assert_eq!(
            result.is_error,
            Some(false),
            "{label}: a projection must serve"
        );
        let text = mcp_text(&result).to_owned();
        let value: Value = serde_json::from_str(&text).expect("the projection parses");
        match entry.detail.expect("a detail entry") {
            "name" => {
                insta::assert_json_snapshot!("detail_name_search", value, {
                    ".served_at" => "<served_at>"
                });
                let mut keys = sorted_keys(&value);
                assert_eq!(
                    keys,
                    vec!["next_cursor", "results", "served_at", "truncated"]
                );
                for item in value["results"].as_array().expect("results") {
                    keys = sorted_keys(item);
                    assert_eq!(
                        keys,
                        vec!["ref", "served_version", "title", "version_status"],
                        "{label}: the name projection keeps the identity fields only"
                    );
                }
            }
            "summary" => {
                insta::assert_json_snapshot!("detail_summary_search", value, {
                    ".served_at" => "<served_at>"
                });
                // The expected key set derives from a full envelope item
                // of the same query minus `examples`, so a projection
                // that drops any other field cannot pass.
                let full_entry = CorpusQuery {
                    detail: None,
                    ..entry.clone()
                };
                let full_result = self.mcp_call(&full_entry).await;
                assert_eq!(
                    full_result.is_error,
                    Some(false),
                    "{label}: the full call must serve"
                );
                let full_value: Value =
                    serde_json::from_str(mcp_text(&full_result)).expect("the envelope parses");
                let full_items = full_value["results"].as_array().expect("results");
                let items = value["results"].as_array().expect("results");
                assert_eq!(
                    items.len(),
                    full_items.len(),
                    "{label}: both calls serve the same result count"
                );
                let Some(full_item) = full_items.first() else {
                    panic!("{label}: the corpus detail entry must hit");
                };
                let mut expected = sorted_keys(full_item);
                expected.retain(|key| *key != "examples");
                for item in items {
                    assert_eq!(
                        sorted_keys(item),
                        expected,
                        "{label}: the summary projection drops examples only"
                    );
                }
            }
            other => panic!("unknown detail level {other}"),
        }
    }
}

/// The first text block of one tool result: the exact JSON bytes the
/// server put there.
fn mcp_text(result: &CallToolResult) -> &str {
    result
        .content
        .first()
        .expect("a content block")
        .as_text()
        .expect("a text block")
        .text
        .as_ref()
}

/// The sorted key set of one JSON object.
fn sorted_keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

/// The plain-search slice: unscoped, unbudgeted, unprojected hits.
fn is_plain(entry: &CorpusQuery) -> bool {
    entry.kind == Kind::Search
        && entry.library.is_none()
        && entry.version.is_none()
        && entry.detail.is_none()
        && entry.max_tokens.is_none()
        && entry.outcome == Outcome::Hit
}

/// The scoped slice: every search that names a library or a version —
/// the hits, and the license-refused trio that misses.
fn is_scoped(entry: &CorpusQuery) -> bool {
    entry.kind == Kind::Search && (entry.library.is_some() || entry.version.is_some())
}

/// The miss-and-budget slice: gibberish misses, budgeted truncated
/// pages, and the version-pinned opens that miss.
fn is_miss_or_budget(entry: &CorpusQuery) -> bool {
    match entry.kind {
        Kind::Search => {
            entry.max_tokens.is_some()
                || (entry.outcome == Outcome::Miss && entry.library.is_none())
        }
        Kind::Open => entry.outcome == Outcome::Miss,
        Kind::Verify => false,
    }
}

/// The open-and-detail slice: the opens that serve, and the MCP-only
/// detail projections.
fn is_open_or_detail(entry: &CorpusQuery) -> bool {
    (entry.kind == Kind::Open && entry.outcome == Outcome::Hit) || entry.detail.is_some()
}

/// The corpus holds exactly 50 entries and the slice predicates
/// partition it, so a corpus edit that moves an entry between slices
/// fails here first.
#[test]
fn the_corpus_holds_exactly_50_entries_in_five_slices() {
    assert_eq!(CORPUS.len(), 50, "the gate owns exactly 50 queries");
    let counts = (
        CORPUS.iter().filter(|e| is_plain(e)).count(),
        CORPUS.iter().filter(|e| is_scoped(e)).count(),
        CORPUS.iter().filter(|e| is_miss_or_budget(e)).count(),
        CORPUS.iter().filter(|e| is_open_or_detail(e)).count(),
        CORPUS.iter().filter(|e| e.kind == Kind::Verify).count(),
    );
    assert_eq!(counts, (19, 12, 8, 8, 3), "the slices partition the corpus");
    for entry in &CORPUS {
        match entry.kind {
            Kind::Search => assert!(!entry.query.is_empty(), "a search carries a query"),
            Kind::Open | Kind::Verify => {
                assert!(
                    entry.doc_ref.is_some_and(|r| r.starts_with("docs://")),
                    "a ref entry carries a docs:// ref"
                );
            }
        }
    }
}

/// The plain-search slice: 19 unscoped phrases (two on the symbol rung)
/// whose MCP text content and CLI stdout match byte for byte minus
/// `served_at`.
#[tokio::test]
async fn plain_searches_match_the_cli_byte_for_byte() {
    let gate = Gate::contract().await;
    let entries: Vec<&CorpusQuery> = CORPUS.iter().filter(|e| is_plain(e)).collect();
    for (index, entry) in entries.iter().enumerate() {
        gate.compare_hit(&format!("plain[{index}] {}", entry.query), entry)
            .await;
    }
}

/// The scoped slice: library-scoped and version-pinned searches hit
/// byte for byte, and the license-refused trio answers the same
/// `NOT_AVAILABLE` envelope on both edges.
#[tokio::test]
async fn scoped_and_refused_searches_match_the_cli() {
    let gate = Gate::contract().await;
    let entries: Vec<&CorpusQuery> = CORPUS.iter().filter(|e| is_scoped(e)).collect();
    for (index, entry) in entries.iter().enumerate() {
        let label = format!("scoped[{index}] {} {:?}", entry.query, entry.library);
        match entry.outcome {
            Outcome::Hit => gate.compare_hit(&label, entry).await,
            Outcome::Miss => {
                gate.compare_miss(&label, entry).await;
            }
        }
    }
}

/// The fresh-state sweep gate for the MCP search: the `expiring` source
/// keeps a serving license status until a read path sweeps it, so with
/// no CLI run yet the first MCP query must still answer the
/// `NOT_AVAILABLE` miss — the sweep runs inside the `search_docs`
/// handler (docs/implementation/07 §Expiry sweep). Without that sweep
/// the source serves and this test fails.
#[tokio::test]
async fn an_expired_source_misses_before_any_cli_read_sweeps_it() {
    let gate = Gate::contract().await;
    let entry = CORPUS
        .iter()
        .find(|e| e.query == "expiring marker")
        .expect("the expiring corpus entry");
    assert_eq!(
        entry.library,
        Some("expiring"),
        "the gate sweeps the expiring source only"
    );
    // The MCP call runs first; no CLI search touched the index yet, so
    // only the handler sweep can refuse the query. The miss envelope's
    // reason code is pinned here so the gate cannot pass on a
    // different-but-matching drift.
    let text = gate.compare_miss("fresh-expiry", entry).await;
    let envelope: Value = serde_json::from_str(text.trim()).expect("the envelope parses");
    assert_eq!(envelope["error"], json!("NOT_AVAILABLE"), "{envelope}");
}

/// The miss-and-budget slice: gibberish misses echo the `NO_RESULT`
/// envelope, version-pinned opens echo `PKG_NOT_INDEXED`, and the
/// budgeted pages serve the same truncated envelope with the same
/// resume cursor on both edges.
#[tokio::test]
async fn misses_and_budgeted_pages_match_the_cli() {
    let gate = Gate::contract().await;
    let entries: Vec<&CorpusQuery> = CORPUS.iter().filter(|e| is_miss_or_budget(e)).collect();
    for (index, entry) in entries.iter().enumerate() {
        let label = format!(
            "miss[{index}] {} {:?}",
            entry.query,
            entry.doc_ref.unwrap_or("(search)")
        );
        match entry.outcome {
            Outcome::Hit => gate.compare_hit(&label, entry).await,
            Outcome::Miss => {
                gate.compare_miss(&label, entry).await;
            }
        }
    }
}

/// The open-and-detail slice: the serving opens match the CLI byte for
/// byte, and the MCP-only detail projections snapshot their shape and
/// hold their projected field sets.
#[tokio::test]
async fn opens_and_detail_projections_match_the_cli() {
    let gate = Gate::contract().await;
    let entries: Vec<&CorpusQuery> = CORPUS.iter().filter(|e| is_open_or_detail(e)).collect();
    for (index, entry) in entries.iter().enumerate() {
        if entry.detail.is_some() {
            gate.compare_detail(entry).await;
        } else {
            let label = format!("open[{index}] {}", entry.doc_ref.unwrap_or("?"));
            match entry.outcome {
                Outcome::Hit => gate.compare_hit(&label, entry).await,
                Outcome::Miss => {
                    gate.compare_miss(&label, entry).await;
                }
            }
        }
    }
}

/// The verify slice: the MCP report and the CLI envelope agree on the
/// re-derived content hash, and the MCP report carries `valid: true`.
#[tokio::test]
async fn verify_doc_matches_the_cli_hash() {
    let gate = Gate::contract().await;
    let entries: Vec<&CorpusQuery> = CORPUS.iter().filter(|e| e.kind == Kind::Verify).collect();
    for (index, entry) in entries.iter().enumerate() {
        gate.compare_verify(&format!("verify[{index}]"), entry)
            .await;
    }
}

/// `verify_doc` on a hash-drifted chunk answers the `HASH_MISMATCH`
/// error envelope — the same drift the CLI reports (the drift itself is
/// planted through the testkit helper, the same move the CLI drift test
/// makes).
#[tokio::test]
async fn verify_doc_reports_hash_mismatch_on_drift() {
    let (env, _config) = common::indexed_env();
    devdocs_testkit::corrupt_one_chunk_text(&env.root.join("data/docs.db")).expect("tamper");
    let client = connect(&env).await;
    let args: JsonObject = json!({ "ref": "docs://acme/v1.0/intro" })
        .as_object()
        .expect("a parameter object")
        .clone();
    let result = client
        .call_tool(CallToolRequestParams::new("verify_doc").with_arguments(args))
        .await
        .expect("the tool call answers");
    assert_eq!(result.is_error, Some(true), "a drifted chunk must fail");
    let envelope: Value = serde_json::from_str(mcp_text(&result)).expect("the envelope parses");
    assert_eq!(envelope["error"], json!("HASH_MISMATCH"), "{envelope}");
}

/// `open_doc` with an unparseable ref answers the `USAGE` error
/// envelope end to end through the rmcp client — the same envelope the
/// CLI prints on stderr for a malformed ref.
#[tokio::test]
async fn open_doc_with_an_unparseable_ref_reports_usage() {
    let env = Env::new();
    let client = connect(&env).await;
    let args: JsonObject = json!({ "ref": "not-a-ref" })
        .as_object()
        .expect("a parameter object")
        .clone();
    let result = client
        .call_tool(CallToolRequestParams::new("open_doc").with_arguments(args))
        .await
        .expect("the tool call answers");
    assert_eq!(result.is_error, Some(true), "a bad ref must fail");
    let envelope: Value = serde_json::from_str(mcp_text(&result)).expect("the envelope parses");
    assert_eq!(envelope["error"], json!("USAGE"), "{envelope}");
}
