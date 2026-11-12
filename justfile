# Hooks, CI, and agent shells run without the user's profile: rustup installs
# cargo, rustup, and the binstall tools under ~/.cargo/bin, and pipx installs
# the lizard and opengrep scan tools under ~/.local/bin, so prepend both once
# for every recipe instead of per command.
home_dir := if env_var_or_default("HOME", "") != "" {
    env_var("HOME")
} else if env_var_or_default("USERPROFILE", "") != "" {
    env_var("USERPROFILE")
} else {
    error("HOME and USERPROFILE are not set")
}
export PATH := home_dir / ".cargo" / "bin" + ":" + home_dir / ".local" / "bin" + ":" + env_var_or_default("PATH", "")

set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Format the workspace.
fmt:
    cargo fmt --all

# Check formatting without writing.
fmt-check:
    cargo fmt --all --check

# Lint every target. Warnings are errors. --locked: CI tests the committed
# lockfile, not whatever cargo would resolve today.
lint:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# Lint and apply machine-applicable fixes.
lint-fix:
    cargo clippy --workspace --all-targets --fix --allow-dirty

# Run tests with nextest. A missing tool fails loudly everywhere: CI installs
# it explicitly, so a missing tool means the install step failed.
test:
    @if command -v cargo-nextest >/dev/null 2>&1; then \
        cargo nextest run --profile ci --workspace --all-targets --no-tests=pass --locked; \
    else \
        echo "cargo-nextest not installed: cargo binstall --no-confirm cargo-nextest"; \
        exit 1; \
    fi

# Run doc tests. (`--all-targets` would exclude them; unit tests run in `test`.)
test-doc:
    cargo test --workspace --doc --locked

# Run the #[ignore]d tests (network, embedding model) on demand. CI never
# runs them; see docs/implementation/04 §Ignored and slow tests.
test-slow:
    cargo test --workspace --all-targets --locked -- --ignored

# Build API docs. Warnings are errors.
docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

# License, advisory, ban, and source checks.
deny:
    @if command -v cargo-deny >/dev/null 2>&1; then \
        cargo deny check; \
    else \
        echo "cargo-deny not installed: cargo binstall --no-confirm cargo-deny"; \
        exit 1; \
    fi

# RustSec advisory scan.
audit:
    @if command -v cargo-audit >/dev/null 2>&1; then \
        cargo audit; \
    else \
        echo "cargo-audit not installed: cargo binstall --no-confirm cargo-audit"; \
        exit 1; \
    fi

# Codacy static analysis locally, against the main-branch diff. Mirrors the
# Opengrep patterns of the "Codacy Static Code Analysis" PR check (Codacy
# runs no clippy for Rust). Optional gate: skipped cleanly when the CLI or
# its repo config is absent, findings fail with exit 1. Not part of `just ci`
# and not a CI job — the cloud app already runs the same patterns on PRs
# (gap named per the agent-ready-repo contract).
codacy:
    @if ! command -v codacy-analysis >/dev/null 2>&1; then \
        echo "codacy-analysis not installed (optional gate, skipped): pnpm add -g @codacy/analysis-cli (fallback if PATH is not refreshed: pnpm exec codacy-analysis)"; \
    elif [ ! -f .codacy/codacy.config.json ]; then \
        echo "codacy-analysis not initialized (optional gate, skipped): codacy-analysis init --remote gh iamwatchdogs devdocs-mcp (refresh an existing snapshot after .codacy.yml changes: codacy-analysis update-config)"; \
    elif [ -z "$(git rev-parse -q --verify origin/main 2>/dev/null)" ]; then \
        echo "codacy gate skipped: origin/main not found to diff against"; \
    else \
        CODACY_DISABLE_UPDATE_CHECK=1 codacy-analysis analyze --no-log --tool Semgrep --diff origin/main; \
    fi

# Local SAST scan with the committed rule set in .config/opengrep-rules:
# no rules download, so the gate runs offline. Findings fail. Not part of
# `just ci` and not a CI job — CodeQL owns SAST in CI and the Codacy PR
# check runs the same engine over the diff (gap named per the
# agent-ready-repo contract).
opengrep:
    @if command -v opengrep >/dev/null 2>&1; then \
        opengrep scan --error --config .config/opengrep-rules .; \
    else \
        echo "opengrep not installed: curl -fsSL https://raw.githubusercontent.com/opengrep/opengrep/main/install.sh | bash"; \
        exit 1; \
    fi

# Filesystem scan: vulnerable Cargo.lock dependencies and hard-coded
# secrets. HIGH or CRITICAL findings fail. Not part of `just ci` and not
# a CI job — the dependency review workflow gates advisories on PRs (gap
# named per the agent-ready-repo contract).
trivy:
    @if command -v trivy >/dev/null 2>&1; then \
        trivy fs . --scanners vuln,secret --severity HIGH,CRITICAL --exit-code 1 --skip-dirs target; \
    else \
        echo "trivy not installed: brew install trivy"; \
        exit 1; \
    fi

# Complexity gate over the Rust crates. A violation fails. Limits start
# above the current worst function (CCN 24, 103 lines) so the gate starts
# green; tighten them as the outliers refactor. The Rust lexer merges
# functions that lack a separating blank line, so keep one blank line
# between test functions (crates/devdocs-store/src/license/tests/mod.rs
# does). Not part of `just ci` and not a CI job — Codacy lists Lizard as a
# non-gating complexity metric on PRs (gap named per the agent-ready-repo
# contract).
lizard:
    @if command -v lizard >/dev/null 2>&1; then \
        lizard crates -l rust -C 25 -L 150; \
    else \
        echo "lizard not installed: pipx install lizard"; \
        exit 1; \
    fi

