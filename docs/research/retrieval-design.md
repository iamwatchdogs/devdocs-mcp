---
type: reference
title: Retrieval Design — Why Docs Help, and How to Retrieve Them Well
description: The causal evidence that documentation retrieval improves code generation, the degradation studies that bound context size, and the concrete retrieval contract (hybrid search, structured chunks, symbol-first levels, citations, budgets, fallback, version pinning).
tags: [retrieval, rag, hybrid-search, chunking, citations, version-pinning, context-rot]
timestamp: 2026-10-27T19:00:00Z
resource: docs/research
sources: [DS, DSV, GLMC, GLMP, GPT1, GPT2, KIMI, GEM]
---

# Retrieval Design

The binding constraint in docs tooling is **retrieval quality, not protocol choice** — ToolRet (7.6k tasks, 43k tools): even strong IR models perform poorly at tool retrieval, and low retrieval quality directly degrades task pass rate (KIMI §4). Two further grounding benchmarks make the same point from different directions (KIMI §4): **RULER** (long-context effective length) shows models degrade well before their nominal context limits — stuffing full docs is worse than targeted retrieval — and **ORD-QA** (IEEE/ACM ICCAD 2025) shows domain-docs QA benchmarks need customized RAG (fine-tuned embeddings + reranker) to beat off-the-shelf pipelines: generic chunk+embed is not enough for vertical docs. This file: (1) the evidence that docs help, (2) the evidence that big context hurts, (3) the contract the evidence supports.

## 1. Why documentation retrieval works

| Study | Finding | Design consequence |
|---|---|---|
| Gorilla (arXiv:2305.15334) | retrieval-grounded API calling lets a 7B model surpass GPT-4 on API-call generation; substantially mitigates hallucination | docs-in-context beats parametric memory for library APIs |
| DocPrompting (ICLR 2023, arXiv:2207.05987) | retrieving official docs before generation: +2.85 pass@1 (52% relative) on CoNaLa, up to +6.9 exact match on Bash tldr | retrieval of *official* docs is load-bearing; motivating fact — libraries outgrow any training cutoff |
| CodeRAG-Bench (NAACL 2025 Findings, arXiv:2406.14497) | oracle documents yield significant pass-rate gains; **real retrievers leave a wide oracle gap**, especially at low lexical overlap | retrieval quality is the bottleneck; evaluate retrieval separately from generation |
| When LLMs Meet API Documentation (arXiv:2503.15231) | 1,017 APIs, 4 less-common Python libs: retrieval improved less-common API usage ~83–220%; **code examples contribute more than prose or parameter lists**; BM25 effective; ideal-target retrieval ≫ noisy top-5 (57–76% vs 74–91% pass) | treat examples as first-class retrieval objects |
| CloudAPIBench (arXiv:2407.09726) | 622 AWS/Azure tasks: GPT-4o low-frequency API validity 38.58% → 47.94% with docs; **selective** retrieval +8.20pp overall; unselective/low-precision retrievers dropped high-frequency APIs 39.02pp | selective retrieval helps; naive retrieval can hurt — never dump |
| ContextBench | 1,136 issue-resolution tasks measuring file/symbol/span-level recall, redundancy, AUC coverage | evaluate the retrieval *process*, not only the final answer |

Applicability boundary (GPT2 §5.6): these studies evaluate unfamiliar-library completion and API invocation. They do not establish that shortest-context-wins for migration research, tutorials, incident response, or long-form design. Compact is a **delivery mode, not a correctness guarantee** — expose detail/budget controls and evaluate per task category.

