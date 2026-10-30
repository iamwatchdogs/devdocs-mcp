---
type: guide
title: CI/CD and Release
description: The GitHub Actions workflows, the release build matrix, the crates.io publish path with trusted publishing, and the versioning policy. Action versions digest-pinned 2026-10-29.
tags: [devdocs, ci, github-actions, release, crates-io]
timestamp: 2026-10-30T00:00:00Z
resource: docs
derives-from: [docs/research]
---

# CI/CD and Release

## Workflows

Four workflows:

| Workflow | Trigger | Purpose |
| --- | --- | --- |
| `ci.yml` | push to `main`, pull requests, manual dispatch | fmt, lint, tests (3 OS), doc tests, msrv, features, deny, coverage, docs |
| `release.yml` | push of `v*.*.*` tags | binary build matrix, build provenance attestation, GitHub release |
| `publish.yml` | push to `main`, manual dispatch | release-plz release PR + crates.io publish + version tag |
| `audit.yml` | daily + weekly schedules, manual dispatch | cargo-deny + cargo-audit daily, cargo-machete weekly |

Two invariants hold across all four workflows:

- **Digest pinning.** Every action is pinned to a full commit SHA with a `# vX.Y.Z` version comment. Renovate (`github-actions.pinDigests: true`) keeps the pins current and opens a single grouped `ci: actions` PR per update batch; action majors are grouped deliberately (cheap to revert). Pinned versions verified 2026-10-29:

- `actions/checkout` v7.0.1, `actions/upload-artifact` v7.0.2, `actions/download-artifact` v8.0.2, `actions/github-script` v9.0.0
- `actions/attest-build-provenance` v4.2.2, `actions/attest` ecosystem for provenance
- `softprops/action-gh-release` v3.0.3, `codecov/codecov-action` v7.1.1
- `dtolnay/rust-toolchain` v1, `Swatinem/rust-cache` v2.9.3, `taiki-e/install-action` v2.88.0, `EmbarkStudios/cargo-deny-action` v2.1.1
- `MarcoIeni/release-plz-action` v0.5.147

- **Token hygiene.** Every `checkout` runs with `persist-credentials: false` — the job token is never left in the runner's git config where dependency build scripts (or a compromised step) could read it. The one exception-free rule: no step in any workflow pushes via git credentials (release-plz authenticates through its `GITHUB_TOKEN` env; release upload and attestations go through APIs).

The `msrv` and `coverage` jobs are CI-only; `just ci` covers the rest locally.

## ci.yml

All compiling jobs share one composite action, `.github/actions/setup-rust` (toolchain + cache + optional tools), so the preamble exists once. Every job sets `timeout-minutes`; the test matrix keeps `fail-fast: false` so one OS failing does not mask the others.

```yaml
jobs:
  fmt:        # toolchain 1.99.1 + rustfmt + just, runs `just fmt-check`
  lint:       # toolchain 1.99.1 + clippy + just, runs `just lint`
  test:       # 3-OS matrix (ubuntu/macos/windows), runs `just test`
  test-doc:   # runs `just test-doc` (cargo test --doc; nextest does not run doctests)
  msrv:       # toolchain 1.90.0, runs `just msrv`
  features:   # runs `just features` (3 feature states of the devdocs binary)
  deny:       # EmbarkStudios/cargo-deny-action
  coverage:   # cargo-llvm-cov + codecov upload (token: secrets.CODECOV_TOKEN)
  docs:       # runs `just docs` (rustdoc -D warnings)
  gate:       # aggregate check: needs every job above; require only "gate"
              # in branch protection (matrix legs publish per-OS check names)
  commits:    # PR-only: validates the PR title against the Conventional
              # Commits rule enforced by the lefthook commit-msg hook —
              # squash merges land the title as the commit release-plz parses,
              # and a non-conventional title would silently produce no release PR
```

Notes:

