# Workspace & Crate Boundaries

Where code belongs and how crates relate.

## Crate map

`crates/*` is the boundary map, auto-included by the workspace glob — a new crate directory becomes a member with no manifest edits:

- `devdocs` — CLI/MCP binary (the app edge); `devdocs-mcp` — MCP server.
- `devdocs-model` — shared types; `devdocs-ingest` — fetch/parse/chunk; `devdocs-store` — SQLite; `devdocs-retrieve` — hybrid search; `devdocs-eval` — evaluation harness; `devdocs-testkit` — test fixtures (never published); `context-chunker`, `context-file` — content helpers.

## Rules

- New code joins an existing crate. A new crate needs a stated external boundary (new adapter, new capability) or a measured compile-time win; internal plumbing stays as modules. With 10 crates of skeleton code, the live failure mode here is over-splitting.
- Two binding crate-graph rules (docs/implementation/05 §Crate graph): `devdocs-model` depends on no workspace crate, and `devdocs-retrieve` never reads through the ingest code.
- Internal crates are declared in `[workspace.dependencies]` with **both** `path` and `version` so the workspace publishes in dependency order (release-plz single-version mode — all crates share one version).
- Dependency versions and lints resolve from `[workspace.dependencies]` / `[workspace.lints]` at the root; never restate them in a member crate.

**Done means**: the code sits in a crate whose responsibility fits one sentence, and both crate-graph rules still hold.