*Table sources: DocPrompting via GLMC Finding 6; Gorilla via KIMI §4; CodeRAG-Bench, API-docs (arXiv:2503.15231), ContextBench via GPT1/GPT2 §5; CloudAPIBench via GPT2 §5.1–5.2 (framing follows GPT2's correction of GPT1).*

Compact-formats evidence (CloudAPIBench ablation, StarCoder2-3B, oracle condition — not the GPT-4o numbers; GPT2 §5.2, correcting GPT1's framing):

| Context | Avg tokens | Valid API invocation |
|---|---:|---:|
| API name only | 36 | 52.73% |
| API description | 79 | 53.22% |
| API specification | 52.57 | 86.82% |
| Description + specification | 95 | 87.14% |
| Full documentation | 685.24 | 88.75% |

The API specification alone is ~92% shorter than full documentation at near-equal validity (86.82% vs 88.75%) — signature + constraints ≈ full page for API-invocation tasks. The evidence supports token-efficient *near-equivalence*, not "outperformance" of full pages.

## 2. Why big context hurts

| Study | Finding |
|---|---|
| Context Rot (Chroma, 2025) | 18 models: performance degrades non-uniformly with input length even at constant task complexity; **topically-related distractors** (what real docs are made of) degrade far worse than irrelevant filler |
| Lost in the Middle (arXiv:2307.03172) | positional bias: GPT-3.5 multi-doc QA 75.8% (relevant doc first) → 53.8% mid-context (setting's nadir 52.9%) — below its 56.1% closed-book score |
| Self-Route (EMNLP 2024, arXiv:2407.16833) | routing to RAG-first-with-LC-fallback: −39–65% cost at −0.2 to −2.2% accuracy; >50% (up to 82%) of queries answerable from small retrieved chunks alone |
| Retrieval meets Long Context (arXiv:2310.03025) | 4K-context model + retrieval matches finetuned 16K-context model |
| BEIR (arXiv:2104.08663) | BM25 is the robust zero-shot baseline; dense retrievers tuned on one domain often underperform it out-of-domain; rerankers best on average |
| Context7 ZK experiment (DS §7 Phase 0) | adding Context7 to 10 framework questions reduced answer quality 73.8% → 59% (8/10 worse) — the only measured end-to-end datapoint that a bad retrieval layer degrades final answers; retrieval can hurt if it returns the wrong material — measure end-to-end answer correctness, not just retrieval success |

Design consequence: **return few, well-placed, version-correct chunks with source citations — never dump the manual** (GLMC Finding 6). Long context still wins sometimes (Self-Route shows fully-resourced LC slightly more accurate on average); the cost delta buys that. Counter-hypothesis to respect: frontier models may already cover popular-library APIs, shrinking retrieval's marginal value — but agents can't know *which* recalled details are stale; provenance-carrying retrieval is the only checkable channel (GLMC Finding 6).

**The naive alternative must be ruled out explicitly (GLMP §2).** For a small corpus — under roughly 200,000 tokens (~500 pages) — Anthropic's contextual-retrieval paper itself observes the best design may be **no retrieval at all**: put the entire corpus in the prompt and let prompt caching absorb the cost, cutting latency by more than half and input costs up to 90% for repeated prefixes. For a personal tool covering ~20 libraries, llms-full.txt files plus prompt caching is a legitimately competitive baseline, and a build decision should state why it loses (GLMP §2). It loses on three measurable grounds as scope grows: cost scales linearly with corpus size on every cache miss; fresh content still has to be selected per task or the model drowns (context rot applies to docs exactly as to anything else); and version pinning becomes manual. The architecture in [architecture.md](architecture.md) is therefore justified by the 50-to-500-library case, where selective retrieval beats both naive stuffing and per-question web fetching.

*Table sources: Context Rot, Lost in the Middle, Self-Route, Retrieval-meets-LC, BEIR via GLMC Finding 6; Context Rot also in DSV §4(a).*

## 3. The retrieval contract

### 3.1 Hybrid retrieval, not vector-only

Compose: **exact identifier matching + BM25/full-text + dense retrieval + reranking**, with source authority and version match as ranking features (all sources; DS §4.4, GPT2 §8.1, GEM §6). Rationale: exact identifiers are critical for API names (dense misses them); BM25 handles technical terms (and is the BEIR-grounded default); dense covers natural-language paraphrase; rerank fixes ordering. Redis Docs MCP production eval (DS §4.4):

| Configuration | MRR | Hit@1 | Hit@3 | Recall@8 |
|---|---:|---:|---:|---:|
| Vector only | 0.816 | 0.75 | 0.88 | 0.93 |
| Hybrid, title field, RRF fusion | 0.836 | 0.71 | **0.96** | **0.99** |

Hybrid improved Hit@3 +0.08 and Recall@8 +0.06 at 0.5–0.7ms p50 retrieval latency. Grep-vs-vector: a 116-question study found grep generally higher accuracy in experiment 1, with harness/tool-calling style dominating; a frontier model with grep-only matched a tuned pipeline (0.61) but took 5× longer; optimized agentic retriever 0.65 in ~5s (DS §4.4) — exact/lexical first for symbols, hybrid for prose. GEM §6's local reference engine runs embeddings as a SQLite user-defined function (Ollama `nomic-embed-text` or Qwen3-Embedding-0.6B, ~250MB auto-downloaded, zero API cost) and supports flexvec-style negative score modulations (`suppress:deprecated`, `suppress:v13`) applied to the L2-normalized embedding matrix before top-k selection. Local vector-store alternatives to hosted databases: `sqlite-vec`, LanceDB, or pgvector (DS §5).

### 3.2 Structure-aware chunks

Chunk units should contain (GPT2 §8.2, GEM §6): heading + breadcrumb, API signature, parameters + constraints, code example, short explanation, version/source metadata — GEM's reference implementation uses ~400-token chunks with 80-token overlap, breadcrumb hierarchy prepended so code is never orphaned from its caveats; FTS5 with Porter or unicode61 tokenization for BM25 ranking; vec0 partition keys per library+version with binary quantization cut storage ~32× and accelerated queries ~10×, keeping brute-force KNN under 75ms past 100k vectors (GEM §6). Never fixed-size-only — a signature separated from its example is less useful than a slightly larger structured unit. Contextual chunk headers cut retrieval failures 49% (67% with reranking) per Anthropic's contextual-retrieval measurements (GLMP L2). AST/tree-sitter symbol extraction from source is 8×–50× cheaper in context than reading whole files (DS §4.3); named prior tools demonstrating the pattern: `agrep`, `livedocs`, `semtree` — extracting signatures, types, and doc comments from source (DS §4.3). Pipeline order: structured docs first (OpenAPI, JSON Schema, TypeDoc, Sphinx, MkDocs, Mintlify Markdown, llms.txt) → source AST second → prose retrieval third (DS §4.3).

### 3.3 Three retrieval levels (progressive disclosure)

Single "query docs" encourages dumping. Expose levels (DS §4.2, GLMP, aider repo-map precedent — 1k-token budget, tree-sitter graph ranking; GEM §7 operationalizes the same three tiers at ~150 / ~200–400 / ~500–1,500 tokens, where tier 2 returns the type signature, parameter table, and one canonical code block with narrative prose stripped, and tier 3 filters against `session_trajectory` and spills overflow >1,500 tokens to a workspace file):

1. **Index level** — headings, symbols, page metadata (very cheap)
2. **Section level** — relevant heading + its prose/code
3. **Symbol level** — exact signature, params, defaults, deprecation, since-version, examples

### 3.4 Version pinning: the differentiator

- Resolve versions **from the project, not model memory**: `package.json`/lockfile, `pyproject.toml`, `go.mod`, `Cargo.toml` (DS §4.1). Concrete publisher precedents (GLMC Finding 6): Next.js pins `@doc-version: 16.4.3` in its llms.txt; freeCodeCamp's DevDocs maintains per-version, offline-downloadable corpora with monthly rebuilds. GEM §6 goes further: a pre-agentic hook parses the lockfile before the first LLM turn, verifies local index partitions, and injects a ~150-token manifest of pre-indexed library IDs — eliminating the resolve round-trip entirely (execution strategy restated in GEM §7).
- Never silently serve `latest` when a version was requested; return explicit fallback metadata: `requested_version`, `served_version`, `version_status` (GPT1/GPT2 §7.1).
- Version-availability policy, fixed *before* eval (DSV §5.1): exact-match → correct; absent → **refuse and name nearest versions** (silent substitution = wrong); alias (`latest`, `v2`) → resolve per declared rule and state it; migration question → cite **both** source and target versions.
- Migration questions are the query type hosted aggregators serve worst — crawl-time indexes chase HEAD (GLMC Insight 3).

### 3.5 Citations: generated by retrieval, never written by the model

Redis Docs MCP's lesson: the model fabricated a URL one path segment off; they stopped letting the model write citations (DS §4.5). Contract:

- Every result carries stable `doc_id`/`evidence_id` + `source_url` + version + commit + `content_hash` + `fetched_at`.
- Model cites `[doc_id]`; server validates every returned ID exists in the index; `fetch(id)` resolves only IDs actually returned by `search`.
- Citation scoring: **precision** = citations that entail the claimed fact and point to the authorized snapshot/version; **coverage** = required claims with ≥1 acceptable citation (GPT2 §11).

### 3.6 Budget the response; disclose truncation

- `max_tokens` parameter; `detail=brief|standard|full` (an exposed response_format enum cut one example 206 → 72 tokens — DSV §4(b)); `truncated: true` + `next_cursor` when cut; never silently drop the most relevant snippet; Claude Code caps tool responses at 25k tokens (DSV §4(b)). **The default is contested across sources — record it, don't average it**: DS §4.6 suggests a "sane default of 2,000–4,000"; GEM §7's reference implementation defaults to **1,000** (explicitly "vs Context7's 5,000"); GLMP recommends a **1,000–2,000 ceiling enforced server-side**. Pick per deployment profile and state it — no source reconciles the spread. Precedents for server-side budgeting: Microsoft Learn's MCP server exposes a `maxTokenBudget` query parameter that truncates content to fit (DS §4.6), and Cloudflare's Code Mode server caps final responses at roughly **6,000 estimated tokens**, marking truncated responses explicitly (DS §2; KIMI §2).
- Tool output hygiene: return high-signal fields only; drop `uuid`/`mime_type`-class noise; namespace and consolidate verbs (`search_docs`, not `list_*` + `get_*` chains) (DSV §4(b)). Structured JSON or compact Markdown over raw text dumps cuts per-call tokens 60–92% (GEM §4).
- MCP tool descriptions: 97.1% of 856 surveyed tools had ≥1 "description smell"; better descriptions improved success +5.85pp but increased steps +67.46% and regressed 16.67% — write **compact, discriminative descriptions and measure**, don't write longer ones (GPT1/GPT2 §5.5).

### 3.7 Fallback ladder (explicit, confidence-gated)

Ordered escalation, entered only when `confidence` is low or `miss_reason` says the index doesn't cover the package/version (DS §4.7):

1. Local exact symbol index
2. Local hybrid docs index
3. Official documentation page fetch
4. Official GitHub source search
5. General web search — clearly marked lower authority

Stop-guard: Context7's own tools tell the model to stop after three calls and use the best result — a useful guardrail against endless retrieval loops. A cold miss should return explicit `not_indexed`/`pending` with a sync hint, not silently scrape an untrusted URL (GPT2 §6.4). An abstention is a valid result; **a source that cannot satisfy provenance/licensing/freshness gates is an abstention, not a successful retrieval** (GPT2 §6.4).

### 3.8 Evidence result shape

Unified core operation (GPT1/GPT2 §7.1, DS §3; full signed-evidence crypto profile in GPT2 §7.1; adopt progressively):

```json
{
  "results": [{
    "ref": "docs://next.js/v16/proxy#rewrite",
    "title": "Proxy rewrite",
    "section": "Routing > Proxy",
    "snippet": "...",
    "examples": [{"language": "tsx", "code": "..."}],
    "source_url": "https://...",
    "source_commit": "<commit>",
    "requested_version": "16",
    "served_version": "16.2.9",
    "version_status": "verified",
    "content_hash": "<sha256>",
    "license_status": "approved_snippet",
    "confidence": 0.94,
    "token_count": 142
  }]
}
```

Never let the model write URLs; never serve latest silently; include `miss_reason` so the agent knows when to escalate.

Session dedup: track served chunk IDs per session and exclude repeats so refined queries page into fresh material instead of re-injecting identical text. Ref ships this as trajectory-aware filtering — an average 5,500 tokens saved per request, up to 95% reduction vs Context7 (GEM §2) — and GEM §6 replicates it locally for free with a `session_trajectory` table in the same SQLite file.

## 4. The one-request fast path

Two-call `resolve` → `query` is the legacy flow; Context7's unified search (select up to 4 libraries, parallel retrieval, dedup, global rerank, version status) should be the **normal fast path**, with explicit resolution reserved for ambiguity (GPT1/GPT2 §2.5). Also halves metered calls on any hosted provider ([context7-internals.md](context7-internals.md)).

## 5. Agent-side workflow rules

The Context7 MCP rules are directionally correct (GPT1 §9): library name + task intent; prefer exact library IDs; use version-specific IDs; **one concept per query**; split unrelated concepts into separate calls; language as ranking preference; cap repeated calls (~3 per question).

Full-loop pattern (GPT1 §9): detect project dependency/version → search one concept → rank → compact snippets → open exact source only if needed → verify signature + version → write code → run tests → re-query only after concrete mismatch → preserve citations.
