set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Format the workspace.
fmt:
    cargo fmt --all

# Check formatting without writing.
fmt-check:
    cargo fmt --all --check

# Lint every target. Warnings are errors.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Lint and apply machine-applicable fixes.
lint-fix:
    cargo clippy --workspace --all-targets --fix --allow-dirty

# Run tests with nextest. In CI, fall back to cargo test when nextest is missing.
test:
    @if command -v cargo-nextest >/dev/null 2>&1; then \
        cargo nextest run --profile ci --workspace --all-targets --no-tests=pass; \
    elif [ "${DEVDOCS_CI:-0}" = "1" ]; then \
        echo "cargo-nextest missing; ci fallback: cargo test"; \
        cargo test --workspace --all-targets; \
    else \
        echo "cargo-nextest not installed: cargo binstall --no-confirm cargo-nextest"; \
        exit 1; \
    fi

# Run unit and doc tests with cargo test (doc tests included).
test-doc:
    cargo test --workspace --all-targets

# Build API docs. Warnings are errors.
docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

# License, advisory, ban, and source checks.
deny:
    @if command -v cargo-deny >/dev/null 2>&1; then \
        cargo deny check; \
    elif [ "${DEVDOCS_CI:-0}" = "1" ]; then \
        echo "cargo-deny missing; ci fallback: cargo audit"; \
        if command -v cargo-audit >/dev/null 2>&1; then cargo audit; \
        else echo "cargo-audit also missing; ci fallback: skipping dependency audit"; fi; \
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
        cargo llvm-cov --workspace --lcov --output-path lcov.info; \
    else \
        echo "cargo-llvm-cov not installed: cargo binstall --no-confirm cargo-llvm-cov"; \
        exit 1; \
    fi

# Check the workspace against the MSRV toolchain.
msrv:
    @if rustup toolchain list | grep -q '^1\.90'; then \
        cargo +1.90.0 check --workspace --all-targets --locked; \
    else \
        echo "MSRV toolchain 1.90.0 missing: rustup toolchain install 1.90.0"; \
        exit 1; \
    fi

# Compile the binary in every feature state CI checks.
features:
    cargo build -p devdocs --no-default-features
    cargo build -p devdocs --no-default-features --features exact-tokens
    cargo build -p devdocs --no-default-features --features embeddings

# Harmless eval harness smoke run (runner lands later).
eval:
    cargo run -p devdocs-eval -- --help

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
    @DEVDOCS_CI=1 just test
    @just docs
    @DEVDOCS_CI=1 just deny
    @just features
