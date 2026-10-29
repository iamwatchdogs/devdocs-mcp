# Async & Blocking

The concurrency contract for Tokio + blocking SQLite.

- All rusqlite access must go through `devdocs_store::Store`, which wraps a `Mutex<Connection>` and runs inside `tokio::task::spawn_blocking` (docs/implementation/05 §Concurrency model). Touch rusqlite nowhere else; reach store methods from async code in `devdocs`, `devdocs-mcp`, or `devdocs-retrieve` only via `spawn_blocking`; release the connection guard before every await.
- CPU-heavy work (parsing, chunking, embedding) must also run in `spawn_blocking`, keeping the executor free.
- Cancellation safety: the rmcp stdio transport and the `ctrlc` handler drop awaits mid-flight, so every `select!`/timeout arm must tolerate cancellation — no half-committed state; cleanup belongs in `Drop`.

**Done means**: a search finds no rusqlite outside `devdocs-store`, no `MutexGuard` held across an await, and every cancellation point leaves state consistent.