# Copy-paste detection over the Rust crates. The .jscpd.json config owns
# the flags: rust format, 50-token minimum clone, tests excluded (fixture
# repetition is expected there), and a 3% clone-ratio threshold against a
# 1.5% baseline. A ratio above the threshold fails. Not part of `just ci`
# and not a CI job — Codacy runs jscpd as a non-gating metric on PRs (gap
# named per the agent-ready-repo contract).
jscpd:
    @if command -v jscpd >/dev/null 2>&1; then \
        jscpd crates; \
    else \
        echo "jscpd not installed: brew install jscpd (or npm i -g jscpd)"; \
        exit 1; \
    fi

# Unused dependency scan.
machete:
    @if command -v cargo-machete >/dev/null 2>&1; then \
        cargo machete; \
    else \
        echo "cargo-machete not installed: cargo binstall --no-confirm cargo-machete"; \
        exit 1; \
    fi

# Line coverage (lcov).
coverage:
    @if command -v cargo-llvm-cov >/dev/null 2>&1; then \
        cargo llvm-cov --workspace --lcov --locked --output-path lcov.info; \
    else \
        echo "cargo-llvm-cov not installed: cargo binstall --no-confirm cargo-llvm-cov"; \
        exit 1; \
    fi

# awk program for coverage-gate: sums LH/LF per docs/04 gated scope. Kept in
# an assignment so the recipe hands awk real newlines, not a joined one-liner.
coverage_gate_awk := '
    /^SF:/ {
        p = substr($0, 4)
        sub(/^.*\/crates\//, "crates/", p)
        if (p ~ /^crates\/devdocs-store\/src\//) {
            s = "devdocs-store"
        } else if (p ~ /^crates\/devdocs-retrieve\/src\//) {
            s = "devdocs-retrieve"
        } else if (p == "crates/devdocs-ingest/src/guard.rs") {
            s = "ingest-guard"
        } else {
            s = ""
        }
    }
    /^LF:/ && s != "" { lf[s] += substr($0, 4) }
    /^LH:/ && s != "" { lh[s] += substr($0, 4) }
    END {
        split("devdocs-store devdocs-retrieve ingest-guard", names, " ")
        fail = 0
        for (i = 1; i <= 3; i++) {
            name = names[i]
            if (lf[name] == 0) {
                printf "coverage-gate: FAIL %s — no LF records in lcov.info (did the scope paths move?)\n", name
                fail = 1
                continue
            }
            pct = 100 * lh[name] / lf[name]
            printf "coverage-gate: %-14s %5.1f%% (floor 80%%)\n", name, pct
            if (pct < 80) {
                printf "coverage-gate: FAIL %s at %.1f%% is below the 80%% floor (docs/implementation/04 §Coverage)\n", name, pct
                fail = 1
            }
        }
        if (fail) exit 1
    }
'

# Enforce the docs/implementation/04 §Coverage floors against the lcov.info
# that `just coverage` produced: 80% line coverage on devdocs-store,
# devdocs-retrieve, and the devdocs-ingest guard module. Parses the existing
# report — no rebuild. Glue code and the CLI layer carry no floor.
coverage-gate:
    @if [ ! -f lcov.info ]; then \
        echo "lcov.info not found at the repo root: run 'just coverage' first"; \
        exit 1; \
    fi
    @awk '{{coverage_gate_awk}}' lcov.info

# Check the workspace against the MSRV toolchain.
msrv:
    @if rustup toolchain list | grep -q '^1\.90'; then \
        cargo +1.90.0 check --workspace --all-targets --locked; \
    else \
        echo "MSRV toolchain 1.90.0 missing: rustup toolchain install 1.90.0"; \
        exit 1; \
    fi

# Compile the binary and the ingest crate in every feature state CI checks.
# The ingest crate is checked, not built: only its feature resolution is
# at stake, and the grammar-less state must stay compilable.
features:
    cargo build --locked -p devdocs --no-default-features
    cargo build --locked -p devdocs --no-default-features --features exact-tokens
    cargo build --locked -p devdocs --no-default-features --features embeddings
    cargo check --locked -p devdocs-ingest --no-default-features
    cargo check --locked -p devdocs-ingest --features symbols

# Run the default eval suite and print the gate table (docs/implementation/14).
# A gate that is not PASS exits 2 — the honest state until the retrieval
# core lands at M4.
eval:
    cargo build --locked -p devdocs
    cargo run -p devdocs-eval -- run pilot

# Public API break detection between releases.
semver:
    @if command -v cargo-semver-checks >/dev/null 2>&1; then \
        cargo semver-checks --release; \
    else \
        echo "cargo-semver-checks not installed: cargo binstall --no-confirm cargo-semver-checks"; \
        exit 1; \
    fi

# Mutation testing.
mutants:
    @if command -v cargo-mutants >/dev/null 2>&1; then \
        cargo mutants --workspace --exclude devdocs-testkit; \
    else \
        echo "cargo-mutants not installed: cargo binstall --no-confirm cargo-mutants"; \
        exit 1; \
    fi

# Benchmarks.
bench:
    cargo bench --workspace

# Full local gate. Mirrors CI.
ci:
    @just fmt-check
    @just lint
    @just test
    @just test-doc
    @just docs
    @just deny
    @just features
