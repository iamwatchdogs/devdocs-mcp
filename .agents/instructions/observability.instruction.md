# Logging & Stream Discipline

What may be written where, in a stdio MCP server.

- **stdout is a protocol channel, never a log sink.** For `devdocs-mcp`, stdout carries the MCP stream (rmcp transport-io) — one stray diagnostic corrupts every connected client session. For the CLI, stdout carries only the result envelope, including `--json` output. All logs, progress, and diagnostics go to stderr.
- Use `tracing` with structured fields; the subscriber (`tracing-subscriber` with `env-filter`) initializes in the `devdocs` binary.
- Log provenance, not content: sizes, counts, hashes, and URLs are fine; fetched document bodies, chunk text, and user queries are not (queries surface only behind `--verbose`, locally).
- No telemetry leaves the machine in v1 (docs/implementation/09).

**Done means**: `--json` output stays byte-clean of log lines, and an MCP session survives a log-heavy run.
