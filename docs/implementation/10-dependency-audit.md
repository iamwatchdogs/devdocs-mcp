---
type: guide
title: Dependency Audit
description: The dependency policy, the audit tool workflow, the Renovate configuration, the new-dependency checklist, and the verified crate-selection table with alternates.
tags: [devdocs, dependencies, cargo-deny, renovate, supply-chain]
timestamp: 2026-10-31T00:00:00Z
resource: docs
derives-from: [docs/research]
---

# Dependency Audit

## Policy

1. Small dependency graph. The tool is a local, read-only binary. Every dependency is attack surface and compile time.
2. Deny by default: unknown registries and git sources fail `cargo deny check` (see [02](02-project-scaffolding.md)).
3. Cargo.lock is committed and pinned. Renovate updates it through reviewable PRs.
4. MSRV 1.90 stays intact. A dependency that forces a higher MSRV needs an explicit decision (see [03](03-code-quality-toolchain.md)).

## Audit workflow

The audit tools float at latest, both locally through `cargo binstall` and in CI. A scanner version pin goes stale silently, because Renovate cannot bump install-action inputs (see [11](11-ci-cd-release.md)). The Rust toolchain stays pinned through `rust-toolchain.toml`.

| Check | Tool | When | Gate |
|---|---|---|---|
| Licenses, advisories, sources, duplicate versions | cargo-deny | every PR + daily schedule | block on failure |
| RustSec advisories | cargo-audit | daily schedule + release | block on vulnerabilities. Triage notes required for warnings |
| Unused dependencies | cargo-machete | weekly schedule | block on findings |
| Public API breaks between lib releases | cargo-semver-checks | on release PRs | block on unexpected breaking change |
| Duplicate-version drift | cargo-deny `[bans] multiple-versions = "warn"` | every PR | review note required |

The advisory-db received commits one day before the 2026-10-28 verification, so the daily schedule sees fresh advisories. A `RUSTSEC-2026-XXXX` advisory on a direct dependency is same-day work: patch, bump, or replace with a documented decision in the PR.

## Renovate

Renovate is the update manager. Dependabot still has Cargo workspace defects in 2026 (member manifests not updated. Issue 14196), so Renovate is the pick (verified 2026-10-28). Config at `.github/renovate.json`:

```json
{
  "$schema": "https://docs.renovatebot.com/renovate-schema.json",
  "extends": ["config:recommended", ":semanticCommits"],
  "managers": ["cargo", "github-actions"],
  "lockFileMaintenance": { "enabled": true },
  "cargo": {
    "pinDigests": false
  },
  "github-actions": {
    "pinDigests": true
  },
  "packageRules": [
    {
      "description": "Group patch and minor updates",
      "matchUpdateTypes": ["patch", "minor"],
      "groupName": "deps: patch and minor",
      "schedule": ["before 6am on monday"]
    },
    {
      "description": "Major updates one at a time",
      "matchUpdateTypes": ["major"],
      "groupName": null,
      "schedule": ["before 6am on monday"]
    },
    {
      "description": "GitHub Actions digest pinning",
      "matchManagers": ["github-actions"],
      "groupName": "ci: actions"
    }
  ],
  "timezone": "UTC",
  "labels": ["dependencies"]
}
```

Notes: Renovate runs the latest stable cargo and has no MSRV awareness. The MSRV CI job is the safety net for that gap. GitHub Actions are digest-pinned. Cargo dependencies stay version-pinned through the lock file.

## New-dependency checklist

Every new direct dependency answers these ten questions in the PR description. One "no" without a mitigation blocks the merge.

