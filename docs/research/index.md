---
type: knowledge bundle index
title: Dev-Docs Tooling Research — Knowledge Bundle
description: Aggregated, deduplicated research on building a Context7-style developer-documentation tool for coding agents. Eight independent deep-research runs (2026-10-26/27) merged into one hierarchical knowledge base. Start here.
tags: [devdocs, mcp, cli, context7, agentic-workflows, index]
timestamp: 2026-10-27T19:00:00Z
resource: docs/research
sources: [DS, DSV, GLMC, GLMP, GPT1, GPT2, KIMI, GEM]
---

# Dev-Docs Tooling Research

**One question, eight independent research runs:** if you build a Context7-style tool that puts official developer documentation in front of coding agents, which architecture should it be? This bundle merges the eight runs into a single, conflict-aware knowledge base. Raw documents are read-only in [`raw/`](raw/); provenance keys for them are in [sources.md](sources.md).

## File map

| File | Type | Scope |
|---|---|---|
| [sources.md](sources.md) | registry | The raw docs (8 runs / 9 files), provenance keys, evidence tiers, known defects inside the sources |
| [verdict.md](verdict.md) | decision | The converged build decision, claims ledger (8 claims × confidence), source disagreements, open questions, reversal conditions |
| [context7-internals.md](context7-internals.md) | reference | How Context7 works: pipeline, serving contract, two pivots, pricing history, documented criticisms |
| [interfaces.md](interfaces.md) | reference | MCP vs CLI vs Code Mode vs dynamic toolsets: schema-rent measurements, the loading-policy mechanism, cost normalization, decision matrix by scenario, llms.txt supply side |
| [retrieval-design.md](retrieval-design.md) | reference | Why docs retrieval helps and how to do it: evidence tables, degradation studies, hybrid search, chunking, symbol levels, citations, budgets, fallback ladder |
| [security-licensing.md](security-licensing.md) | reference | Docs as untrusted input: threat model, mitigation set, license/ToS ingestion gate, code-mode sandbox contract |
| [benchmark-protocol.md](benchmark-protocol.md) | protocol | The missing controlled evaluation: 7 steps with completion criteria, pre-registered gates, adoption decision rule |
| [architecture.md](architecture.md) | reference | The layered design (ingest → normalize → index → serve), CLI/MCP/code-mode contracts, harness integration, phased roadmap with gates, risk register |
| [log.md](log.md) | change log | Chronological record of changes to this bundle (OKF reserved file) |

## Reading paths

- **Deciding whether/how to build** → [verdict.md](verdict.md), then [architecture.md](architecture.md)
- **Choosing an interface for a given harness** → [interfaces.md](interfaces.md) §5 decision matrix; adoption claims gated by [benchmark-protocol.md](benchmark-protocol.md)
- **Building the retrieval engine** → [retrieval-design.md](retrieval-design.md), then [architecture.md](architecture.md) L1–L3
- **Running the evaluation** → [benchmark-protocol.md](benchmark-protocol.md) (execute top-to-bottom; gates are the completion criteria)
- **Hardening for production / multi-user** → [security-licensing.md](security-licensing.md)
- **Auditing a number before quoting it** → [sources.md](sources.md) (defects list), then the parenthetical source key attached to the claim in the topic file, then the named raw document in `raw/`

## Conventions

- **Provenance keys** (DS, DSV, GLMC, GLMP, GPT1, GPT2, KIMI, GEM) — defined in [sources.md](sources.md). Provenance travels as inline parenthetical keys on each claim, e.g. `(GLMC Finding 2)` or `(DS §4.4)`; a section-level citation names a section of the *named* raw document and must resolve there. Exception: [log.md](log.md) is the OKF reserved change log and carries an empty `sources` list by design — it records changes, not claims.
- **Every number keeps its baseline.** Headline reductions are within-study only and are never compared across rows; cache-dependence and denominator caveats travel with the numbers.
- **Conflicts are explicit.** Where sources disagree, [verdict.md](verdict.md) records the disagreement and the resolution rule (audit-grade anchors win; GPT2 supersedes GPT1; majority audited figures win).
- **The verdict is a hypothesis, not a post-benchmark conclusion.** No gate in [benchmark-protocol.md](benchmark-protocol.md) has been executed; adoption claims are limited to lookup performance until they pass.
- Change history: [log.md](log.md).
