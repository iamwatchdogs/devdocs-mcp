---
type: guide
title: Project Scaffolding
description: The exact commands, file tree, Cargo workspace, and config files that create the devdocs repository. Run the steps in order from an empty directory.
tags: [devdocs, scaffolding, cargo, workspace]
timestamp: 2026-10-28T00:00:00Z
resource: docs
derives-from: [docs/research]
---

# Project Scaffolding

## Prerequisites

Install these tools before the scaffold step. Verify each with the version command in the table.

| Tool | Version floor | Install | Verify |
|---|---|---|---|
| rustup | latest | https://rustup.rs | `rustup --version` |
| Rust stable | 1.99.1 | `rustup toolchain install 1.99.1` | `rustc --version` |
| git | 2.40 or later | system package | `git --version` |
| just | 1.59.0 | `cargo binstall just` or `brew install just` | `just --version` |
| lefthook | 2.2.2 | `cargo binstall lefthook` or `brew install lefthook` | `lefthook version` |
| cargo-binstall | 1.26.1 | `cargo install cargo-binstall --locked` | `cargo binstall -V` |

`cargo binstall` installs prebuilt binaries. It avoids long compile times for the dev tools.

## Scaffold commands

Run these commands from the parent directory of the repository. The repository root already contains `docs/research`. The commands create the Cargo workspace around it.

```sh
git init -b main devdocs-mcp
cd devdocs-mcp

cargo new --lib crates/devdocs-model
cargo new --lib crates/devdocs-store
cargo new --lib crates/devdocs-ingest
cargo new --lib crates/devdocs-retrieve
cargo new --lib crates/devdocs-mcp
cargo new --lib crates/devdocs-testkit
cargo new --bin crates/devdocs
cargo new --bin crates/devdocs-eval

mkdir -p eval/golden eval/runs migrations .github/workflows
```

The first `cargo build` generates `Cargo.lock`. Commit `Cargo.lock`. This is a binary workspace, so the lock file is part of the release process.

## File tree

This tree is the target state at milestone M1. The `docs/` subtree exists already.

```text
devdocs-mcp/
├── Cargo.toml                 # workspace root
├── Cargo.lock                 # committed
├── rust-toolchain.toml        # pins the dev toolchain
├── rustfmt.toml
├── deny.toml                  # cargo-deny config
├── lefthook.yml               # git hooks config
├── justfile                   # task runner
├── .gitignore
├── .github/
│   └── workflows/
│       ├── ci.yml
│       └── release.yml
├── LICENSE-APACHE
├── LICENSE-MIT
├── README.md
├── docs/
│   ├── 00-implementation-guide.md   # this guide
│   ├── 01 .. 14-*.md
│   └── research/                    # evidence bundle (read-only inputs in raw/)
├── eval/
│   ├── golden/                      # golden question files (JSONL)
│   └── runs/                        # eval run artifacts (gitignored)
├── migrations/                      # embedded SQL migrations (devdocs-store)
│   └── 0001_init.sql
└── crates/
    ├── devdocs-model/       # types, errors, config, license states
    ├── devdocs-store/       # SQLite schema, migrations, queries
    ├── devdocs-ingest/      # L1+L2: adapters, fetch guard, normalize, symbols
    ├── devdocs-retrieve/    # L3: hybrid search, fusion, budgets
├── devdocs-mcp/         # L4: rmcp server adapter
├── devdocs/             # binary crate named `devdocs` (the CLI)
├── devdocs-eval/        # eval harness binary
└── devdocs-testkit/     # shared test helpers and fixtures
```

Note. `cargo new --bin crates/devdocs` creates a crate named `devdocs`. The binary is `devdocs`. The tree entry above shows the alias for clarity only.

## Workspace root Cargo.toml

Create this file at the repository root. Workspace dependencies are declared once here. Member crates reference them with `workspace = true`.