1. Is there no reasonable way to write this ourselves in under 200 lines?
2. Is the license on the `deny.toml` allow list?
3. Is the crate maintained (release or commit inside the last 6 months)?
4. Does the crate have an MSRV at or below 1.90?
5. Does it compile with `--no-default-features` for our feature set?
6. How many transitive dependencies does it add? (`cargo tree -i <crate>`, plus a before-and-after `cargo tree | wc -l` count in the PR.)
7. Is it on the alternates table below, or is a better-known alternative documented and rejected?
8. Does it run code at build time (`build.rs`)? What does the build script do?
9. Does it add unsafe code, `cc` compilations, or downloads at build time? Which?
10. Do we need the whole crate or can a smaller feature set serve?

## Verified selection table (2026-10-28)

Direct dependencies with the alternates considered. Keep this table current when dependencies change.

| Dependency | Picked | Alternates considered | Why picked |
|---|---|---|---|
| async runtime | tokio 1.54.1 | smol, async-std (stale) | Standard, maintained |
| HTTP client | reqwest 0.13.5 | hyper direct (more code), ureq (sync, no custom resolver path) | dns_resolver hook for SSRF pinning. Rustls default |
| SQLite | rusqlite 0.41.0 bundled | sqlx (heavy, async driver), Turso (pre-1.0) | Bundled FTS5, sync API fits spawn_blocking design |
| CLI | clap 4.6.7 derive | argh (too thin), bpaf | Standard, completions |
| Markdown | comrak 0.59.0 | pulldown-cmark (no GFM tables by default, no sourcepos parity), markdown-rs (younger) | Reliable sourcepos for code blocks |
| HTML extraction | dom_smoothie 0.19.0 | scraper (raw DOM, more code), readability ports list | Active, Markdown out |
| Extraction fallback | rust-trafilatura 2.2.8 | htmd 0.5.6 | Published F1 scores. Keep htmd as a noted alternate |
| Chunker | text-splitter 0.33.1 | hand-rolled | Markdown-aware, code-block-atomic, token lengths |
| Symbols | tree-sitter 0.25.8 + 4 grammars | regex heuristics only | AST-grade symbols. TS deferred (grammar conflict) |
| MCP | rmcp 3.8.0 | hand-rolled JSON-RPC (more surface to own) | Official SDK, adopters, spec-current |
| OpenAPI | oas3 0.22.0 | openapiv3 2.2.0 (no commits since 2025-06) | Activity in 2026 |
| YAML | serde_yaml_ng 0.10.0 | serde_yaml (archived), serde_yml (archived) | Only maintained fork |
| Token count | heuristic. tiktoken-rs 0.12.1 optional | HF tokenizers (heavy) | Budget math is approximate by design |
| Embeddings (optional) | fastembed 7.5.0 | ort direct, candle | Ships bge-small-en-v1.5 (384d, Apache-2.0) on the ort backend |
| Robots.txt | internal minimal parser | texting_robots 0.2.2 (dormant since 2023) | Small grammar. Vendor-in with tests, no dormant dep |
| Errors | thiserror 2.0.21 / anyhow 1.0.104 | — | Standard split |

Rejected on evidence: `sitemap` crate (unmaintained since 2020. Sitemap parsing is 50 lines with quick-xml if ever needed), `cargo-husky` (unmaintained since 2020. Lefthook instead), `sqlite-vec` (dormant, pre-1.0. In-process scan instead), `git2` (credential and shallow-fetch gaps. System git instead).

## Optional supply-chain additions

- cargo-vet (0.10.3, active) records reviewed dependency versions with an auditable criteria file. Adopt when the contributor count grows past one. It adds process cost that a solo project does not need yet.
- cargo-crev is low-activity (verified 2026-10-28). Not adopted.
- `cargo tree` diff in the new-dependency PR (item 6 of the checklist) is the lightweight substitute for now.

## MSRV and feature gates in CI

- The MSRV job runs `cargo check --workspace --all-targets --locked` on toolchain 1.90. Renovate PRs run it too, because a fresh lock file can pull a dependency that needs a newer toolchain.
- `--no-default-features` job state: one CI job builds the binary with no features, one with `exact-tokens`, one with `embeddings`. All three must compile ([02](02-project-scaffolding.md)).
