# Logging & Stream Discipline

What may be written where, in a stdio MCP server.

- **stdout is a protocol channel, never a log sink.** For `devdocs-mcp`, stdout carries the MCP stream (rmcp transport-io) — one stray diagnostic corrupts every connected client session. For the CLI, stdout carries only the result envelope, including `--json` output. All logs, progress, and diagnostics go to stderr.
- Use `tracing` with structured fields. The subscriber (`tracing-subscriber` with `env-filter`) initializes in the `devdocs` binary; until the ingest layer emits structured events, plain stderr lines under `--verbose` are the accepted stand-in (README deviations).
- Log provenance, not content: sizes, counts, hashes, and URLs are fine; fetched document bodies, chunk text, and user queries are not (queries surface only behind `--verbose`, locally).
- No telemetry leaves the machine in v1 (docs/implementation/09).
- Under `--json`, errors print to **stderr** as the structured envelope. docs/implementation/05 §Error model owns the error shape; docs/implementation/08 §CLI contract owns stdout contents. When another document proposes stdout for errors, this file is the tiebreaker: stderr wins, and the proposer is corrected in the same change.

**Done means**: `--json` output stays byte-clean of log lines, and an MCP session survives a log-heavy run.
