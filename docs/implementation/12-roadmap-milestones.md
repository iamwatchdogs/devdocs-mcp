---
type: guide
title: Roadmap and Milestones
description: The phased roadmap for devdocs with concrete milestones, scope, acceptance criteria, and effort. Each phase gate matches the research validation gates.
tags: [devdocs, roadmap, milestones, gates]
timestamp: 2026-11-17T10:00:00Z
resource: docs
derives-from: [docs/research]
---

# Roadmap and Milestones

## Phases

The phase gates come from the research bundle (research: architecture.md, roadmap table). A phase starts only after the previous gate passes. No gate has run yet. The eval harness in [14](14-benchmark-eval.md) runs them.

| Phase | Weeks | Deliverable | Gate |
|---|---|---|---|
| 0 — Measure | 1 | Harness probe: schema rent and per-query cost of the current docs workflow. Eval scaffold | Baseline cost per query recorded. Pilot set of 20 questions curated |
| 1 — Core | 2–4 | `devdocs` CLI + SQLite FTS5 + symbol index over llms.txt and git ingest for ~50 libraries. Version pinning. JSON + citations + hashes. Skill file | Symbol recall ≥ 95% on the golden set. hit@5 ≥ 0.8. p95 < 400 ms. $0 schema. An agent completes doc tasks through the CLI alone |
| 2 — Quality | 5–8 | Contextual headers (already in), dedup, injection filter, trust and freshness metadata, MCP shim, license registry | Retrieval failures −49% versus Phase 1 baseline. MCP and CLI produce identical results for the same query |
| 3 — Scale | 9–12 | Optional embedding hybrid with RRF. Explicit `devdocs sync` command. Benchmark run per the research protocol | Hybrid improves hit@3 without breaking the latency gate. CLI+local cost ≤ $0.20 per 1,000 queries at frontier pricing (the model in research: interfaces.md §1). Protocol gates PASS |
| 4 — Optional Code Mode | 13+ | Sandbox per the research contract. Typed facade | Sandbox escape and exfiltration suite passes. Code mode wins cost or p95 on multi-hop tasks with no security regression |

## Milestones

Effort is in focused working days for one engineer.

