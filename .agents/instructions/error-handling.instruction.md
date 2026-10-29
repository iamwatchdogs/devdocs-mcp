# Error Handling

How errors are modeled, propagated, and surfaced.

- Library crates (`devdocs-model`, `devdocs-ingest`, `devdocs-store`, `devdocs-retrieve`, …) define structured errors with `thiserror` so callers can match on variants. The app edge (`devdocs` CLI, `devdocs-mcp`) maps them to user-facing outcomes: the CLI maps errors to documented exit codes 0–4 — extend that mapping table (docs/implementation/05 §Error model) when adding error kinds, and mind the clap exit-2→1 remap noted there.
- Attach `.context()` at boundaries so the failure chain reads as a narrative; guard invariants with types (newtypes over raw `String`/`u64`/paths) so bad input cannot reach fallible code.

## Panic policy

`unwrap_used` and `expect_used` are denied workspace-wide with no blanket allow sites: fallible paths return errors. A genuine panic site takes a targeted `#[allow]` with a written invariant comment — a design decision, not a shortcut. Tests opt out with `#![allow(clippy::unwrap_used, clippy::expect_used)]` at the top of test files only (docs/implementation/03 §Test-code exception). `missing_errors_doc` and `missing_panics_doc` are allowed, so public functions need no `# Errors` sections.

**Done means**: every fallible path returns an error that reaches the CLI edge as a documented exit code, and `just lint` passes without new allow sites.