- All cargo-invoking recipes (`lint`, `test`, `test-doc`, `docs`, `features`) run `--locked`: CI tests the committed lockfile, not whatever cargo would resolve today. A Cargo.toml edit without a matching Cargo.lock fails loudly instead of silently regenerating the dependency set.
- `just test` / `just deny` hard-fail when their tool is missing — everywhere, with no fallback. CI installs tools explicitly via the composite action, so a missing tool is a loud environment failure, not a silent downgrade.
- The composite action's install-action tools (`just`, `cargo-nextest`, `cargo-llvm-cov`) and audit.yml's (`cargo-audit`, `cargo-machete`) deliberately float at latest: Renovate cannot bump install-action inputs, so a pin would go stale silently, and security scanners must see new releases. The release-critical toolchain (`cargo-zigbuild@0.24.0` + `ziglang==0.17.0`) is pinned where it is used; those two pins are human-maintained (no manager covers install-action inputs or inline pip pins) — review them at each release rehearsal. Same policy for the Rust toolchain literals: Renovate is configured to leave the dtolnay/rust-toolchain `toolchain` input alone (dep `rust` disabled) because `rust-toolchain.toml` is the toolchain's source of truth — an auto-bumped release/publish compiler desynchronized from the CI-validated one would erode the release gate.
- The `msrv` recipe validates the real lock file against toolchain 1.90 (the workspace `rust-version` in `Cargo.toml`; do not restate the literal here — this file references it).
- The coverage job necessarily re-runs the full suite: instrumented builds cannot share the nextest cache profile. `cargo llvm-cov` accepts nextest as its runner if a speed-up is ever needed.
- codecov uploads carry `token: ${{ secrets.CODECOV_TOKEN }}` (optional secret, not in the setup list): codecov-action v4+ supports tokenless uploads only from fork PRs to public repos, so an unset secret degrades main-branch uploads to a logged failure — which does not fail the job (`fail_ci_if_error` is unset).
- The `features` recipe will compile the embeddings backend (ort) once the feature flags forward to the store crate; today they are empty no-ops (README "Scaffold deviations"), so the cache cost is currently bounded by compilation alone.

## release.yml (binaries)

The release builds use native runners for macOS and Windows and `cargo-zigbuild` for Linux, including musl and arm64. `cross-rs` has not tagged a release since 2023, so it is not used. Cargo-dist exists but moves slowly. The hand-rolled matrix is easier to audit for this project. A workflow-level concurrency group (`release`, no cancellation) serializes tag pushes so the 7-leg matrix never runs twice concurrently.

```yaml
permissions:
  contents: read          # workflow default; only publish-github gets write

jobs:
  build:
    timeout-minutes: 90
    permissions:
      contents: read
      id-token: write      # for build provenance attestation
      attestations: write
    strategy:
      fail-fast: false
      matrix:
        include:
          - { target: x86_64-apple-darwin,        os: macos-15,       zig: false, smoke: false }
          - { target: aarch64-apple-darwin,       os: macos-15,       zig: false, smoke: true  }
          - { target: x86_64-pc-windows-msvc,     os: windows-latest, zig: false, smoke: true  }
          # Windows ARM64 is out of scope (docs 01); do not add a Windows ARM64 target.
          - { target: x86_64-unknown-linux-gnu,   os: ubuntu-latest,  zig: true,  smoke: true  }
          - { target: aarch64-unknown-linux-gnu,  os: ubuntu-latest,  zig: true,  smoke: false }
          - { target: x86_64-unknown-linux-musl,  os: ubuntu-latest,  zig: true,  smoke: true  }
          - { target: aarch64-unknown-linux-musl, os: ubuntu-latest,  zig: true,  smoke: false }
    steps:
      - checkout (fetch-depth: 0, persist-credentials: false), then verify the
        tag commit is reachable from origin/main — branch protection gates main
        pushes, not tag pushes, so without this a manually pushed tag could
        build, attest, and publish binaries from un-gated code
      - toolchain (targets: matrix.target), rust-cache keyed by matrix.target
        (the default key omits matrix values, so the Linux legs sharing job id
        `build` would overwrite one entry)
      - [zig targets] taiki-e/install-action for cargo-zigbuild@0.24.0
        (checksum-verified); pip install ziglang==0.17.0 for the zig backend —
        ziglang ships only on PyPI, so its pin is version-only (accepted
        residual trust; cargo-zigbuild itself is checksummed)
      - cargo [zig]build -p devdocs --release --locked --target ${{ matrix.target }}
      - smoke test: run the built binary's `--version` (skipped where the
        runner cannot execute the target natively — x86_64-apple-darwin would
        run only under Rosetta emulation on the arm64 macos-15 runner, which
        is not a meaningful smoke of the native build, and the arm64 Linux
        targets on x86_64 runners)
      - Package: dist/devdocs-<tag>-<target>/ with the binary; .tar.gz for
        POSIX targets, .zip for Windows; per-archive .sha256 next to each
        archive (sha256sum via Git Bash on Windows and coreutils on Linux;
        shasum -a 256 on macOS, which ships no GNU coreutils)
      - actions/attest-build-provenance on dist/* — SLSA build provenance
        stored in the repo's attestation store
      - upload-artifact as dist-<target> (if-no-files-found: error)

  publish-github:
    needs: build
    timeout-minutes: 10
    permissions:
      contents: write
    steps:
      - download-artifact (merge-multiple) into dist/   # no checkout: the job
        only consumes artifacts; softprops resolves the tag from the event
      - smoke test: extract the linux-gnu archive, run `--version`
      - softprops/action-gh-release uploads dist/*.tar.gz, dist/*.zip,
        dist/*.sha256 with generated release notes
```

