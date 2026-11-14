# Phase 1 demo — the MCP spot-check

Date: 2026-11-13. Repo state: `5291fdc`. The spot-check runs the same
query as the CLI demo (demo-cli.md) through the three-tool MCP server and
compares the returned refs against the CLI results. The server is the
stdio JSON-RPC process the M8 contract tests exercise
(`crates/devdocs/tests/mcp_stdio.rs` spawns it, and
`crates/devdocs/tests/mcp_contract.rs` holds the systematic
identical-results gate over a 50-query corpus).

## Setup

Start the server (it stays up for the transcript). `DEVDOCS_HOME` points
at the corpus home the `just eval-corpus` recipe built):

```console
$ devdocs --db eval/runs/corpus.db --config eval/corpus.toml mcp --transport stdio
```

The protocol is newline-delimited JSON-RPC 2.0 on stdout: `initialize`,
the `notifications/initialized` confirmation, then `tools/call` — the
same shape the stdio tests drive (crates/devdocs/tests/mcp_stdio.rs
§`Session::handshake`).

## Transcript

### 1. Handshake

```text
--> {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"mcp-demo","version":"0.0.0"}}}
<-- {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{"name":"devdocs","version":"0.1.0"}}}
--> {"jsonrpc":"2.0","method":"notifications/initialized"}
```

### 2. The tool list carries the three tools

```text
--> {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
<-- {"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"open_doc",...},{"name":"search_docs",...},{"name":"verify_doc",...}]}}
```

Observed names, in order: `open_doc`, `search_docs`, `verify_doc`.

### 3. search_docs with the CLI demo query

```text
--> {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search_docs","arguments":{"query":"proxy rewrite","library":"next.js","version":"16.2.9"}}}
```

The tool result is one text content block holding the result envelope
JSON (the same envelope the CLI prints). The returned refs, in order:

```text
docs://next.js/v16.2.9/v/rewrite
docs://next.js/v16.2.9/v/rewrite
docs://next.js/v16.2.9/r/rewrite
docs://next.js/v16.2.9/api-reference
docs://next.js/v16.2.9/v/how-to-route-requests-to-the-right-zone/routing-requests-using-proxy#proxy
docs://next.js/v16.2.9/v/how-to-route-requests-to-the-right-zone/routing-requests-using-proxy#proxy
docs://next.js/v16.2.9/examples/conditional-statements#proxy
docs://next.js/v16.2.9/examples/conditional-statements#proxy
```

## Comparison

The CLI demo ran the identical query
(`--json search "proxy rewrite" --library next.js --version 16.2.9`
against the same database) and returned the ref list above. The
comparison script extracts `results[].ref` from both sides and compares
them element-wise:

**Result: same refs, same order.** The two lists are identical, so the
spot-check passes. The systematic backing for this identity is the M8
identical-results gate, `crates/devdocs/tests/mcp_contract.rs`: the
spawned CLI and the in-process MCP server serve byte-identical JSON
envelopes for the same query against the same index, minus the
`served_at` wall-clock field, across a 50-query corpus. The transcript
here is the manual spot-check that gate requires for the Phase 1 demo.

## Closing the server

```console
<EOF on stdin — the server exits>
```
