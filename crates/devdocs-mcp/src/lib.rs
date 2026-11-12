//! The MCP server adapter (docs/implementation/08 §MCP adapter).
//!
//! `devdocs mcp` serves the three documented tools over stdio through
//! rmcp. The tools run the shared retrieve pipeline through the shared
//! wiring, so the MCP path and the CLI path produce identical envelopes
//! for identical queries against the same index. stdout carries only the
//! protocol stream; logs stay on stderr
//! (`.agents/instructions/observability.instruction.md`).

pub mod detail;
// The error helpers expose no public items: only the tool handlers in
// this crate call them, so the module stays crate-private.
pub(crate) mod error;

use std::sync::Arc;

use devdocs_model::config::Config;
use devdocs_model::error::{Error, UpstreamCode};
use devdocs_retrieve::ProjectContext;
use devdocs_retrieve::wiring;
use devdocs_store::Store;
use rmcp::ServiceExt;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ErrorData, Implementation, ServerCapabilities, ServerConfig};
use rmcp::{tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::detail::{Detail, VerifyReport};

/// The server instructions text, verbatim from docs/implementation/08
/// §MCP adapter.
pub const SERVER_INSTRUCTIONS: &str = "devdocs serves version-pinned official docs from a local index.\n\
     search_docs: one concept per query. open_doc: read a section by ref from search_docs results only.\n\
     Stop after 3 searches per question. verify_doc confirms a citation before you rely on it.";

/// Serves the MCP adapter over stdio until the client disconnects.
///
/// The store opens through [`wiring::open_store`] with `force = false`:
/// the startup migration runs, and no `--force` flag exists on the MCP
/// path. The caller owns the tracing subscriber; this function writes
/// nothing to stdout except the protocol stream.
///
/// # Errors
/// [`Error::Upstream`] when the store cannot open or migrate, the rmcp
/// server cannot start, or the serve loop fails.
pub async fn serve(config: &Config) -> Result<(), Error> {
    // The store is sync SQLite behind a Mutex: async code reaches it
    // only inside spawn_blocking
    // (.agents/instructions/async-and-performance.instruction.md); the
    // closure takes an owned config because spawn_blocking needs 'static.
    let blocking_config = config.clone();
    let store = match tokio::task::spawn_blocking(move || {
        wiring::open_store(&blocking_config, false)
    })
    .await
    {
        Ok(Ok(store)) => store,
        Ok(Err(err)) => return Err(err),
        // defensive: the block touches no unwinding state; the arm keeps
        // a panicked worker a startup failure, not a hang.
        Err(join) => {
            return Err(Error::Upstream {
                code: UpstreamCode::UpstreamError,
                detail: format!("the store open task failed: {join}"),
            });
        }
    };
    let server = DevdocsServer::new(store, config.clone());
    // stdio_pipes drives the streams through the non-blocking pipe
    // driver on Linux, and falls back to the blocking handles on other
    // platforms (the workspace declares rmcp 3.8.0, the lockfile
    // resolves 3.9.0 — docs/implementation/08 §MCP adapter).
    let running = match server.serve(rmcp::transport::stdio_pipes()).await {
        // rmcp reports a client that closes stdin before the initialize
        // handshake as `ConnectionClosed`: the transport receive side hit
        // end of stream. A client that closes the pipe ends the server
        // normally, so this is a clean exit, not an upstream error.
        Err(rmcp::service::ServerInitializeError::ConnectionClosed(_)) => return Ok(()),
        Ok(running) => running,
        Err(e) => {
            return Err(Error::Upstream {
                code: UpstreamCode::UpstreamError,
                detail: format!("the mcp server failed to start: {e}"),
            });
        }
    };
    running.waiting().await.map_err(|e| Error::Upstream {
        code: UpstreamCode::UpstreamError,
        detail: format!("the mcp serve loop failed: {e}"),
    })?;
    Ok(())
}

/// The MCP server state: the opened index, the declarative config, and
/// the rmcp tool router.
#[derive(Debug)]
pub struct DevdocsServer {
    store: Arc<Store>,
    config: Config,
    tool_router: ToolRouter<Self>,
}

/// The `search_docs` tool parameters (docs/implementation/08 §MCP
/// adapter).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// One concept per query.
    pub query: String,
    /// The library to scope the search to, when known.
    pub library: Option<String>,
    /// The version to pin the query to, when known.
    pub version: Option<String>,
    /// How much of each item to serve. The default is `full`.
    pub detail: Option<Detail>,
    /// The output budget in tokens. The config default applies when
    /// absent.
    pub max_tokens: Option<u64>,
}