Notes:

- macOS runners are pinned to `macos-15` (arm64), not the floating `macos-latest` label: the smoke flags hardcode which targets the runner executes natively, and a label repoint would silently invert them. The x86_64-apple-darwin smoke is skipped deliberately — Rosetta emulation exercises an emulated, not native, binary. The Windows release leg floats on `windows-latest` (x86_64 today; its smoke flag assumes that). ci.yml's test matrix keeps floating labels deliberately: CI tracks the current runner image for coverage breadth, while release pins macOS for smoke-flag determinism. Branch protection should require only ci.yml's aggregate `gate` check — matrix legs publish per-OS check names that silently desync from a hand-maintained required-checks list.
- Windows ARM64 (`aarch64-pc-windows-msvc`) is Tier 1 since Rust 1.99 but is deliberately out of scope for Phase 1 per [01](01-tech-stack-decision.md) — the matrix must not add it.
- musl builds are fully static: bundled SQLite compiles from source, rustls needs no OpenSSL. If musl DNS becomes a problem, switch the resolver to hickory in a follow-up.
- macOS binaries ship unsigned. Gatekeeper requires a right-click open on first run. Notarization is a deliberate skip ([01](01-tech-stack-decision.md)).
- Consumers verify a binary's origin with `gh attestation verify <artifact> -R <owner>/<repo>`; the `.sha256` files alone are self-generated and prove nothing independently.

## Publish to crates.io

release-plz (CLI 0.4.2; action v0.5.147, verified 2026-10-29) owns versioning and changelogs. Trusted publishing (GA since mid-2025) removes the long-lived API token. Both jobs use the `RELEASE_PLZ_TOKEN` secret. The workflow runs two jobs on every push to `main` (and on manual dispatch, which both jobs gate to `refs/heads/main` so a stray dispatch from a feature branch can never publish):

- `release-plz-pr` — runs `release-pr` (the command input is explicit: without it the action runs both commands), opening/updating the release PR with `contents: write` + `pull-requests: write`. It uses `RELEASE_PLZ_TOKEN` rather than `GITHUB_TOKEN`: `pull_request` runs from a GITHUB_TOKEN-created PR land in an approval-required state that is easy to miss, while a PAT-created PR gets ci.yml automatically. The release PR therefore always runs fmt/lint/test/deny before the human merge. This job has its own concurrency group; the release job must not (see below).
- `release-plz-release` — runs `release` after the release PR is merged: publishes the workspace crates and creates the version tag; no-ops when there is nothing to release. Permissions: `contents: write`, `pull-requests: read` (to detect the release PR), `id-token: write`. It **must** use `RELEASE_PLZ_TOKEN`, not `GITHUB_TOKEN`: events triggered by the default `GITHUB_TOKEN` never start other workflow runs, so a GITHUB_TOKEN-created tag would never fire `release.yml` and binaries would silently never be published. Both jobs carry a guard step that fails with setup instructions when the secret is missing.

