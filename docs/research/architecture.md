---
type: reference
title: Recommended Architecture — One Core, Three Thin Adapters
description: The layered docs-tool design the evidence converges on (ingest → normalize → index → serve), the CLI and MCP contracts, the roadmap with validation gates, and the risk register. The corpus is the product; adapters are swappable.
tags: [architecture, cli, mcp-adapter, roadmap, layered-design]
timestamp: 2026-10-27T19:00:00Z
resource: docs/research
sources: [DSV, GLMP, GPT1, GPT2, GLMC, DS, GEM]
---

# Recommended Architecture

Four layers, one core, three thin adapters (GLMP Ch.11, DSV §5, GPT1/GPT2 §7 — all three converge independently):

```
┌──────────────────────────────────────────────────────┐
│ L1 INGEST   llms.txt/llms-full.txt → git repos       │
│             (pinned commits/tags) → OpenAPI → HTML   │
│             last resort; license/ToS gate per source │
├──────────────────────────────────────────────────────┤
│ L2 NORMALIZE  chrome strip → AST-aware segmentation  │
│             (never split code blocks) → contextual   │
│             chunk headers → dedup → injection filter │
├──────────────────────────────────────────────────────┤
│ L3 INDEX    SQLite FTS5 (BM25) + exact-symbol index  │
│             + optional vectors (RRF fusion) + rerank │
│             + trust/freshness metadata + golden set  │
├──────────────────────────────────────────────────────┤
│ L4 SERVE    CLI (primary) │ MCP 2–4 tools │ Code Mode│
│             all thin wrappers over the same core     │
└──────────────────────────────────────────────────────┘
```

Every layer is open-source commodity or directly evidenced practice — nothing requires novel research (GLMP Ch.11). Layer-by-layer rationale and the contracts follow.

## L1 — Ingest (distribution-first)