```toml
[workspace]
resolver = "3"
members = [
    "crates/devdocs",
    "crates/devdocs-model",
    "crates/devdocs-store",
    "crates/devdocs-ingest",
    "crates/devdocs-retrieve",
    "crates/devdocs-mcp",
    "crates/devdocs-eval",
    "crates/devdocs-testkit",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.90"
license = "MIT"
repository = "https://github.com/OWNER/devdocs-mcp"

[workspace.dependencies]
# runtime
tokio = { version = "1.54.1", features = ["rt-multi-thread", "macros", "fs", "net", "time", "signal", "sync"] }
reqwest = { version = "0.13.5", features = ["gzip"] }
# storage
rusqlite = { version = "0.41.0", features = ["bundled"] }
# cli + serialization
clap = { version = "4.6.7", features = ["derive"] }
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.154"
toml = "0.9"
serde_yaml_ng = "0.10.0"
# errors + logging
thiserror = "2.0.21"
anyhow = "1.0.104"
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }
# content processing
comrak = "0.59.0"
text-splitter = { version = "0.33.1", features = ["markdown", "tiktoken-rs"] }
dom_smoothie = "0.19.0"
rust-trafilatura = "2.2.8"
oas3 = "0.22.0"
tree-sitter = "0.25.8"
tree-sitter-rust = "0.24.2"
tree-sitter-go = "0.25.0"
tree-sitter-python = "0.25.0"
tree-sitter-javascript = "0.25.0"
# mcp
rmcp = { version = "3.8.0", features = ["server", "transport-io", "macros", "schemars"] }
# web + hashing + fs
url = "2.5.8"
time = { version = "0.3", features = ["serde", "formatting", "parsing", "macros"] }
sha2 = "0.11.1"
blake3 = "1.8.7"
regex = "1.13.1"
dirs = "7.0.0"
tempfile = "3.27.0"
walkdir = "2.5.0"
ignore = "0.4.34"
# token counting (optional, exact mode)
tiktoken-rs = { version = "0.12.1", optional = true }
# embeddings (optional, phase 3)
fastembed = { version = "7.5.0", optional = true }
# dev
insta = { version = "1.49.0", features = ["json", "redactions"] }
proptest = "1.12.0"
criterion = "0.8.2"
assert_cmd = "2.2.2"
predicates = "3.1.4"
wiremock = "0.6.5"

[workspace.lints.rust]
unsafe_code = "deny"
missing_docs = "warn"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
dbg_macro = "deny"
todo = "warn"
module_name_repetitions = "allow"
missing_errors_doc = "allow"
missing_panics_doc = "allow"
multiple_crate_versions = "allow"

[profile.release]
lto = "thin"
strip = "symbols"
codegen-units = 1
```

Note on `toml = "0.9"`. The `toml` crate was the one dependency family member not individually re-verified on 2026-10-28. `cargo update` will resolve the latest compatible 0.9.x. Confirm the version at scaffold time with `cargo add toml --dry-run`.

Note on `resolver = "3"`. Edition 2024 sets resolver 3 as the default for the workspace when declared in `edition`. The explicit line keeps the intent visible.

## Member crate template

Each library crate has this `Cargo.toml` shape. Replace `NAME` and the dependency list per crate.

```toml
[package]
name = "devdocs-model"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "Core types, configuration, and errors for devdocs"

[dependencies]
serde.workspace = true
thiserror.workspace = true

[dev-dependencies]
insta.workspace = true

[lints]
workspace = true
```

The `[lints] workspace = true` line applies the workspace lint table to every crate. Do not restate lints per crate.

## Optional features

Cargo declares features per crate, not per workspace. The two optional dependency groups live in the binary crate `crates/devdocs/Cargo.toml`:

```toml
[features]
default = []
exact-tokens = ["dep:tiktoken-rs"]
embeddings = ["dep:fastembed"]

[dependencies]
tiktoken-rs = { workspace = true, optional = true }
fastembed = { workspace = true, optional = true }
```

`embeddings` also gates the vector tables in `devdocs-store` with the same feature name. A feature-gated build must still compile with `--no-default-features`. CI checks both states.

## Crate dependency rules