Two deliberate deviations from "add a concurrency group everywhere": the release job has **no** concurrency group — with `cancel-in-progress: false` a newly queued run still replaces a pending one, which can silently skip a release; a superseded run is mostly self-healing (the newer run publishes the same or newer versions), so upstream's guidance (never risk skipping a release) wins. And trusted publishing is release-plz's built-in OIDC exchange (`id-token: write` is all it needs) — no `rust-lang/crates-io-auth-action` step and no `CARGO_REGISTRY_TOKEN`.

`release.yml` owns the GitHub release object and its notes (it has the artifacts). `release-plz.toml` sets `git_release_enable = false` so release-plz only publishes crates and pushes the tag, and disables per-package tags: release-plz's multi-package default is `{package}-v{version}`, which `release.yml`'s `v*.*.*` trigger would never match — the binaries, attestations, and GitHub release would silently never run. Instead, the `devdocs` crate alone pushes `v{{ version }}` (package-section override); it is released last in dependency order, so the single tag is created only after every other crate has published, and release-plz skips tag creation when the tag already exists. Two automated writers on one release would race, and the softprops update path overwrites the release body by default (`append_body` defaults to replacement), which would clobber release-plz's changelog — so the ownership is single and decided, not emergent.

One-time setup:

1. On crates.io, open the crate Settings and add a Trusted Publisher: this repository, the workflow filename `publish.yml`, **and the environment `release`**. Note the crates.io limitation: a brand-new crate cannot use trusted publishing on its first publish — publish it manually once, then the chain owns it.
2. Create a fine-grained personal access token (Contents: read/write **and** Pull requests: read/write, on this repository only — creating the release PR needs the Pull requests permission) and store it as the `RELEASE_PLZ_TOKEN` repository secret.
3. Create the `release` GitHub environment (repo Settings → Environments) restricted to the `main` branch; the release-plz-release job declares it. The environment claim in the Trusted Publisher entry means OIDC tokens minted by any other job in the repo — including the release.yml build legs, which hold `id-token: write` for attestations and execute dependency build scripts — are rejected by crates.io.

Flow: push to `main` → release-plz opens a release PR (changelog + version bumps, CI runs on the PR automatically) → a human merges it → the merge push runs `release-plz release`, which publishes crates in dependency order and creates the version tag → the tag push triggers `release.yml` for the binaries and the GitHub release. The human merge is the release gate.

## Scheduled audits

`audit.yml` runs `cargo-deny` and `cargo-audit` daily and `cargo-machete` weekly (the job-level `if:` guards compare against the exact cron strings — change them together). Runs are serialized by a concurrency group (`cancel-in-progress: false`): a manual dispatch queues behind an in-flight scheduled run instead of cancelling it, because a cancelled run reports `cancelled` — not `failure` — and its result would never reach the report job. Only scheduled failures open a `dependencies`-labeled issue; recurrences add comments to the existing issue instead of spawning duplicates (dedup paginates all open issues). Note the platform behavior: GitHub auto-disables scheduled workflows after 60 days without repository activity — Renovate's automated PRs keep this repo active, but if activity ever stops, re-enable the workflow from the Actions tab.

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

Then merge the release PR (or `git tag vX.Y.Z && git push origin vX.Y.Z` for a tag without a crates.io publish).

The first real release doubles as a rehearsal for the paths `just ci` cannot reach: the zig install on the ubuntu runner image, the `cargo-zigbuild@0.24.0` × `ziglang==0.17.0` pairing (no upstream compatibility matrix exists — record the verified pairing in the workflow comment once confirmed), the per-target rust-cache keys, and the release.yml tag→release chain. Treat its first run as supervised.
