# devdocs-mcp

Local, read-only documentation server for coding agents, exposed over MCP with a SQLite-backed index and hybrid retrieval.

> **Status: scaffold (M0).** Crate sources are stubs; the clap CLI, config commands, and eval harness land in later milestones (docs/implementation/12). See "Scaffold deviations" for where the repo intentionally differs from the docs.

## Prerequisites

| Tool | Version | Install |
|---|---|---|
| rustup | latest | <https://rustup.rs> |
| Rust | 1.99.1 (pinned by `rust-toolchain.toml`) | `rustup toolchain install 1.99.1` |
| git | 2.40+ | system package |
| just | 1.59.0 | `brew install just` |
| lefthook | 2.2.2 | `brew install lefthook` |
| cargo-binstall | 1.26.1 | `brew install cargo-binstall` |

Dev tools used by optional recipes:

```sh
cargo binstall --no-confirm cargo-nextest cargo-deny cargo-audit \
  cargo-machete cargo-llvm-cov cargo-mutants cargo-semver-checks
```

## Install

Build from source. The binary lands at `target/release/devdocs`.

```sh
cargo build --release --locked
```

Install into `~/.cargo/bin`:

```sh
cargo install --path crates/devdocs --locked
```

Activate git hooks once per clone:

```sh
lefthook install
```

## Quickstart

```sh
devdocs --version
```

Everything beyond the version banner lands in later milestones (docs/implementation/12). Crate-level docs: `just docs`.

## Workspace layout

| Path | Role |
|---|---|
| `crates/devdocs-model` | Core types, config, errors. No workspace dependencies |
| `crates/devdocs-store` | SQLite schema, migrations, queries. Owns `fastembed` behind `embeddings` |
| `crates/devdocs-ingest` | Source adapters, fetch guard, normalization, symbols |
| `crates/devdocs-retrieve` | Hybrid search, fusion, budget packing |
| `crates/devdocs-mcp` | MCP server adapter over `devdocs-retrieve` |
| `crates/devdocs` | `devdocs` binary: CLI plus MCP entry point |
| `crates/devdocs-eval` | Golden-set evaluation harness |
| `crates/devdocs-testkit` | Shared test helpers and fixtures |
| `migrations/` | Embedded SQL migrations |
| `eval/golden/` | Golden question files (JSONL) |
| `docs/` | Implementation guide and research bundle |

## Dev commands

| Command | What it does |
|---|---|
| `just` | List recipes |
| `just fmt` | `cargo fmt --all` |
| `just fmt-check` | Check formatting without writing |
| `just lint` | Clippy, all targets, warnings denied |
| `just lint-fix` | Clippy with machine-applicable fixes |
| `just test` | `cargo nextest run --workspace --all-targets` |
| `just test-doc` | `cargo test --workspace --all-targets` (includes doc tests) |
| `just docs` | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` |
| `just deny` | Licenses, advisories, bans, sources (`deny.toml`) |
| `just audit` | RustSec advisory scan |
| `just machete` | Unused dependency scan |
| `just coverage` | `cargo llvm-cov` to `lcov.info` |
| `just msrv` | `cargo +1.90.0 check --workspace --all-targets --locked` |
| `just features` | Build `devdocs` with no features, `exact-tokens`, `embeddings` |
| `just eval` | Eval harness smoke run |
| `just semver` | Public API break detection |
| `just mutants` | Mutation testing |
| `just bench` | `cargo bench --workspace` |
| `just ci` | `fmt-check` + `lint` + `test` + `docs` + `deny` + `features` |

Optional recipes print an install hint and exit non-zero when the external tool is missing. Inside `just ci`, `test` falls back to `cargo test` and `deny` falls back to `cargo audit` (or skips with a message) so local CI stays green without every tool.

## Scaffold deviations

| Doc says | Repo does | Why |
|---|---|---|
| Toolchain/MSRV 1.85 | Toolchain 1.99.1, workspace `rust-version = 1.90` | Pinned compiler follows dependency MSRVs. rusqlite 0.41 needs 1.89 and text-splitter 0.33 needs 1.90, so 1.90.0 is the first passing MSRV checked by `just msrv` |
| `resolver = "3"` | `resolver = "2"` | Block 1 chose resolver 2; edition 2024 crates still compile. Revisit when the workspace moves to resolver 3 |
| `fastembed` optional in `crates/devdocs` | Optional in `devdocs-store` behind `embeddings` | The store owns vector tables and embedding storage; the binary feature forwards to it (target state; the binary flag is currently an empty no-op — see the `exact-tokens` / `embeddings` row below) |
| `text-splitter` always enables `tiktoken-rs` | `tiktoken-rs` gated behind `exact-tokens` | Keeps the default build small; tokenizer counts are approximate unless the feature is on |
| `exact-tokens` / `embeddings` forward to the store and ingest crates | Empty no-op flags on `crates/devdocs` | Keeps `just features` green before the store and ingest layers land; repoint when they do (docs/implementation/12) |
| Structured logs via `tracing` and `tracing-subscriber` | `Reporter` writes plain lines to stderr under `--verbose` | The subscriber crate is not in the lock yet; add it when the ingest layer emits structured events |
| Help groups subcommands under headings | Only `Global options` gets a heading | clap 4.6.7 `next_help_heading` applies to arguments, not subcommands |
| `MIT OR Apache-2.0` with `LICENSE-MIT` and `LICENSE-APACHE` | `license = "MIT"`, one `LICENSE` file | The scaffold committed a single MIT license; the dual-license path was never set up. Decide at M9 (packaging): add the Apache text or update docs/02 |
| docs/11 release matrix ships `aarch64-pc-windows-msvc` | `release.yml` excludes it | docs/01 rules Windows ARM64 out of scope for Phase 1; the release matrix follows docs/01. docs/11 needs the fix |
| docs/04 names a `just test-slow` recipe | No such recipe; `just test-doc` runs `cargo test` | No `#[ignore]` tests exist yet; the recipe lands with the first one |
| docs/14 says `just eval` runs the default suite | `just eval` runs `--help` (smoke only) | The eval harness lands at M10 |

## License

MIT. See `LICENSE`.