| ID | Milestone | Scope | Acceptance criteria | Effort |
|---|---|---|---|---|
| M0 | Repository scaffold | [02](02-project-scaffolding.md) complete. CI green on an empty workspace | `just ci` passes in CI. All lint gates active | 1 d |
| M1 | CLI skeleton | clap commands, exit codes 0/1/2/3/4, JSON errors, non-TTY rules, config load | `crates/devdocs/tests` covers every error code. `devdocs --help` fits one screen | 2 d |
| M2 | Store layer | Schema 0001, migrations runner, FTS5 verified, WAL, store API | Migration up/down tests. FTS5 behavior tests (bm25 weights, snippet, tokenizer) | 2 d |
| M3 | llms.txt ingest E2E | Fetch guard, license gate (deny by default), llms.txt v2 adapter, chrome strip, segmentation, publish | Add 3 real libraries offline-safe (fixture replay). Snapshots stable. Guard property tests pass | 4 d |
| M4 | Search + result shape | Resolve, FTS5 rung, fusion placeholder (single rung), budget pack, result envelope, `open`, `verify` | `--json` snapshots fixed. Exit 2 path with `miss_reason`. Budget tests | 3 d |
| M5 | Symbol index | tree-sitter pass over code fences for rust, go, python, javascript. `symbols` table. `devdocs symbol` | Symbol recall ≥ 95% on the pilot golden set (the Phase 1 gate, partial) | 3 d |
| M6 | Git adapter | Shell-out clone, pinned tag/commit, docs-only walk, commit provenance | Fixture-repo tests. Pinned-SHA fallback path tested. No-network CI stays green | 3 d |
| M7 | License gate full | State machine transitions, audit lines, `source approve`, runtime enforcement at all four points | Transition table tests. Revoked-source drop-at-query test | 2 d |
| M8 | MCP adapter | rmcp server, 3 tools, annotations, instructions, contract tests vs CLI | Identical-results gate passes on a 50-query corpus sample | 2 d |
| M9 | Packaging | Release matrix, checksums, trusted publishing, README, CONTRIBUTING | Tag `v0.1.0`. Release artifacts smoke-tested. Publish PR merges clean | 2 d |
| M10 | Phase 1 gate run | Golden set at 45+ questions, eval harness, full Phase 1 gate | Gate table in [14](14-benchmark-eval.md) all PASS or triaged | 3 d |
| M11 | Quality pass | M11a (#16): injection filter v0, quarantine ops, secret redaction at ingest and at output ([09](09-security-implementation.md)). M11b (#19): session dedup, trust metadata surfacing, the −49% measurement | Filter rule tests. Quarantine release flow works. Redaction pass tests ([09](09-security-implementation.md)). Failures −49% versus the failure rate the Phase 1 gate report records, measured on the golden set | 4 d |
| M12 | Phase 3 optional | M12a (#17): the `embeddings` feature, embed at ingest, binary-quantized blobs. M12b (#20): query-side vector rung, RRF hybrid, hybrid gate. Sync (#21): explicit `devdocs sync` command, no daemon | Hybrid improves hit@3 on the golden set at p95 < 400 ms. Feature-off build unchanged. Sync skips unchanged sources by hash | 5 d |

The lettered splits name the ticket set: M3a/M3b share row M3 ([13](13-implementation-plan.md) §M3), M4b is the skill companion (#22), M11a (#16) and M11b (#19) split row M11, and M12a (#17), M12b (#20), and the sync ticket (#21) split row M12.

Total to the Phase 1 gate (M0–M10): about 27 working days. Phases 2 and 3 add about 9 to 12 more.

Status note (2026-11-06): M5 is implemented. Its partial gate waits for the fixture corpus: the pilot golden set exists, the fixture DB behind it does not, so the pilot run reports placeholder zeros until the corpus lands ([14](14-benchmark-eval.md)). The ≥ 0.95 symbol-recall criterion stands.

Status note (2026-11-13): M10 is implemented. The golden set holds 48 questions across 3 libraries (next.js 16.2.9, zod 4.6.7, serde 1.0.229) and seven of the 8 task types (the poisoned case is not yet in the corpus), and the fixture corpus is committed: next.js and zod recorded via llms.txt, serde via git fixture replay at recorded commit `7fc3b4c` (a checked-out tree plus a commit-sha file, not a tag checkout) ([14](14-benchmark-eval.md) §Fixture corpus). The official gate run is PASS on every Phase 1 row (symbol recall 1.0, hit@5 1.0, p95 48.9 ms, version correctness 1.0, cost $0). The Retrieval-failures row stays INCONCLUSIVE and moves to M11 with the failure-rate baseline. The Phase 1 gate report records that baseline. The version-correctness definition was fixed before the run: abstentions leave the denominator ([01](01-tech-stack-decision.md)). The tracked gate report with the demo transcripts lives at `eval/reports/2026-11-13-pilot/`.

## Definition of done

A milestone is done when:

1. All acceptance criteria pass on the main branch, not on a feature branch.
2. The relevant guide files ([05](05-architecture.md) to [09](09-security-implementation.md)) still match the code. Divergence is a bug in the docs. Fix it in the same PR.
3. CI is green: fmt, clippy, tests on three OS targets, MSRV, features, deny, coverage, docs.
4. No new dependency without the [10](10-dependency-audit.md) checklist.

## Explicit non-goals per phase

| Phase | Not building |
|---|---|
| 1 | Code Mode, federation, HTML crawl adapter, private-source auth, TypeScript symbols |
| 2 | Web UI, hosted service, multi-user profiles |
| 3 | Alternative index backends, rerankers, hosted federation |
| 4 | Anything beyond the sandbox contract acceptance criteria |

The HTML crawl adapter stays last-resort-only. The Phase 1 corpus comes from llms.txt and git, which are publisher-sanctioned or version-exact paths (research: architecture.md §L1).

## Deferred work register

The items below have no milestone. Each entry names the trigger that starts the work. These entries are decisions, not omissions.

| Item | Decision | Trigger |
|---|---|---|
| OpenAPI adapter ([07](07-ingest-pipeline.md) §OpenAPI adapter) | Not scheduled in any milestone. The `oas3` 0.22.0 pin ([01](01-tech-stack-decision.md)) stays the chosen parser | A needed library publishes an OpenAPI specification and no llms.txt, and the gate report shows the API surface is not retrievable from the llms.txt and git corpus |
| HTML crawl adapter ([07](07-ingest-pipeline.md) §HTML adapter) | Stays last-resort-only. Not scheduled in any milestone. The robots.txt and license gates ([09](09-security-implementation.md)) are a build precondition | A needed library publishes no llms.txt and no git mirror, and the publisher grants crawl rights. The gates must exist before any build starts |
| Near-duplicate cosine dedup ([07](07-ingest-pipeline.md) §Normalization pipeline) | Deferred out of the M12a/M12b split. No ticket owns it | Duplicate chunks crowd the top search ranks on the golden set, and the gate report shows a hit-rate loss from them |
| `devdocs db vacuum|stats` ([08](08-retrieval-interfaces.md) §CLI contract) | Placeholders today. One polish ticket owns both when the trigger fires. Startup migrate is not deferred: it runs at CLI start (#6) and at MCP server start (#12) | Large-replacement bloat appears in the store, or size reporting becomes necessary to debug it |
| FTS5 monthly fuzz job ([13](13-implementation-plan.md) §Risk register) | Not scheduled. The property test runs on every CI run | A fuzz finding, or a MATCH parse failure in a tracked issue |
| Gemfile.lock parser ([08](08-retrieval-interfaces.md) §Version resolution) | Not scheduled. Ruby lockfiles parse in a later phase | A Ruby library joins the golden set |
| Phase 4 Code Mode | Unscheduled by design. Phase 4 starts only after the Phase 3 gate passes | The Phase 3 gate PASS, plus documented multi-hop cost and p95 evidence (research: benchmark-protocol.md) |
