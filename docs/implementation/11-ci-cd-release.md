---
type: guide
title: CI/CD and Release
description: The GitHub Actions workflows, the release build matrix, the crates.io publish path with trusted publishing, and the versioning policy. Action versions verified 2026-10-28.
tags: [devdocs, ci, github-actions, release, crates-io]
timestamp: 2026-10-28T00:00:00Z
resource: docs
derives-from: [docs/research]
---

# CI/CD and Release

## Workflows

Two workflows: `ci.yml` on every push and pull request, `release.yml` on tags and a monthly publish schedule. Action versions verified 2026-10-28: `actions/checkout` v5 line (v6 current), `Swatinem/rust-cache` v2 (2.9.3), `taiki-e/install-action` v2, `cargo-deny-action` v2, `dtolnay/rust-toolchain` v1, `codecov-action` v5. Pin exact tags at scaffold time and let Renovate digest-pin them.

## ci.yml

```yaml
name: ci
on:
  push:
    branches: [main]
  pull_request:

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always
  INSTA_FORCE_UPDATE: "0"

jobs:
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@1.99.1
      - run: cargo fmt --all --check

  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@1.99.1
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace --all-targets -- -D warnings

  test:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@1.99.1
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@v2
        with:
          tool: cargo-nextest
      - run: cargo nextest run --workspace --all-targets

  msrv:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@1.85.0
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --workspace --all-targets --locked

  features:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        flags: ["--no-default-features", "--features exact-tokens", "--features embeddings"]
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@1.99.1
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --workspace --all-targets ${{ matrix.flags }}

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: EmbarkStudios/cargo-deny-action@v2

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@1.99.1
        with:
          components: llvm-tools-preview
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@v2
        with:
          tool: cargo-llvm-cov
      - run: cargo llvm-cov --workspace --lcov --output-path lcov.info
      - uses: codecov/codecov-action@v5
        with:
          files: lcov.info

  docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@1.99.1
      - uses: Swatinem/rust-cache@v2
      - run: cargo doc --workspace --no-deps -D warnings
```

Notes:

- The `msrv` job uses `--locked` so it validates the real lock file against toolchain 1.85.
- The `embeddings` feature job compiles the ort backend. Keep the cache warm to bound its cost.
- A scheduled workflow (weekly) runs `cargo audit` and `cargo machete` and opens an issue on findings.

## release.yml (binaries)

The release builds use native runners for macOS and Windows and `cargo-zigbuild` (0.24.0, verified active 2026-10-28) for Linux, including musl and arm64. `cross-rs` has not tagged a release since 2023, so it is not used. Cargo-dist exists but moves slowly. The hand-rolled matrix is easier to audit for this project.

```yaml
name: release
on:
  push:
    tags: ["v*.*.*"]

permissions:
  contents: write

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - { target: x86_64-apple-darwin,        os: macos-latest }
          - { target: aarch64-apple-darwin,       os: macos-latest }
          - { target: x86_64-pc-windows-msvc,     os: windows-latest }
          - { target: aarch64-pc-windows-msvc,    os: windows-latest }
          - { target: x86_64-unknown-linux-gnu,   os: ubuntu-latest,  zig: true }
          - { target: aarch64-unknown-linux-gnu,  os: ubuntu-latest,  zig: true }
          - { target: x86_64-unknown-linux-musl,  os: ubuntu-latest,  zig: true }
          - { target: aarch64-unknown-linux-musl, os: ubuntu-latest,  zig: true }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v5
      - uses: dtolnay/rust-toolchain@1.99.1
        with:
          targets: ${{ matrix.target }}
      - uses: Swatinem/rust-cache@v2
      - name: Install zigbuild
        if: matrix.zig
        run: pip install cargo-zigbuild
      - name: Build (zig)
        if: matrix.zig
        run: cargo zigbuild --release --target ${{ matrix.target }}
      - name: Build (native)
        if: "!matrix.zig"
        run: cargo build --release --target ${{ matrix.target }}
      - name: Stage
        shell: bash
        run: |
          out="devdocs-${{ github.ref_name }}-${{ matrix.target }}"
          mkdir -p "dist/${out}"
          cp "target/${{ matrix.target }}/release/devdocs"[.exe]* "dist/${out}/" 2>/dev/null || \
            cp "target/${{ matrix.target }}/release/devdocs" "dist/${out}/"
          ( cd dist && tar -a -cf "${out}.tar.gz" "${out}" )
      - uses: actions/upload-artifact@v4
        with:
          name: dist-${{ matrix.target }}
          path: dist/*.tar.gz

  publish-github:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: actions/download-artifact@v4
        with:
          merge-multiple: true
          path: dist
      - name: Checksums
        run: ( cd dist && sha256sum *.tar.gz > SHA256SUMS )
      - name: Smoke test (linux gnu)
        run: |
          tar -xzf dist/devdocs-${{ github.ref_name }}-x86_64-unknown-linux-gnu.tar.gz
          ./devdocs-${{ github.ref_name }}-x86_64-unknown-linux-gnu/devdocs --version
      - uses: softprops/action-gh-release@v2
        with:
          files: |
            dist/*.tar.gz
            dist/SHA256SUMS
          generate_release_notes: true
```

Notes:

- The `aarch64-pc-windows-msvc` target became Tier 1 in Rust 1.99 (verified 2026-10-28), so the matrix ships it.
- musl builds are fully static: bundled SQLite compiles from source, rustls needs no OpenSSL. If musl DNS becomes a problem, switch the resolver to hickory in a follow-up.
- macOS binaries ship unsigned. Gatekeeper requires a right-click open on first run. Notarization is a deliberate skip ([01](01-tech-stack-decision.md)).
- The smoke test extracts one artifact and runs `--version` before publishing.

## Publish to crates.io

release-plz (0.4.2, verified very active 2026-10-28) owns versioning, changelogs, and the publish PR. Trusted publishing (GA since mid-2025) removes the long-lived API token.

One-time setup:

1. On crates.io, open the crate Settings and add a Trusted Publisher: this repository plus the workflow filename `publish.yml`.
2. Add the `CARGO_REGISTRY_TOKEN` path only as a fallback if trusted publishing is unavailable.

`.github/workflows/publish.yml`:

```yaml
name: publish
on:
  workflow_dispatch:

jobs:
  release-plz:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
      id-token: write
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0
      - uses: dtolnay/rust-toolchain@1.99.1
      - name: OIDC token for crates.io
        uses: rust-lang/crates-io-auth-action@v1
        id: cratesio
      - uses: MarcoIeni/release-plz-action@v1
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ steps.cratesio.outputs.token }}
```

Flow: release-plz opens release PRs from Conventional Commits with generated changelogs. Merging the PR bumps versions, tags, and publishes workspace crates in dependency order. The tag push then triggers `release.yml` for the binaries.

## Versioning policy

- SemVer. Pre-1.0: breaking changes bump the minor, everything else the patch.
- The CLI exit codes, JSON shapes, and error codes in [08](08-retrieval-interfaces.md) are public contract. Any change is a breaking change and needs a changelog note.
- The DB schema has its own compatibility rule: a binary must be able to open a DB written by the same minor series. Schema breaking changes bump the minor and run a migration.
- Workspace crates version together (release-plz single-version mode). They move as one product until any crate is consumed externally.

## Local release rehearsal

Before tagging:

```sh
just ci
cargo build --release
./target/release/devdocs --version
./target/release/devdocs search "test" --json   # against a scratch DEVDOCS_HOME
```

Then `git tag vX.Y.Z && git push origin vX.Y.Z`.