Priority order (GLMP L1, DSV §5.1, GPT2 §2.7): **llms.txt / llms-full.txt** (publisher-sanctioned; Mintlify auto-generates; per-page `.md` twins where offered) → **documentation git repos pinned to commits/tags** (reproducible, version-exact) → **OpenAPI/JSON Schema** where available → **HTML crawl** as last resort. Version detection from manifests (`package.json`-pattern, Context7's). Refresh stratified by library velocity — daily for fast-moving libraries the agent touches, on-demand elsewhere; support both scheduled and event-driven refresh.

License/ToS gating is an L1 property, not a footnote: the per-source registry state machine in [security-licensing.md](security-licensing.md) runs before anything is fetched, embedded, or served. llms.txt quality varies (ADK's contained no links) — degrade gracefully, don't trust blindly ([interfaces.md](interfaces.md) §6).

## L2 — Normalize

Chrome stripping; **AST-aware segmentation that never splits code blocks** (the failure mode behind "useless snippet" complaints); contextual chunk headers (−49% retrieval failures, −67% with reranking); dedup (exact + cosine); prompt-injection filtering at ingest; chunk shape per [retrieval-design.md](retrieval-design.md) §3.2 (heading+breadcrumb, signature, params, example, explanation, provenance).

## L3 — Index

Deliberately boring (GLMP L3): **SQLite FTS5** as the BM25 baseline (proven local-first pattern; single-digit-ms queries, offline); tree-sitter **exact-symbol index** for signatures; optional embedding hybrid fused by **reciprocal rank fusion** (GEM §6's reference pipeline: ~70% vector / 30% BM25 via RRF, then MMR dedup; binary-quantized vec0 partitions per library+version cut storage ~32× and keep brute-force KNN under 75ms past 100k vectors; the whole corpus for a project's dependency tree fits one portable SQLite file — GEM's `~/.docctl/docs.db` single-file storage); metadata on every row: `(library, version, source_url, commit_sha, content_hash, section_path, symbol, text)` (GLMC Rec. 1) + trust/freshness/deprecation/license_status. For the vector rung, Alex Garcia's **sqlite-vec** extension (v0.1.9) loads natively in Node v22.18+/v24 LTS (`node:sqlite` with `allowExtension: true`), Python, and Go — a zero-dependency in-process RAG engine with no external servers (GEM §6). **Golden-question eval set per library** — Context7's benchmark-driven quality loop, reproduced locally ([context7-internals.md](context7-internals.md)); gate re-indexed content on it before shipping.

## L4 — Serve (the central finding as code)

### CLI — the primary adapter

Zero schema cost in shell-capable harnesses; composable (`| jq`, `| grep`); works offline/CI. Design rules (GLMP §6, DSV §3, GPT1 §7.2):

```bash
devdocs add https://github.com/vercel/next.js --version v16.2.9
devdocs sync
devdocs search "rewrite a request in Next.js proxy" --library next.js --version 16 --json
devdocs symbol next.js@16.2.9 useRouter --json
devdocs open docs://next.js/v16/proxy#rewrite --max-tokens 1200 --json
devdocs versions next.js --json
devdocs verify docs://next.js/v16/proxy#rewrite
devdocs eval <suite> --json
```

Required properties: JSON output mode; **stable exit codes** (0 hit, 2 no-result, 3 upstream error, 4 budget exceeded — DS §3); bounded output (`--max-tokens`, `--fields`, `--head`); deterministic ordering; machine-readable errors on stderr; local cache; no secrets in stdout/argv (stdin for sensitive queries); source/version filters; **provenance in every response** (library, version, source URL, last-verified date). One binary, no daemon, no registration for basic use; discoverable via `--help`. GEM §5's agent-native CLI checklist (drawn from Justin Poehnelt's Agentic CLI Guide, implemented in the Google Workspace CLI `gws`; the marcioaltoe agentic-CLI design spec; and Apideck's OpenAPI-native CLI) adds seven contract properties worth adopting wholesale: 3-tier progressive disclosure (`--list` ~20 tokens → `--toc` ~200 → section read; measured effect: discovery overhead drops from 50,000+ tokens of exploration to ~80–400 tokens of manifest — GEM §5); non-TTY auto-detection that disables spinners/colors/tables; runtime introspection (`schema --json`, `--describe`) instead of prose man pages; fail-fast non-interactive errors as structured JSON (`{"error": "PKG_NOT_INDEXED", "hint": ...}`); strict input hardening (reject path traversal, control chars, double encoding); a paired ~500-token SKILL.md written by `skill install`; and one static binary that also starts an MCP server (`devdocs mcp --transport stdio`). The wrapper pattern has named precedents: Phil Schmid's `mcp-cli` (85× token reduction via progressive shell discovery) and Sam Morrow's `tool-cli`/`mcpi` both mount an MCP interface as a thin stdio/HTTP wrapper over a CLI binary (GEM §5). GEM §7's reference runtime shape is the same idea: one static binary (Go + Cobra, or TypeScript on Node 24 LTS `node:sqlite`) serving CLI by default and MCP on demand. A docs CLI is not a novel category — the lineage supplies proven conventions: DevDocs.io (offline-capable per-language/version API corpora for human developers, a decade old), tldr-pages (community example-first distillation of man-page verbosity), and Microsoft Learn's free no-key CLI + MCP server shipped from one corpus (GLMP §6). GLMP §8 names the axis token tables miss — **latency**: local SQLite returns in single-digit ms and works offline vs ~200ms at the edge for hosted retrieval and seconds of code-gen for code mode; fast, cheap local retrieval invites the agent to *check docs more often*, which is a behavior win beyond per-query economics.

**Ship a lean skill file with it** — a novel CLI has no model prior knowledge; Context7's own CLI skill pack is "roughly 9.9 KB (2,008 B SKILL.md + 5,896 B references + 2,377 B rules)" versus ~4.5 KB for the MCP-path skill and rule files (GLMC Finding 1, pinned source; the ~9.9 KB headline is GLMC's own rounding of the byte figures, restated in Insight 2); GEM's paired skill carries an ~80-token `docctl agent-prompt` summary as its frontmatter load. Budget for it from day one and keep it lean; the 12.7k-token one-vendor skill is the failure-mode illustration ([interfaces.md](interfaces.md) §1). Harness allowlisting of the exact binary pattern (`Bash(devdocs:*)`) avoids the host's background safety-scan tax ([interfaces.md](interfaces.md) §1; GEM §4).

### MCP adapter — 2–4 tools max

`search_docs` (with `detail` parameter), `open_doc`, `verify_doc` — thin wrapper over the same core, never a reimplementation (DSV §5, GPT1 §7.3, GLMC Rec. 3). GLMP's own shim naming is the precedent to copy: exactly two tools — `docs_search` with a `detail` parameter (`name`/`summary`/`full`) and `docs_fetch` — replicating the Tool Search detail-level contract and the two-tool economy that keeps Context7's own footprint near 1,500 tokens (GLMP Ch.11). Sized to stay below the ~10%-of-context auto-deferral threshold (GLMP L4). Token-lean, discriminative descriptions (description quality is a measured failure source — [retrieval-design.md](retrieval-design.md) §3.6). Server instructions state when to use each tool and when to stop. Gate: MCP and CLI produce **identical retrieval results** for the same query (DS §7 Phase 3).

Avoid exposing many overlapping operations (separate library-lookup/page-search/section-fetch/signature-fetch/comparison tools) unless evaluation shows separation improves behavior (GPT1 §7.3). Consolidation principle: workflow-shaped tools, high-signal fields only (DSV §4(b)).

### Code-mode adapter — optional, gated

Small typed facade (`docs.search`, `docs.open`, `docs.compact`) for sandbox-equipped harnesses doing multi-hop work: parallel lookup, version comparison across libraries, citation verification, batch retrieval with in-sandbox filtering; main model receives only the compact result (GPT1 §7.4). GEM §7's reference shape is a **single code-mode tool** (`execute_doc_query_ts`) in an isolated Worker/QuickJS sandbox with `defer_loading: true` — cutting multi-step documentation cross-referencing tokens by **56–58%** while preserving prompt caching across turns. Ship only after the sandbox contract in [security-licensing.md](security-licensing.md) §4 passes and only if it wins on the [benchmark-protocol.md](benchmark-protocol.md) gates at *your* tool count (2–4 tools, not 500 — [interfaces.md](interfaces.md) §3).

### Harness integration (optional, measured elsewhere)

Three host-side patterns from GEM §6 cut docs context without touching the core: **pre-agentic lockfile resolution** — a deterministic step before the first LLM turn parses `package.json`/`pnpm-lock.yaml`/`pyproject.toml`/`requirements.txt`/`go.mod`/`Cargo.toml` (e.g. `next@15.2.1`, `zod@3.24.0`), verifies the matching local index partitions are synced, and injects a ~150-token manifest of pre-indexed library IDs, eliminating the resolve round-trip (echoed in [retrieval-design.md](retrieval-design.md) §3.4); **file-spilling** — outputs over ~1,500 tokens are written to a deterministic workspace file (e.g. `/tmp/docctl_cache/nextjs_app_router.md`) and the model receives a ~200-token outline (path + headings + line ranges), reading exact lines on demand (GitHub Copilot CLI / OpenCode `~/.local/share/opencode/tool-output/` / Morrow's `mcpi` pattern); and **research-subagent delegation** — exploratory reads of 20k+ tokens run in a disposable subagent context that returns a 1,000–1,500-token implementation contract, keeping the primary transcript lean. All three are harness policies, not core features; adopt per harness.

### One-request fast path

Core exposes unified `docs.search(query, library?, version?, language?, limit?, max_tokens?, detail?)` as the normal path; explicit `resolve` reserved for ambiguity (GPT1/GPT2 §7.1). Evidence result shape and version-status metadata per [retrieval-design.md](retrieval-design.md) §3.8.

### Optional federation

Expose Context7 (or GitMCP/Docfork) as an **optional remote provider** behind the same interface for long-tail coverage — within its 1,000 calls/mo free cap, never a hard dependency (GLMC Rec. 3, DSV §5, GLMP Ch.10). Hybrid economics: local index for your core stack + private docs; hosted for breadth you don't crawl.

## Cold-start policy

First touch of an un-ingested library: return explicit `not_indexed`/`pending` with a sync hint — never silently scrape an untrusted URL synchronously; bounded direct-fetch fallback only for approved sources. Prioritize ingestion by declared dependency, request frequency, version churn, trust/license status. Measure time-to-first-index, storage per version, refresh bandwidth, embedding cost (GPT2 §6.4).

## Build roadmap with validation gates

Phases from GLMC Recommendations, GLMP Ch.12 (90-day shape), DS §7 (build plan with evaluation gates), GPT2 §12 — merged. Each gate must pass before the next phase begins. Status: **no gate executed** (see [verdict.md](verdict.md)).

| Phase | Weeks | Deliverable | Gate (completion criterion) |
|---|---|---|---|
| 0 — Measure | 1 | Harness probe: schema rent + per-query cost of current workflow vs published bands; eval-harness scaffold | Baseline cost/query recorded; schema rent within published bands; golden-set pilot (~20 questions) effort estimated |
| 1 — Core | 2–4 | `devdocs` CLI + SQLite FTS5 + exact-symbol index over llms.txt/git ingest for ~50 most-used libraries; version pinning; JSON + citations + hashes; lean skill file | Exact-symbol retrieval recall ≥95% on golden set; hit@5 ≥0.8; p95 latency <400ms; $0 schema; agent completes docs tasks via CLI only |
| 2 — Quality | 5–8 | Contextual chunk headers, dedup, injection filter, trust/freshness metadata; 2–4-tool MCP shim; license/ToS registry | Retrieval failures −49% vs Phase 1 (contextual-retrieval bar); MCP + CLI produce identical results for same query |
| 3 — Scale | 9–12 | Optional embedding hybrid (RRF); sync jobs; federation to hosted indexes for long tail; benchmark per [benchmark-protocol.md](benchmark-protocol.md) | Hybrid improves Hit@3/recall without unacceptable latency; cost/query ≤$0.20 frontier-class at 1k queries (GLMP Ch.9's CLI+local modeled cost at frontier pricing — the model and its assumptions live in [interfaces.md](interfaces.md) §1; Phase 0's gate is to reproduce that number on this harness); protocol gates PASS on CLI-vs-MCP comparison |
| 4 — Optional Code Mode | 13+ | Sandbox per [security-licensing.md](security-licensing.md) §4; typed facade | Sandbox escape/exfiltration suite passes; code mode wins cost-per-verified-success or p95 on multi-hop tasks without correctness/security regression |

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Platform absorption (tool search + Agent Skills auto-manage tool surfaces) | CLI adapter immune by construction; **the index — not the protocol — is the moat** (GLMP Ch.12) |
| Local index goes stale silently | last-verified metadata in every response; velocity-stratified syncs; golden-question regression gates before re-indexed content ships |
| BM25 underperforms on semantic queries | FTS5-first with embeddings as an optional rung (contextual-retrieval measurements: −49% failures, −67% with reranking — GLMP L2; see [retrieval-design.md](retrieval-design.md) §3.2) |
| Benchmark gaming in own evals | execution-based scoring, held-out golden questions, blinded grading ([benchmark-protocol.md](benchmark-protocol.md)) |
| llms.txt fragmentation | three-tier L1 ingest degrades gracefully to repo clones and crawl |
| Free-tier/hosted pricing moves | local corpus is structurally immune; hosted providers are optional fallback only |
| Cold-start economics unquantified | explicit cold-miss UX (§ above); estimate from pilot, not invented numbers |

## Existing alternatives / competitive boundary

The version-correct official-docs-corpus gap is **contested, not unowned** (GLMC Finding 6; GPT2 §6.3, anchor E11). A 2025–2026 wave targets it directly — a capability scan, not a quality leaderboard, whose function is to narrow the differentiation claim:

| Public system | Observed scope (GPT2 E11 pinned self-descriptions, except where noted: mcpdoc archive date + GitMCP/DeepWiki descriptions per GLMC Finding 6; Docfork per GLMP §7) | Overlap with this design | Gap to test, not assume |
|---|---|---|---|
| arabold/docs-mcp-server (**Grounded Docs**, MIT) | local SQLite index; CLI, MCP, and web UI; websites, repos, packages, local files; separate versions per library; full-text + optional vectors | High overlap with local ingestion, versioning, adapters | compare provenance, deletion, licensing controls, retrieval quality, cold-start cost |
| LangChain `mcpdoc` (archived 2026-09-18) | user-supplied llms.txt sources + URL fetch; domain allowlisting | strong overlap with index-driven retrieval | narrower corpus scope; verify versioning, offline indexing, provenance |
| Ref MCP | hosted/stdio MCP; search + URL read; session-aware filtering/compaction | progressive disclosure + hosted access | source/version guarantees, private-source governance, offline operation |
| `neuledge/context` | local MCP over a community package registry | packaged docs + MCP adapter | corpus breadth, revocation provenance, licensing |
| GitMCP / DeepWiki (Cognition) / Docfork | GitMCP: GitHub-repo docs over MCP, no freshness/versioning claims; DeepWiki: AI-generated repo wikis with line-level citations; Docfork: 9,000-library MIT-licensed index | adjacent (code-grounded or breadth supply) | good for code-grounded answers, not version-pinned official docs |
| Context7 | hosted documentation catalog and MCP/API/CLI surfaces | **direct reference system** (GPT2 §6.3's own fifth row, reproduced here for completeness — its internals, pricing, and documented criticisms live in [context7-internals.md](context7-internals.md)) | public/private boundary, reproducibility, deployment-specific governance |

GLMC's framing: several of the differentiation items this bundle claims (provenance on every span, deterministic refresh, per-version pinning) are **already partially implemented by Grounded Docs** [61]. A practitioner built a local-first alternative specifically in response to the January 2026 free-tier cut (GLMC Finding 6). GPT2's rule: before implementation, run the same capability manifest and benchmark against at least one local and one hosted competitor — if the target is only a local version-aware index, Grounded Docs and `neuledge/context` are direct alternatives, not merely adjacent tools.

## Why this beats the alternatives

Not a Context7 clone (the value is the index, not the MCP interface — [context7-internals.md](context7-internals.md)); not a wide MCP server (surface area is a measured cost center — [interfaces.md](interfaces.md) §1); not a hosted-service rebuild (the commodity floor is moving under that business — free-tier history, free alternatives at scale). The durable position is the layer everyone else treats as a compatibility checkbox: **a high-quality, eval-scored, version-pinned local retrieval core with an interface portfolio** (GLMP Ch.12). One core, three adapters, and the interface becomes a property of the runtime environment rather than a permanent architectural commitment.