The crate graph is acyclic. Enforce it in review with this table.

| Crate | Depends on | Must not depend on |
|---|---|---|
| devdocs-model | (external only) | any workspace crate |
| devdocs-store | model | ingest, retrieve, cli |
| devdocs-ingest | model, store | retrieve, mcp, cli |
| devdocs-retrieve | model, store | ingest, mcp, cli |
| devdocs-mcp | model, retrieve | store internals, ingest |
| devdocs | model, store, ingest, retrieve, mcp | (binary. May depend on all) |
| devdocs-eval | model, store, retrieve | ingest internals, mcp |
| devdocs-testkit | model, store | (dev-dependency only) |

## rust-toolchain.toml

```toml
[toolchain]
channel = "1.99.1"
components = ["rustfmt", "clippy"]
```

This file pins the developer and CI toolchain. The workspace MSRV stays at 1.90 in `rust-version`. Dependency MSRVs forced the floor up from the planned 1.85: rusqlite 0.41 needs 1.89 and text-splitter 0.33 needs 1.90, so 1.90.0 is the first MSRV that `just msrv` passes. See [03](03-code-quality-toolchain.md) for the MSRV policy.

## deny.toml

This is the complete `cargo-deny` 0.20 configuration. Cargo-deny 0.16 and later reject the legacy config fields, so this uses the version-2 style throughout.

```toml
[graph]
all-features = false
no-default-features = false

[licenses]
version = 2
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Zlib",
    "CC0-1.0",
    "MPL-2.0",
]
# aws-lc-rs (reqwest 0.13 default crypto) is Apache-2.0 OR ISC. No OpenSSL entry needed.
confidence-threshold = 0.93

[advisories]
version = 2
yanked = "deny"
ignore = { }

[bans]
multiple-versions = "warn"
wildcards = "deny"
highlight = "all"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
allow-git = []
```

## .gitignore

```text
/target
/eval/runs/
*.db
*.db-wal
*.db-shm
.DS_Store
```

## justfile

```just
default:
    @just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

lint:
    cargo clippy --workspace --all-targets -- -D warnings

lint-fix:
    cargo clippy --workspace --all-targets --fix --allow-dirty

test:
    cargo nextest run --workspace --all-targets

test-slow:
    cargo test --workspace --all-targets   # doc tests and ignored tests

cov:
    cargo llvm-cov --workspace --lcov --output-path lcov.info

deny:
    cargo deny check

audit:
    cargo audit

machete:
    cargo machete

semver:
    cargo semver-checks --release

mutants:
    cargo mutants --workspace --exclude devdocs-testkit

docs:
    cargo doc --workspace --no-deps -D warnings

bench:
    cargo bench --workspace

ci: fmt-check lint test deny
```

## lefthook.yml

```yaml
pre-commit:
  commands:
    fmt:
      run: cargo fmt --all --check
    clippy:
      run: cargo clippy --workspace --all-targets -- -D warnings

commit-msg:
  commands:
    conventional:
      run: |
        head -1 {1} | grep -qE \
        '^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\([a-z0-9-]+\))?: .{1,72}$'
```

Activate the hooks after the first commit:

```sh
lefthook install
```

The hook block above uses a shell one-liner. Keep the regex simple. A failed match blocks the commit.

## Licenses and README

- Add `LICENSE-MIT` and `LICENSE-APACHE` with the standard texts. The workspace `license` field is `MIT OR Apache-2.0`.
- `README.md` starts as a stub: one paragraph, one install line, one example command. Expand it at milestone M9 (release).
- Add `CONTRIBUTING.md` at milestone M9. It records the PR checklist from [03](03-code-quality-toolchain.md).

## Commit and branch policy

- Branch name: short lowercase words with hyphens, for example `feat-fts5-search`.
- Commit messages follow Conventional Commits: `type(scope): summary`. The `commit-msg` hook enforces the shape.
- One logical change per commit. `git commit --fixup` plus `git rebase --autosquash` cleans a branch before review.
- The default branch is `main`. Tag releases as `v0.x.y`.