/// The `open_doc` tool parameters (docs/implementation/08 §MCP adapter).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct OpenParams {
    /// The section ref to read. Refs come only from `search_docs`
    /// results.
    #[serde(rename = "ref")]
    pub doc_ref: String,
    /// The output budget in tokens. The config default applies when
    /// absent.
    pub max_tokens: Option<u64>,
}

/// The `verify_doc` tool parameters (docs/implementation/08 §MCP
/// adapter).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct VerifyParams {
    /// The section ref to confirm. Refs come only from `search_docs`
    /// results.
    #[serde(rename = "ref")]
    pub doc_ref: String,
}

#[tool_router]
impl DevdocsServer {
    /// Builds the server over the already-opened store. Callers that
    /// serve stdio use [`serve`]; tests and other transports build the
    /// server and drive [`rmcp::ServiceExt::serve`] themselves.
    #[must_use]
    pub fn new(store: Arc<Store>, config: Config) -> Self {
        Self {
            store,
            config,
            tool_router: Self::tool_router(),
        }
    }

    /// Searches the pinned local index for one concept.
    #[tool(
        name = "search_docs",
        description = "Search the version-pinned official docs in the local index. One concept per query. Read a result with open_doc.",
        // Not read-only: the read can record the expiry transition
        // (docs/implementation/07 §Expiry sweep).
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false,
        )
    )]
    pub async fn search_docs(
        &self,
        params: Parameters<SearchParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let plan = devdocs_retrieve::Plan {
            limit: self.config.search.limit_default,
            max_tokens: params
                .0
                .max_tokens
                .unwrap_or(self.config.search.max_tokens_default),
            rrf_k: self.config.search.rrf_k,
            cursor: None,
        };
        let query = devdocs_retrieve::Query::new(
            &params.0.query,
            params.0.library.as_deref(),
            params.0.version.as_deref(),
        );
        // The MCP path never walks the host's lockfiles: the project
        // context stays empty (docs/implementation/08 §MCP adapter).
        let project = ProjectContext::empty();
        // The store is sync SQLite behind a Mutex: async code reaches it
        // only inside spawn_blocking
        // (.agents/instructions/async-and-performance.instruction.md).
        let store = Arc::clone(&self.store);
        let mut envelope = match tokio::task::spawn_blocking(move || {
            // The expiry sweep runs at the top of every public read path
            // (docs/implementation/07 §Expiry sweep); search_with_context
            // alone skips it, so the MCP search must sweep up front.
            store.sweep_expired(&devdocs_model::now_rfc3339())?;
            devdocs_retrieve::serve::search_with_context(&store, &query, &project, &plan)
        })
        .await
        {
            Ok(Ok(envelope)) => envelope,
            Ok(Err(err)) => return error::from_retrieve(err),
            // defensive: the block touches no unwinding state; the arm
            // keeps a panicked worker a protocol error, not a hang.
            Err(join) => return Err(error::internal(format!("the search task failed: {join}"))),
        };
        wiring::attribute_results(&self.config, &mut envelope);
        let json = params
            .0
            .detail
            .unwrap_or_default()
            .render(&envelope)
            .map_err(|e| error::internal(format!("cannot serialize the envelope: {e}")))?;
        error::success_result(json)
    }

    /// Reads one section by ref, bounded by the token budget.
    #[tool(
        name = "open_doc",
        description = "Read one section by ref. Refs come only from search_docs results. The result carries the section text bounded by the token budget.",
        // Not read-only: the read can record the expiry transition
        // (docs/implementation/07 §Expiry sweep).
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false,
        )
    )]
    pub async fn open_doc(
        &self,
        params: Parameters<OpenParams>,
    ) -> Result<CallToolResult, ErrorData> {
        let plan = devdocs_retrieve::Plan {
            max_tokens: params
                .0
                .max_tokens
                .unwrap_or(self.config.search.max_tokens_default),
            ..devdocs_retrieve::Plan::defaults()
        };
        // The store is sync SQLite behind a Mutex: async code reaches it
        // only inside spawn_blocking
        // (.agents/instructions/async-and-performance.instruction.md).
        let store = Arc::clone(&self.store);
        let doc_ref = params.0.doc_ref;
        let mut envelope = match tokio::task::spawn_blocking(move || {
            devdocs_retrieve::serve::open(&store, &doc_ref, &plan)
        })
        .await
        {
            Ok(Ok(envelope)) => envelope,
            Ok(Err(err)) => return error::from_retrieve(err),
            // defensive: the block touches no unwinding state; the arm
            // keeps a panicked worker a protocol error, not a hang.
            Err(join) => return Err(error::internal(format!("the open task failed: {join}"))),
        };
        wiring::attribute_results(&self.config, &mut envelope);
        let json = Detail::Full
            .render(&envelope)
            .map_err(|e| error::internal(format!("cannot serialize the envelope: {e}")))?;
        error::success_result(json)
    }

    /// Confirms a citation: re-derives the content hash and reports it.
    #[tool(
        name = "verify_doc",
        description = "Confirm a citation before you rely on it. Refs come only from search_docs results. Returns the valid flag and the content hash.",
        // Not read-only: the read can record the expiry transition
        // (docs/implementation/07 §Expiry sweep).
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false,
        )
    )]
    pub async fn verify_doc(
        &self,
        params: Parameters<VerifyParams>,
    ) -> Result<CallToolResult, ErrorData> {
        // The store is sync SQLite behind a Mutex: async code reaches it
        // only inside spawn_blocking
        // (.agents/instructions/async-and-performance.instruction.md).
        let store = Arc::clone(&self.store);
        let doc_ref = params.0.doc_ref;
        let mut envelope = match tokio::task::spawn_blocking(move || {
            devdocs_retrieve::serve::verify(&store, &doc_ref)
        })
        .await
        {
            Ok(Ok(envelope)) => envelope,
            Ok(Err(err)) => return error::from_retrieve(err),
            // defensive: the block touches no unwinding state; the arm
            // keeps a panicked worker a protocol error, not a hang.
            Err(join) => return Err(error::internal(format!("the verify task failed: {join}"))),
        };
        wiring::attribute_results(&self.config, &mut envelope);
        let Some(item) = envelope.results.first() else {
            // defensive: serve::verify always serves exactly one item on
            // the success path; the arm keeps the protocol error total.
            return Err(error::internal(String::from(
                "the verify result carried no item",
            )));
        };
        let json = serde_json::to_string(&VerifyReport {
            valid: true,
            content_hash: item.content_hash.clone(),
        })
        .map_err(|e| error::internal(format!("cannot serialize the report: {e}")))?;
        error::success_result(json)
    }
}

// The rmcp `tool_handler` macro generates `list_tools` and `get_tool`
// as async trait methods whose bodies never await, which trips
// `clippy::unused_async_trait_impl` inside the macro expansion; the
// generated shape is the macro's own, not ours to change.
#[allow(clippy::unused_async_trait_impl)]
#[tool_handler(router = self.tool_router)]
impl rmcp::ServerHandler for DevdocsServer {
    /// The server identity: tools only, no resources or prompts
    /// (docs/implementation/08 §MCP adapter).
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("devdocs", env!("CARGO_PKG_VERSION")))
            .with_instructions(SERVER_INSTRUCTIONS)
    }
}
