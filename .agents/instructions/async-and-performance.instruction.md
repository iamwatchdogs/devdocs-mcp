# Async & Blocking

The concurrency contract for Tokio + blocking SQLite.

- All rusqlite access goes through `devdocs_store::Store`, which wraps a `Mutex<Connection>` and runs inside `tokio::task::spawn_blocking` (docs/implementation/05 §Concurrency model). Never touch rusqlite outside the store crate, never call store methods directly from async functions in `devdocs`, `devdocs-mcp`, or `devdocs-retrieve`, and never hold a `Connection` across an await.
- CPU-heavy work (parsing, chunking, embedding) also runs in `spawn_blocking`, keeping the executor free.
- Write cancellation-safe async: the rmcp stdio transport and the `ctrlc` handler drop awaits mid-flight, so every `select!`/timeout arm must tolerate cancellation — no half-committed state, cleanup in `Drop` where needed.

**Done means**: a search finds no rusqlite outside `devdocs-store`, no store call on the executor without `spawn_blocking`, and every cancellation point leaves state consistent.
