# Context7-Style Developer Documentation for Agentic Workflows

## Executive conclusion

**Decision scope:** for a developer-owned, shell-capable environment, the **provisional implementation hypothesis** is **a local-first documentation retrieval core with a CLI as the default interface, a small/lazy MCP adapter for hosts without shell access, and code mode for multi-step documentation workflows**. This is not a post-benchmark architecture recommendation and is not a universal claim that CLI is superior to MCP.

The hypothesis is conditional on the workload and deployment boundary:

1. Retrieval quality comes from indexing, versioning, ranking, provenance, and freshness—not from MCP itself.
2. CLI interfaces avoid eager tool-schema costs and naturally support progressive disclosure.
3. MCP can be valuable for remote authorization, structured access, and multi-user governance when paired with an authenticated gateway and tenant controls.
4. Code mode is valuable for loops, comparisons, batch retrieval, filtering, and citation verification.
5. Documentation retrieval should return compact signatures and examples first; full pages should be loaded only when necessary.

These are design hypotheses with different evidence strength. The benchmark numbers below are source-reported, study-specific results; they do not isolate protocol effects or prove that compact context is optimal for every documentation task. A production decision requires the matched, frozen-snapshot benchmark in §11 and the security/operations gates in §10. Until those gates pass, the report intentionally makes no general interface recommendation.

This report evaluates Context7’s public architecture, MCP versus CLI, code-mode MCP, developer-documentation ingestion, and agentic workflow design. Vendor benchmarks are separated from academic and community evidence. The evidence freeze and traceability rules are in §2.6.

---

## 1. Research scope and evidence policy

### Questions evaluated

- How does the Context7 MCP server work internally?
- What is public versus private in the Context7 implementation?
- When is MCP preferable to a CLI?
- When does code-mode MCP improve developer-documentation workflows?
- How should a Context7-like CLI be designed?
- How should official documentation be represented and supplied to agents?
- What benchmark protocol avoids misleading CLI/MCP comparisons?

### Evidence labels

- **Academic**: peer-reviewed publication, conference paper, or research artifact/preprint.
- **Vendor/engineering**: controlled benchmark or engineering report from a company building the system.
- **Community**: Hacker News, open-source project, or practitioner report; useful for failure modes, but not proof of general performance.

Benchmarks are not pooled into one universal ranking. Token accounting, cache behavior, models, task complexity, and failure handling differ substantially between studies. Only within-study deltas are compared.

---

## 2. How Context7 works internally

### 2.1 Publicly confirmed MCP architecture

The public Context7 repository contains the MCP adapter. Its README states that the supporting API backend, parsing engine, and crawling engine are private.

The current MCP surface exposes two tools:

```text
resolve-library-id
query-docs
```

The public source maps them to:

```text
resolve-library-id
  → GET /api/v2/libs/search

query-docs
  → GET /api/v2/context
```

The MCP server supports:

- stdio and HTTP transports
- API keys and OAuth
- stateless HTTP handling
- read-only/idempotent tool annotations
- static tool-list cache hints
- 60-second upstream request timeout
- query redaction guidance
- advisory guidance to make at most three documentation calls per question (not a server-enforced protocol limit)
- explicit single-concept query guidance

The MCP server is therefore primarily a transport and policy adapter, not the indexing system itself.

References:

- [Context7 repository](https://github.com/upstash/context7)
- [MCP server source at E1](https://raw.githubusercontent.com/upstash/context7/d9561600ad502cd72ab8bae4f9172f434aadf4d1/packages/mcp/src/index.ts)
- [MCP API proxy source at E1](https://raw.githubusercontent.com/upstash/context7/d9561600ad502cd72ab8bae4f9172f434aadf4d1/packages/mcp/src/lib/api.ts)
- [Context7 MCP rules at E1](https://raw.githubusercontent.com/upstash/context7/d9561600ad502cd72ab8bae4f9172f434aadf4d1/rules/context7-mcp.md)

### 2.2 Catalog and retrieval behavior

Upstash’s engineering article says the Context7 library catalog uses Redis Search with:

- exact and stemmed fields
- weighted title and description matching
- popularity and verification signals
- source-type filters
- trust scores
- version metadata
- more than 110,000 indexed libraries at publication time

The article documents the library catalog, not every detail of the document retrieval index. Context7’s API documentation separately confirms that document retrieval supports vector search, LLM reranking, and a fast mode that skips reranking (E15).

The likely public evidence-backed flow is:

```text
repository / website / OpenAPI / llms.txt
        ↓
parser and crawler
        ↓
documentation, examples, metadata, versions
        ↓
sparse/vector retrieval
        ↓
reranking
        ↓
bounded snippets with source references
```

References:

- [How Redis Search powers Context7](https://upstash.com/blog/how-redis-search-powers-context7)
- [Context7 API guide](https://context7.com/docs/api-guide)
- [Search documentation in one request](https://context7.com/docs/api-reference/search/search-documentation-in-one-request.md)
- [Get documentation context](https://context7.com/docs/api-reference/context/get-documentation-context.md)

### 2.3 Ingestion and source configuration

Context7’s dated library-ingestion documentation says it indexes Markdown, MDX, reStructuredText, text, notebooks, OpenAPI specifications, websites, and `llms.txt` sources. It generally prefers documentation files over raw source code and falls back to source-derived documentation when little documentation exists (E13).

The same vendor documentation says `context7.json` supports:

- included folders
- excluded folders and files
- branch selection
- project title and description
- agent rules
- previous versions
- branch versions

This is a strong pattern: documentation owners can declare what is authoritative and what should be excluded from agent retrieval.

References:

- [Adding libraries](https://context7.com/docs/adding-libraries)
- [Library owners and `context7.json`](https://context7.com/docs/library-owners)
- [Keeping libraries fresh](https://context7.com/docs/library-updates)

### 2.4 Freshness

Context7’s published refresh thresholds are popularity-based:

| Popularity rank | Refresh threshold |
|---|---:|
| Top 100 | 1 day |
| Top 1,000 | 15 days |
| Top 5,000 | 30 days |
| Other libraries | 45 days |

It also supports automatic background refresh on request and manual refresh. The current published page says website libraries use a slightly higher threshold than the table above and that a website refresh replaces the active version only after a crawl-success check. A local implementation should support scheduled and event-driven refreshes, but must publish a complete generation atomically as specified in §10.1.

These thresholds are a dated description of the published Context7 policy, not a recommended universal freshness SLA. They were observed on 2026-10-27 (E8); a production system should choose thresholds from source criticality, update frequency, and stale-answer risk, and should make the policy configurable.

### 2.5 Current two-call design versus one-call search

The traditional MCP flow is:

```text
resolve library → query documentation
```

Context7’s newer REST search endpoint can perform a unified lookup in one request. Its dated API documentation says it can select up to four libraries, retrieve documentation in parallel, deduplicate, globally rerank, and report version status (E15).

For a new tool, the one-request path should be the normal fast path. Explicit resolution should remain available for ambiguous libraries or workflows that need the user to choose a source.

### 2.6 Evidence freeze and claim traceability

This report uses an evidence freeze of **2026-10-27**. “Current” means observed on that date; it does not mean permanently true. High-precision claims are tied to the following audit anchors:

| ID | Claims supported | Evidence anchor | Reproduction status |
|---|---|---|---|
| E1 | Public MCP adapter, tool names, endpoint mapping, transports, and policy code | `upstash/context7`, `master` at commit `d9561600ad502cd72ab8bae4f9172f434aadf4d1` | Immutable repository commit |
| E2 | Context7 efficiency benchmark questions and harness artifacts | `upstash/context7-efficiency-benchmark`, `master` at commit `fdd9969f21e216d0d99b6011f72350e79a098dbc` | Immutable repository commit; the pinned repository does **not** contain the published per-run result tables, so those figures are not attributed to E2 |
| E3 | Context7 versus web-search experiment and headline percentages | Published 2026-05-27; retrieved 2026-10-27; normalized-capture SHA-256 `c6d5d163c4ee5e407f843385a548d6929bb51d9049ababe057fc03594fabd6d3` | Dated vendor page plus capture digest; not a native immutable archive |
| E4 | `llms.txt`/Markdown serving benchmark, denominator, exclusions, and confidence procedures | `mintlify/docs-url-discovery-bench` at commit `5f055a89336cb94050e36eaa479374b390e92462`; pinned raw `RESULTS.md` SHA-256 `834db268ca77dba11d2051b66e2a2b235ed714909858e7165f0648e13df27b6c`; normalized main-page capture SHA-256 `7a103d7606df3c5ff9c9c8aff979f0338a43e8ba4476ec3264d9d75cd51a0138` | Immutable repository commit and committed result data |
| E5 | Context7 API endpoints, version fallback behavior, and request accounting | API guide retrieved 2026-10-27; normalized-capture SHA-256 `80b4fe3c08392e60e2aa9bfd87fe25fda159cf7cbc98c307a050d1c96a5f1585` | Dated vendor capture; reverify before implementation |
| E6 | Redis Search catalog fields, weighting, filtering, and reranking/fast-mode description | Published 2026-06-24; retrieved 2026-10-27; normalized-capture SHA-256 `0f0be6f73294509506e8635241eb4632d1268c7bf50bd393ea2c9d00ba7545ad` | Dated vendor page plus capture digest |
| E7 | Academic API-documentation and retrieval findings | `arXiv:2503.15231v1`, `arXiv:2407.09726v1`, ACL Anthology `2025.findings-naacl.176`, and DOI `10.1145/3858625` | Stable identifier; applicability limits are stated in §5.6 |
| E8 | Published plan prices and refresh thresholds | Plans/pricing and library-updates pages retrieved 2026-10-27; normalized-capture SHA-256 `2cb09ad8c4841ba257da53c83b872c5c7f104b4a9d0d3fd9782de4047bb99edd` and `7dbf4d05b2d0cc26b7c38bd9cf98b4efe7aaa369a20a454ccd12e1d58a67e634` | Mutable vendor pages; treat as date-stamped estimates and reverify before budgeting |
| E9 | Context7 architecture-change benchmark and context-bloat claims | `new-context7` page retrieved 2026-10-27; normalized-capture SHA-256 `282c8900f2afb976f66e755e849665f44b78a54d11bbf5af89f7c116a1695c1d` | Dated vendor capture; no independent raw-result artifact claimed |
| E10 | CLI/MCP/code-mode comparison figures | Scalekit capture SHA-256 `c77148385876588595c05e4fc7ed3107ffad43c516f0acd2282d15c4a6b2f84c` and Port of Context capture SHA-256 `e47deddb14d4a1c8f2a65e0a4d33d5df6ca91f7aa0ea02498008b0875e8c2177`, both retrieved 2026-10-27 | Dated vendor/practitioner captures; small, confounded studies, not protocol estimates |
| E11 | Public competitor capability scan | `arabold/docs-mcp-server` at `4e35baa7e82c3ec447405c48a301c8a4185e715f`; `langchain-ai/mcpdoc` at `8d01c08598e3f19fd6318bded3ffdcda85db03a4`; `ref-tools/ref-tools-mcp` at `f5feb829cc7bdd18fb9b276c27e3f391b2c4132c`; `neuledge/context` at `690422b717afc5b73249de7a770742e6dcc3620a` | Immutable README/repository anchors; feature claims are self-descriptions, not independent quality evidence |
| E12 | Academic paper/project pages and ContextBench metrics | Retrieved 2026-10-27; normalized-capture SHA-256: API-docs `8929f840428d0209ce73850b6eb80b7aaee773886991ae76c0b1d7ae3ff6b2c6`, CloudAPIBench `42678eb764d69f7f045f8ef054429d34b630ee951791d539d65866acf69bb1bd`, ContextBench `bcfd4eb7d6cfc3dcd7c8bea06528450b365586b22fb049fd59b3a1aa8e45e27f` | Versioned paper captures/project capture; use exact sections/tables cited in §5 |
| E13 | Context7 ingestion, privacy, and safety claims | Retrieved 2026-10-27; normalized-capture SHA-256: adding libraries `2d18595be3cc26291883898080be9867e10290c65c27a8720888fa92f7367135`, data safety `c01347f2f22b1b068cdbb7bd14afa3e222ebd0184527353150380c90b5125451`, data privacy `db24871efd0e13528c1d2cbebc59aa21ca0c89a2c60e444c81e4665291cbe73d`, library owners `636e4f9ce6e27d97289a0ebfbb22aba846da73ced6012c948a36ba2e1d118886` | Dated mutable vendor captures; claims are vendor-described controls, not independent assurance |
| E14 | GitHub MCP toolsets, read-only/lockdown modes, and scope filtering | `github/github-mcp-server` at commit `f38526264abb14961b32cea386e38ec6d3cf674f`; README/config captures SHA-256 `371e8c991df6e5e61ca6a864d692141dbf1a728ae0790d376b54c96b91e4ff13` and `528f8bf095e2a84fda928e1f5bbddb22e647113e9671f424c31f52e17c4a9d19` | Immutable repository commit |
| E15 | Context7 unified search and context retrieval behavior | Search/context pages retrieved 2026-10-27; normalized-capture SHA-256 `853533b680684fcc2d4cb8ebc075e739af176c96ba16afb6b6a742ed371a6ee4` and `02d98fd2885b23f9c5de504a0abbd864230e082fd05afe807ae3c77d47b51b74` | Dated mutable vendor captures; reverify before implementation |

The normalized captures used for these hashes are retained at `/private/var/folders/9j/fmw5m6110hz9n8l4d20n37vr0000gn/T/context7-agentic-docs-evidence/` with filenames matching the evidence IDs. The capture procedure is: fetch the URL through Jina Reader, preserve the returned UTF-8 bytes unchanged, and run `shasum -a 256`. The directory is local, not a public archive; copy it to an approved durable evidence store if this report is promoted. The digests are audit checks for the retrieved text, not a claim that a mutable web page has become immutable. For a reproducible architecture decision, E1, E2, E4, E11, E14, and the retained E3–E10/E12–E15 captures must be inspected or re-run; every future revision must record the retrieval date, source revision or content digest, and the exact table/passage used for any changed numerical claim.

### 2.7 Ingestion, licensing, and redistribution gate

Documentation being public does not automatically grant permission to crawl, cache, embed, or redistribute it. Every source enters a registry with:

| Registry field | Required decision |
|---|---|
| Publisher/repository identity and authentication method | Expected host, signed/known repository identity where available, short-lived credential broker, tenant owner for private sources |
| License and terms | SPDX/license URL or “unknown”; whether indexing, snippets, embeddings, and redistribution are allowed |
| Robots and access policy | `robots.txt`, API limits, crawl-delay, ToS restrictions, and an approval record for exceptions |
| Retention and output mode | Transient fetch, private local cache, tenant-only cache, or shareable index; attribution and notice requirements; whether embeddings, reranker/provider caches, model prompts, and generated excerpts may persist |
| Revocation/deletion | Contact/trigger, SLA, tombstone propagation, backup/replica deletion, and audit evidence |

The registry is an enforcement state machine: `pending_review → approved_fetch → approved_index → approved_embed → approved_cache → approved_snippet/redistribute`, with `denied`, `expired`, and `revoked` terminal/revocation states. Each source version and every derivative artifact (normalized text, chunk, embedding, reranker input/cache, snippet, attribution/notice bundle, and model/provider transmission) carries the license/terms decision and deletion dependency. Each transition records an owner, evidence, scope, expiry, and deletion action; refresh re-evaluates the state before fetching, embedding, or publishing output. Default behavior is **deny** when license, ToS, robots, publisher identity, or provider retention terms are unknown; a documented legal exception may permit transient/no-redistribution access only and must expire. Open-licensed documentation may be indexed subject to attribution and license conditions. Private sources require explicit owner authorization and tenant-scoped retention. A hosted shared corpus requires legal review of source terms, derivative embeddings, provider/model retention, attribution, and output behavior; this report is not legal advice.

---

## 3. Context7 pricing and cost model

Context7’s published plans, observed on 2026-10-27, state:

| Plan | Included usage |
|---|---:|
| Free | 1,000 API calls/month |
| Pro | 2,000 calls/seat/month |
| Pro overage | $5 per 1,000 calls |
| Private repositories | $5 per 1M parsed tokens |
| Self-hosting | Enterprise |

The two-step MCP lookup consumes approximately two API calls:

```text
resolve-library-id + query-docs = 2 calls
```

Therefore, 1,000 calls can represent roughly 500 uncached two-step lookups. A unified search endpoint halves that request consumption.

Hosted Context7 is attractive when the value is in:

- broad public-library coverage
- managed crawling
- automatic freshness
- private repositories
- trust and safety filtering
- OAuth and team governance

A local alternative is attractive when the requirements are:

- predictable cost
- offline operation
- private source control
- custom ranking
- local caching
- no external query transmission
- strict version and retention control

Reference: [Context7 plans and pricing](https://context7.com/docs/plans-pricing)

Prices and included quotas are mutable commercial terms. Use them only for an order-of-magnitude example, recheck them before procurement, and include the local implementation’s storage, operations, egress, and refresh costs in the TCO model below.

---

## 4. Normalized benchmark evidence

These studies measure different scenarios. The numbers below preserve each study’s baseline and task definition rather than pretending they are one common leaderboard.

| Study | Scenario | Within-study result | Evidence |
|---|---|---:|---|
| Context7 versus web search | 100 documentation questions using the same Claude Agent SDK | Context7 reported the **mean of five category-level reductions** as 36.81% fewer total tokens, 34.56% lower cost, 98.98% fewer fresh input tokens, and 22.77% fewer tool calls | Vendor benchmark E3; arithmetic reconciliation in §4.1 |
| Context7 old versus new architecture | 80+ coding questions across popular libraries including Next.js, FastAPI, Playwright, tRPC, and Docker | Approximately 9.7k → 3.3k context tokens, 24s → 15s latency, and 3.95 → 2.96 tool calls | Vendor benchmark E9; no raw per-run result artifact claimed |
| GitHub CLI versus MCP | 75 scheduled arm attempts (25 each) using the same Claude model, prompts, and five read-only tasks | MCP used 4–32× more tokens; transport/text completion was 25/25 for CLI and 18/25 for MCP, with 7 MCP connection timeouts; this is not an answer-correctness result | Vendor benchmark E10 capture; harness commit not independently pinned |
| Stripe CLI versus raw MCP versus code mode | 12 Stripe tasks; all arms passed all tasks | CLI: 711,555 tokens; raw MCP: 506,970; code mode: 294,924 | Vendor benchmark E10 |
| Documentation URL discovery | 2,400 valid attempts: 100 questions × 2 agents × 4 serving arms × 3 repetitions across 20 sites; see §4.4 and E4 | In the reported HTML→`md-link` contrast, adding a linked `llms.txt` index reduced input tokens about 41–60% and reduced 404s close to zero | Reproducible vendor benchmark E4 |
| API documentation RAG | 1,017 APIs across four less-common Python libraries | BM25 top-5 documentation raised average pass rates from roughly 18–41% to 57–76%; ideal target documents reached roughly 74–91% | Academic preprint |
| CloudAPIBench | 622 AWS/Azure API invocation tasks | GPT-4o low-frequency API validity rose 38.58% → 47.94% with documentation; a suboptimal/low-precision retriever harmed high-frequency APIs by 39.02 percentage points | Academic research |
| MCP tool descriptions | 856 tools across 103 MCP servers | In the MCP Universe benchmark, all-component description augmentation found 97.1% with at least one smell; median success improved +5.85pp, median execution steps increased +67.46%, and 16.67% of cases regressed | ACM research |

### 4.1 Context7 versus web search

Context7’s five-category benchmark used 20 questions per category and the Claude Agent SDK. The reported averages were:

| Metric | Web search | Context7 | Reported change |
|---|---:|---:|---:|
| Total tokens | 136,562.10 | 84,278.20 | −36.81% |
| Fresh input tokens | 38,402.84 | 386.70 | −98.98% |
| Output tokens | 3,289.93 | 1,543.88 | −52.97% |
| Tool calls | 3.47 | 2.55 | −22.77% |
| Cost | $0.22 | $0.14 | −34.56% |

The most important result is not that Context7 “uses fewer tokens” in the abstract. It is that page-level web retrieval produced much more fresh context, while Context7 returned focused documentation slices.

Reference: [Context7 versus Claude Code web search benchmark](https://upstash.com/blog/context7-vs-web-search-benchmark)

**Arithmetic reconciliation.** The source’s bold “Avg” row is the arithmetic mean of the five category-level percentage reductions. It is not the percentage obtained by dividing the displayed overall mean totals. Using the rounded overall means shown above gives different checks:

| Metric | Source-reported mean of category reductions | Check from displayed overall means | Interpretation |
|---|---:|---:|---|
| Total tokens | 36.81% | 38.29% (`1 - 84,278.20 / 136,562.10`) | Both are valid summaries of different aggregation levels; use the source-reported value only when describing its benchmark |
| Fresh input tokens | 98.98% | 98.99% (`1 - 386.70 / 38,402.84`) | Rounding-level difference |
| Output tokens | 52.97% | 53.07% (`1 - 1,543.88 / 3,289.93`) | Rounding/aggregation difference |
| Tool calls | 22.77% | 26.51% (`1 - 2.55 / 3.47`) | Aggregation difference; do not present 22.77% as a ratio of the overall means |
| Cost | 34.56% | 36.36% (`1 - 0.14 / 0.22`) | Displayed dollar means are rounded to cents; do not infer a cost ratio from them |

The report therefore makes no claim that these two aggregation methods should agree. A future independent replication must publish per-question or per-category paired data, the aggregation formula, and unrounded values. Until then, the source-reported figures are descriptive vendor results, not independently recomputed estimates.

**Cache sensitivity.** The benchmark’s fresh-input contrast is cache-miss dependent. Let `F` be fresh input tokens, `h` the fraction of otherwise repeated input served as cache-read, and `p_f`, `p_c`, and `p_w` the provider’s fresh/cache-read/cache-write rates. The billed input component is:

```text
billed_input(h) = F × ((1 − h) × p_f + h × p_c) + cache_write_tokens × p_w
```

Using the source’s displayed mean `F` values, the fresh component alone would be:

| Assumed hit rate h | Web-search fresh component | Context7 fresh component | What this does not include |
|---:|---:|---:|---|
| 0% | 38,402.84 | 386.70 | Cache reads/writes, output, tool schemas, and service cost |
| 50% | 19,201.42 | 193.35 | Same exclusions; provider rates still determine dollars |
| 95% | 1,920.14 | 19.34 | Same exclusions; high hit rates can reduce the dollar gap |

This is a sensitivity illustration, not a reanalysis of the vendor result. Reported decisions must measure hit/miss state and provider rates; they must not treat the 98.98% fresh-input reduction as a cache-invariant total-cost reduction.

### 4.2 GitHub CLI versus MCP

Scalekit compared a shell-based CLI, CLI plus an approximately 800-token skills file, and GitHub’s official MCP server using 43 tool schemas over 75 scheduled arm attempts (25 per arm). The reliability subtest reports transport/text completion, not semantic task correctness.

Reported median tokens per task:

| Task | CLI | MCP | MCP multiplier |
|---|---:|---:|---:|
| Repository language and license | 1,365 | 44,026 | 32× |
| PR details and review status | 1,648 | 32,279 | 20× |
| Repository metadata and install | 9,386 | 82,835 | 9× |
| Merged PRs by contributor | 5,010 | 33,712 | 7× |
| Latest release and dependencies | 8,750 | 37,402 | 4× |

The benchmark also reported 7 connection timeouts in 25 MCP runs. This reliability result is important, but it is not purely a protocol result: it includes a remote server and network path. Treat the 25/25 versus 18/25 figures as observed completion/reliability behavior, not proof that one interface answered tasks correctly.

Reference: [Scalekit MCP versus CLI benchmark](https://www.scalekit.com/blog/mcp-vs-cli-use)

### 4.3 Code mode

Port of Context compared three Stripe integrations using Claude Sonnet 4.6:

| Approach | Total tokens | Cost | Average/task |
|---|---:|---:|---:|
| CLI | 711,555 | $2.22 | 59,296 |
| Raw MCP | 506,970 | $1.60 | 42,248 |
| Code-mode MCP | 294,924 | $0.98 | 24,577 |

For a multi-step invoice task:

| Approach | Model steps | Tokens |
|---|---:|---:|
| CLI | 19 | 497,556 |
| Raw MCP | 12 | 168,480 |
| Code mode | 4 | 38,847 |

This supports code mode for loops and dependent calls, but not necessarily for a single documentation lookup.

Reference: [CLI versus MCP versus Code Mode](https://www.portofcontext.com/blog/cli-vs-mcp-vs-code-mode)

### 4.4 Documentation delivery and `llms.txt`

Mintlify’s benchmark used **2,400 valid attempts**: 100 questions × 2 agents × 4 serving arms × 3 repetitions. Equivalently, each agent/arm cell has n=300. The full run retained 2,672 rows: the 2,400 valid attempts plus 272 failed attempts that the retry loop reran until each cell reached n=300. The failures were reported as 256 Codex CLI crashes, 7 timeouts, 6 Claude network errors, and 3 Claude contamination-gate failures. The primary tables exclude those failed attempts; they remain inspectable in the committed data.

The published accuracy table is therefore **conditional on valid attempts**. The source does not publish a failure-adjusted accuracy table in the cited passage, so this report does not manufacture one; the correct all-launched estimate requires the committed row-level outcomes. Under the protocol in §11, an initial crash/timeout is a quality failure and retries cannot refill the primary denominator.

For Claude:

```text
HTML input:     296k tokens
Markdown-link:  175k tokens
```

For Codex:

```text
HTML input:     285k tokens
Markdown-link:  115k tokens
```

The HTML→`md-link` contrast changes both the serving format and the presence of a linked index, so its 41–60% input-token reduction is a combined-format result, not an index-only causal estimate. The source’s index-only Markdown→`md-link` contrast reduced input tokens from 215k to 175k for Claude (about 19%) and from 156k to 115k for Codex (about 26%); the source warns that cross-agent token accounting is not comparable. Accuracy was approximately 94–99% conditional on valid attempts; inlining the entire index did not justify its additional token cost.

Reference: [Mintlify Docs URL Discovery Benchmark results at E4](https://github.com/mintlify/docs-url-discovery-bench/blob/5f055a89336cb94050e36eaa479374b390e92462/RESULTS.md)

The benchmark’s denominator is therefore auditable, but its result should still be interpreted narrowly: it measures navigation/content-negotiation behavior on 20 Mintlify sites and exact-path grading, not documentation-answer correctness or a general CLI-versus-MCP protocol effect.

### 4.5 Study quality and confounder register

The following matrix prevents a within-study result from being misreported as a protocol law:

| Study | What is actually compared | Major confounders | Safe claim | Required follow-up |
|---|---|---|---|---|
| Context7 versus web search | Two retrieval implementations in the Claude Agent SDK | Vendor-owned Context7 service, web-search implementation, model/tool routing, caching, prompts, task selection, and source freshness | The reported Context7 setup used fewer tokens/cost on these 100 questions | Re-run with frozen documentation, matched tool budgets, identical model/tool routing, cold/warm cache arms, and paired task-level data |
| Context7 old versus new architecture | Two Context7 implementations on 80+ coding questions | Vendor task corpus, architecture changes beyond interface, retrieval/index changes, and possible measurement changes | The newer Context7 configuration was better on its own corpus | Do not attribute the delta to MCP/CLI; publish the harness and per-task data |
| GitHub CLI versus MCP | Shell workflow versus a remote MCP configuration | 43 schemas, remote network, server implementation, allowlists/toolsets, authentication, output formatting, and small n=25 | The tested CLI configuration had lower measured context use and fewer observed timeouts | Compare CLI, narrow MCP, and local MCP with equivalent capabilities and network paths |
| Stripe CLI versus raw MCP versus code mode | Three integrations for 12 Stripe tasks | Integration quality, task choice, model, code-mode runtime, and all-pass ceiling effect | Code mode used fewer tokens in that integration benchmark | Replicate on documentation tasks with failure-sensitive grading and matched capabilities |
| Mintlify `llms.txt` benchmark | Four serving formats over frozen site snapshots | Exact-path grading, curl-only fetching, model/CLI differences, retry exclusions, and site-specific content | Linked indexes reduced navigation friction in that corpus | Preserve full transcripts, report site-clustered intervals, and test answer quality separately |

No row supports the statement “MCP is inherently expensive” or “CLI is inherently more reliable.” The supported statement is narrower: a particular implementation and configuration produced a measured difference under a stated workload. The new benchmark in §11 is the decision gate for this design.

### 4.6 Role and uncertainty of the existing numbers

Most vendor and practitioner rows above publish point estimates rather than a complete uncertainty analysis; the Stripe study also reports an all-pass ceiling on its 12 tasks. The retained E4 capture includes Mintlify’s confidence intervals and site-clustered tests, but this summary does not reproduce every source statistic; where a source does not publish uncertainty, no interval is implied. These figures are directional priors used to select hypotheses and test sizes, not evidence that an implementation has passed the acceptance rule in §11.3.

---

## 5. Academic findings relevant to the design

### 5.1 API documentation materially improves unfamiliar-library coding

The paper [When LLMs Meet API Documentation, v1 / E12](https://arxiv.org/html/2503.15231v1) studied Polars, Ibis, GeoPandas, and Ivy, with 1,017 eligible APIs.

The authors found:

- retrieval improved less-common API usage by approximately 83–220%
- code examples contributed more than descriptive prose or parameter lists
- BM25 was effective for the task
- ideal target-document retrieval substantially outperformed noisy top-5 retrieval
- documentation quality and example diversity mattered

This strongly supports a documentation index that treats examples as first-class retrieval objects.

### 5.2 Compact API specifications can approach full-page performance with fewer tokens

CloudAPIBench measured API hallucinations across 622 synthetic AWS/Azure tasks.

For GPT-4o:

- low-frequency API validity was only 38.58% without documentation
- documentation increased it to 47.94%
- documentation with a suboptimal/low-precision retriever could damage high-frequency API performance
- the paper’s GPT-4o DAG++ strategy—triggering retrieval from API-index lookup or low invocation confidence—improved the overall CloudAPIBench score by 8.20 percentage points in its reported configuration; the paper also reports a 39.02-point high-frequency drop for unselective DAG, with one retrieved document at 50% retriever precision illustrating the distraction mechanism. These are not generic guarantees for every retriever or task mix

The paper also reports a separate context-format ablation using StarCoder2-3B and an oracle/target-document retrieval condition; the following numbers must not be attributed to the GPT-4o DAG/DAG++ results above:

| Context | Average tokens | Valid API invocation |
|---|---:|---:|
| API name only | 36 | 52.73% |
| API description | 79 | 53.22% |
| API specification | 52.57 | 86.82% |
| Description + specification | 95 | 87.14% |
| Full documentation | 685.24 | 88.75% |

In this specific API-invocation setting, the API specification was about 92.3% shorter than full documentation (52.57 versus 685.24 average tokens) while reaching 86.82% versus 88.75% valid invocation. The evidence supports token-efficient near-equivalence for this task, not “outperformance” of full pages.

References:

- [CloudAPIBench paper, v1 / E12](https://arxiv.org/html/2407.09726v1)
- [ICSE-SEIP DOI](https://doi.org/10.1109/ICSE-SEIP66354.2025.00027)

### 5.3 Code retrieval remains difficult

[CodeRAG-Bench](https://aclanthology.org/2025.findings-naacl.176/) describes six programming tasks across four categories, five document sources, ten retrievers, and ten language models in the cited paper. Public project summaries may use a different task count; this report uses the paper’s taxonomy here.

Its central finding is that high-quality retrieved context helps, but:

- retrievers often fail to find useful documents
- generators do not always use retrieved context effectively
- retrieval quality and generation quality must be evaluated separately

### 5.4 Evaluate the retrieval process, not only the final patch

[ContextBench](https://euniai.github.io/ContextBench/) reports 1,136 issue-resolution tasks from 66 repositories and eight programming languages (E12 capture). It measures:

- file-level recall and precision
- symbol-level recall and precision
- span-level coverage
- edit-location coverage
- trajectory-aware redundancy
- AUC coverage over time

This is a useful model for evaluating a documentation agent: success alone hides whether the agent found the right evidence efficiently or merely guessed correctly.

### 5.5 Tool descriptions can become a measurable failure source

The ACM study [MCP Tool Descriptions Are Smelly](https://doi.org/10.1145/3858625) examined 856 tools across 103 MCP servers.

Reported findings:

- 97.1% contained at least one description smell
- 56% failed to clearly state their purpose
- description augmentation improved median task success by 5.85 percentage points
- median execution steps increased by 67.46%
- 16.67% of cases regressed

The implication is not “write longer tool descriptions.” It is “write compact, discriminative descriptions and measure behavior.”

### 5.6 Applicability boundaries

The academic evidence does not establish one universal “compact-first” ranking policy. The API-documentation studies primarily evaluate unfamiliar-library code completion and API invocation; they show that examples, API specifications, and selective retrieval can help under those task definitions. They do not directly evaluate migration research, conceptual tutorials, incident response, legal/compliance documentation, or long-form design work.

Use the evidence as task-specific hypotheses:

| Task type | Initial retrieval hypothesis | What must be tested before adoption |
|---|---|---|
| Exact API lookup or code completion | Signature + constraints + a representative example first | Invocation validity, executable tests, and citation precision |
| Natural-language how-to | Structured snippet first, then page expansion | Answer completeness and omitted prerequisites |
| Migration/version comparison | Retrieve both pinned versions and change/migration pages | Version correctness, change recall, and stale-answer rate |
| Conceptual/tutorial or operations guidance | Do not assume the shortest context is best | Human-rated completeness, safety, and source coverage |
| Security-sensitive or private documentation | Provenance and access controls take priority over compactness | Unauthorized-content rate, injection resistance, and audit completeness |

The implementation should therefore expose `detail`/budget controls and evaluate compact versus expanded context by task category. “Compact” is a delivery mode, not a correctness guarantee.

## 6. MCP, CLI, and code mode: scenario-specific decision

### CLI is best when

- the agent runs locally
- the developer is the user
- the task is a one-off or read-heavy lookup
- the agent already has shell access
- context efficiency is important
- local caching is useful

Advantages:

- no eager tool catalog
- progressive disclosure through `--help`
- straightforward caching
- easy filtering and piping
- broad compatibility with coding agents

Risks:

- ambient credentials
- weaker default auditability
- text parsing variability
- installation and PATH management

### Raw MCP is best when

- the agent cannot use a shell
- the service is remote
- OAuth and tenant isolation matter
- calls require structured audit records
- explicit tool boundaries are important

Risks:

- eager schemas consume context
- poor output design causes follow-up calls
- remote transport failures
- excessive or overlapping tools confuse selection

The official GitHub MCP server supports toolsets, individual-tool allowlists, read-only mode, and lockdown mode. Therefore, a fair comparison should test both its default/all-tool configuration and a narrowly filtered configuration.

Reference: [GitHub MCP Server](https://github.com/github/github-mcp-server)

### Code mode is best when

- several documentation queries must run in parallel
- many pages must be filtered or deduplicated
- versions must be compared
- raw pages are much larger than the final answer
- the workflow includes loops or dependent calls
- the agent needs to verify citations or signatures

Code mode is probably unnecessary for:

```text
“How do I use React useEffect cleanup?”
```

It is valuable for:

```text
“Compare the authentication APIs in Next.js 15 and 16, verify three examples, and cite the exact migration guidance.”
```

References:

- [Anthropic: Code execution with MCP](https://www.anthropic.com/research/code-execution-with-mcp)
- [Anthropic: Advanced tool use](https://www.anthropic.com/research/advanced-tool-use)
- [Cloudflare: Code Mode](https://blog.cloudflare.com/code-mode/)

### 6.1 Scenario-weighted interface decision

The interface choice should be made per deployment, not by averaging incompatible studies:

| Scenario | Default candidate | Why | Non-negotiable controls |
|---|---|---|---|
| Local developer or coding agent with shell access | CLI | Progressive disclosure, simple caching, pipes, and low integration overhead | Least-privilege credentials, structured output, redacted logs, deterministic version selection |
| Remote host without shell access | MCP | Native tool boundary and typed request/response path | Authenticated gateway, tenant isolation, quotas, audit logs, narrow tool allowlist |
| Multi-user governed service | MCP or authenticated API | Central policy and identity can be valuable | The concrete gateway must provide identity, authorization, audit, retention, and isolation; MCP alone does not provide these |
| Offline/private repository | Local CLI/core | Avoids external query transmission and gives retention control | Encrypted local index, source ACLs, deletion workflow, no ambient credentials |
| Batch comparison, citation verification, or multi-step research | Code mode over the core | Parallel calls and local filtering reduce repeated model turns | Sandboxed execution, bounded resources, explicit network policy, provenance-preserving synthesis |
| Single short lookup from a host that already has a stable MCP integration | Raw MCP may be adequate | Avoids adding a separate CLI installation | Small schema, bounded output, timeout/retry policy, measured context overhead |

The phrase **CLI-first** in this report means “build and evaluate the retrieval core behind a shell-friendly interface first for the local developer scenario.” It does not mean “replace MCP in every deployment.” A change of scenario changes the default candidate.

### 6.2 Total-cost-of-ownership model

For a real choice, record the following instead of comparing token counts alone:

```text
TCO(workload) = index build + storage/egress + service/operations
                + model input/output + cache effects + reliability/retry cost
                + security/governance cost + migration/maintenance cost
```

Measure the terms separately for cold and warm caches, and report both cost per successful task and cost per attempted task. Count failure retries, operator time, authentication/gateway operation, index refresh, and retention/deletion work. A hosted MCP service may be lower-TCO even when its tool schema costs more tokens; a local CLI may be lower-TCO even when it shifts installation and operations to the developer. The decision matrix and the benchmark quality gates, not a single token metric, determine the selected interface.

Use fixed formulas in the benchmark:

```text
cost_attempt_i = Σ(token_class_i × frozen_rate_class)
                 + embedding_i + reranker_i + transport_i + service_i
C_attempts     = Σ_i cost_attempt_i             # initial attempts + retries, once
C_fixed        = allocated index/operations/security cost over a declared horizon
C_workload     = C_attempts + C_fixed
per_run        = C_attempts / planned run rows
per_task       = C_attempts / planned unique tasks
per_success    = C_workload / completed unique tasks
```

`C_attempts` includes each retry exactly once; retries are not also added to `C_fixed`. A completed unique task is defined in §11.3 as at least 3 of 5 preregistered runs passing; zero completed tasks makes `per_success` undefined/infinite. Freeze provider price cards, cache-read/write rates, embedding/reranker rates, exchange-rate assumptions, fixed-cost allocation horizon, and operator-rate convention before collection. Report model-only, service/infra, fixed-cost, per-run, per-task, and per-success values separately; never mix a billed-token estimate with an unpriced TCO claim.

### 6.3 Competitive boundary scan

The following is a capability scan, not a quality leaderboard. It narrows the differentiation claim: a new system must demonstrate a measurable advantage rather than assume that “local + version-aware” is novel.

| Public system | Observed scope at E11 anchor | Overlap with proposal | Apparent gap to test, not an assumption |
|---|---|---|---|
| Grounded Docs MCP Server | Local SQLite index; CLI, MCP, and web UI; websites, repositories, packages, local files; separate versions; full-text plus optional vectors | High overlap with local ingestion, versioning, and adapters | Compare provenance, deletion, licensing controls, retrieval quality, cold-start cost, and sandbox/tenant profiles |
| `mcpdoc` | User-supplied `llms.txt` sources and URL fetch tool; explicit domain allowlist behavior | Strong overlap with index-driven documentation retrieval and SSRF boundary | Narrower apparent corpus/index scope; verify versioning, offline indexing, provenance, and benchmark behavior |
| Ref MCP | Hosted/stdio MCP; search and URL-read operations; session-aware filtering/compaction | Overlap with compact progressive disclosure and hosted access | Compare source/version guarantees, private-source governance, reproducibility, and local/offline operation |
| `neuledge/context` | Local MCP backed by a community package registry; downloads packages and queries locally | Overlap with local packaged docs and MCP adapter | Compare corpus breadth, update/revocation provenance, CLI usability, and source licensing |
| Context7 | Hosted documentation catalog and MCP/API/CLI surfaces | Direct reference system | Public/private boundary, reproducibility, and deployment-specific governance remain the target of this report |

This scan does not claim these systems have or lack a feature beyond their pinned self-descriptions. Before implementation, run the same capability manifest and benchmark against at least one local competitor and one hosted competitor. If the target is only a local version-aware index, Grounded Docs and `neuledge/context` are direct alternatives, not merely adjacent tools.

### 6.4 Cold-start, storage, and abstention economics

Estimate economics from a pilot rather than inventing universal per-library numbers. For each library/version, record normalized source bytes `B`, chunk count `N`, embedding dimension `D`, bytes per vector component `w`, postings/metadata overhead `P`, replicas `R`, and refresh delta `ΔB`:

```text
index_storage      ≈ R × (B + N × D × w + P)
refresh_bytes      = ΔB
refresh_compute    = changed_chunks × (parse + embed + index work)
refresh_cost       = fetch_cost(ΔB) + refresh_compute_cost
query_cost         = retrieval + rerank + transport + model context cost
```

The implementation must define a cold-start policy: prioritize sources by declared dependency, request frequency, version churn, and trust/license status; queue authorized on-demand ingestion; return an explicit `not_indexed`/`pending` result rather than silently serving latest or scraping an untrusted URL synchronously; and provide a bounded direct-fetch fallback only for an approved source. Measure time-to-first-index, storage per version, refresh bandwidth, embedding cost, queue latency, and stale/abstention rate. A source that cannot satisfy provenance, licensing, or freshness gates is an abstention, not a successful retrieval.

### 6.5 Deployment profiles and phase gates

Keep these profiles separate:

| Profile | Required before release | Explicitly out of scope |
|---|---|---|
| Local single-user CLI/core | Source registry, local cache/index, safe parser worker, version/provenance contract, local secret policy, deletion and refresh tests, protected per-user paths (0700 directories/0600 files), symlink-safe locking, OS-keystore/KMS-backed encryption where secrets/private docs exist, and shared-workstation detection | Multi-tenant claims and code-mode execution; fail closed if the storage boundary cannot be established |
| Hosted/private MCP/API | All local controls plus identity/tenant authorization, brokered credentials, residency/retention, atomic generations, SLO/RTO/RPO, tamper-evident audit, quota and recovery tests | Anonymous shared ingestion |
| Code mode | Hosted or local profile plus the §10.2 sandbox backend, brokered capability, escape/exfiltration tests, fail-closed platform detection, and independent resource quotas | Running on a host that lacks the required isolation primitives |

Phase gates are profile-specific. Phase 1 can ship a local core without code mode; Phase 3 can add an MCP adapter only after the intended local or hosted profile passes; Phase 4 cannot ship until the code-mode gate passes. A target that cannot satisfy a profile’s controls must abstain from that deployment mode, not silently downgrade its security contract.

---

## 7. Recommended architecture

```text
                    ┌──────────────────────┐
                    │ Versioned sources    │
                    │ Git / web / OpenAPI  │
                    └──────────┬───────────┘
                               ↓
                    ┌──────────────────────┐
                    │ Parser + normalizer  │
                    │ headings/examples/API│
                    └──────────┬───────────┘
                               ↓
                    ┌──────────────────────┐
                    │ Hybrid index         │
                    │ exact + BM25 + dense │
                    └──────────┬───────────┘
                               ↓
                    ┌──────────────────────┐
                    │ Rerank + budget      │
                    │ version/provenance   │
                    └──────────┬───────────┘
                               ↓
              ┌────────────────┼────────────────┐
              ↓                ↓                ↓
           CLI adapter      MCP adapter       Code mode
```

### 7.1 Core retrieval operation

Start with one unified operation. In a private deployment, the gateway—not the model or CLI arguments—must attach a non-forgeable request context containing the authenticated actor, tenant, authorization scope, and audit correlation ID:

```text
docs.search(
  query,
  library?,
  version?,
  language?,
  limit?,
  max_tokens?,
  detail?
)
request_context = {
  actor_id, tenant_id, authorized_source_scope, authz_epoch, scope_fingerprint, audit_id
}  // server-derived; never model-controlled
```

`scope_fingerprint = HMAC(K_scope[key_id], canonical-CBOR(scope_envelope_v1))`, where `K_scope[key_id]` is the actual secret resolved from the server KMS—not the public key ID. The typed envelope includes tenant, actor/service principal, policy version, allow and deny rules, canonical source IDs/ACL versions, operation/purpose, residency/data-class entitlements, version entitlements, and authorization epoch. Authorization runs before candidate generation and before every cache/provider hit; physical or logical index partitions filter by scope before ranking, with post-hit filtering only as defense in depth. Derived indexes, caches, embeddings, and citation bundles include tenant, scope fingerprint, generation, and authorization epoch in their keys. `open_doc`, `verify_doc`, `add`, `sync`, CLI commands, and MCP calls use the same boundary; a caller cannot select a tenant by supplying a field in the query. If authorization, revocation, KMS, or audit dependencies are unavailable, the operation fails closed. Internal denial reasons are kept in audit records; external responses use one indistinguishable `not_available` shape with bounded/padded result counts, metadata, bytes, cache behavior, and timing for unauthorized versus absent resources, and private source URLs/titles/versions are redacted unless authorized.

Return structured evidence:

```json
{
  "results": [
    {
      "evidence_id": "ev1.<signed-opaque-reference>",
      "title": "Proxy rewrite",
      "section": "Routing > Proxy",
      "snippet": "...",
      "examples": ["..."],
      "source_url": "https://...",
      "source_commit": "<immutable-git-commit>",
      "parser_version": "<parser-version>",
      "source_snapshot": "<immutable-snapshot-id>",
      "requested_version": "16",
      "served_version": "16.2.9",
      "version_status": "verified",
      "content_hash": "<sha256>",
      "span": {"start": 1200, "end": 1342},
      "license_status": "approved_snippet",
      "attribution": ["<required-notice>"],
      "redistribution": "snippet_only",
      "token_count": 142
    }
  ]
}
```

Never silently serve latest documentation when a version was requested. Return explicit fallback metadata.

`evidence_id` is an opaque, signed reference binding tenant/scope fingerprint, generation, source snapshot, parser/normalizer version, canonical byte span, content hash, requested/served version, nonce, expiry, and audience. `open_doc` and `verify_doc` require and verify that reference (or fail closed); a bare mutable URL is not sufficient. Canonical names are `library`, `evidence_id`, `requested_version`, and `served_version` across the core, CLI, MCP, and code-mode adapters. Internally, adapters record stable reason codes such as `not_indexed`, `version_not_found`, `unauthorized`, `source_untrusted`, `stale`, and `budget_exceeded`; externally, unauthorized and absent resources use only the indistinguishable versioned `not_available` error shape to avoid existence or timing oracles. `allow_fallback` defaults to `false`; if explicitly enabled, the response must include the reason and served snapshot in the authorized audit/result context.

The default signature profile is **Ed25519 over versioned canonical CBOR** with domain separation string `devdocs/evidence/v1`, explicit algorithm/key ID, audience, tenant/scope fingerprint, actor, generation, nonce, expiry, and authorization epoch fields. The canonical-CBOR profile, field ordering, byte-string encoding, and test vectors are part of the versioned schema; JSON is not an alternate signing encoding. Trust roots and publisher attestations are managed in the source registry; repository identity is verified against the expected owner/repository and commit, while mutable websites require an approved domain/owner attestation or remain `source_untrusted`. Key rotation, revocation, signer identity, rollback rejection, and `verify_doc` re-reading the pinned generation artifact and recomputing the span hash are tested with fixtures. Canonical bytes are the post-normalization UTF-8 bytes, and spans are byte offsets into that exact artifact; parser/normalizer changes produce a new snapshot and invalidate prior evidence references. Runtime response policy checks `license_status` before returning an artifact: attribution/notices are emitted when required, mixed-license results are partitioned and labeled, full-document output is blocked when only snippets are approved, and revoked/expired states return `not_available`.

### 7.2 CLI surface

```bash
devdocs add https://github.com/vercel/next.js --version v16.2.9
devdocs sync
devdocs search "rewrite a request in Next.js proxy" --library next.js --version 16 --json
devdocs open docs://next.js/v16/proxy#rewrite --max-tokens 1200 --json
devdocs versions next.js --json
devdocs verify docs://next.js/v16/proxy#rewrite
```

Required CLI properties:

- JSON output mode
- stable exit codes
- bounded output
- deterministic ordering
- machine-readable errors on stderr
- local cache
- explicit authentication handling
- no secrets in stdout
- `--max-tokens`
- `--fields`
- `--head`
- source/version filters
- stdin or query-file input for sensitive queries; never require secrets in argv or URLs

Context7’s own `ctx7` CLI demonstrates this general pattern with `library`, `docs`, `--json`, setup, project-level configuration, and skills integration.

Reference: [Context7 CLI at E1](https://raw.githubusercontent.com/upstash/context7/d9561600ad502cd72ab8bae4f9172f434aadf4d1/packages/cli/README.md)

### 7.3 MCP adapter

Expose only a small set of tools:

```text
search_docs
open_doc
verify_doc
```

Avoid exposing many overlapping operations such as separate library lookup, page search, section fetch, example fetch, signature fetch, and comparison tools unless evaluations show that the separation improves behavior.

For remote deployments, use an authenticated gateway with POST bodies or equivalent non-URL query transport, redacted request/response tracing, short-lived brokered source credentials, server-derived tenant context, and explicit data-residency policy. The adapter must fail closed when the gateway cannot prove authorization or the requested evidence snapshot no longer exists.

### 7.4 Code-mode adapter

Expose a small typed facade that supports parallel retrieval and local filtering:

```ts
const hits = await docs.search({
  query: "proxy rewrite",
  library: "next.js",
  version: "16",
  language: "TypeScript"
})

const pages = await Promise.all(
  hits.slice(0, 3).map(hit => docs.open({ evidence_id: hit.evidence_id, content_hash: hit.content_hash }))
)

console.log(docs.compact(pages, { maxTokens: 1200 }))
```

The sandbox should handle parallel retrieval, filtering, deduplication, version comparisons, and citation assembly. Its `docs` facade is a brokered capability bound to the server-derived request context; it cannot access the host’s credentials or arbitrary tenant data. The main model should receive only the compact final result.

---

## 8. Retrieval design

### 8.1 Hybrid retrieval

Use:

```text
exact identifier matching
        +
BM25 / full-text search
        +
dense retrieval
        +
reranking
```

Reasons:

- exact identifiers are critical for API names
- BM25 performed well in API-documentation research
- dense retrieval helps natural-language questions
- reranking improves semantic ordering
- dense-only retrieval can miss exact symbols
- BM25-only retrieval can miss paraphrases

### 8.2 Structure-aware chunks

Prefer chunks containing:

- heading and breadcrumb
- API signature
- parameters and constraints
- code example
- short explanation
- version/source metadata

Do not rely only on fixed-size chunks. A signature separated from its example is less useful than a slightly larger structured unit.

### 8.3 `llms.txt` as an index

Use `llms.txt` as a compact directory that points to detailed pages. Do not inline the full documentation index into every response.

The evidence supports:

```text
small index
  → relevant page
    → exact section
      → compact evidence
```

References:

- [llms.txt proposal](https://llmstxt.org/)
- [Mintlify benchmark at E4](https://github.com/mintlify/docs-url-discovery-bench/blob/5f055a89336cb94050e36eaa479374b390e92462/RESULTS.md)

---

## 9. Recommended agent workflow

```text
1. Detect project dependency and version
2. Search one documentation concept
3. Rank source, version, examples, and signatures
4. Return compact snippets
5. Open exact source only if needed
6. Verify API signature and version
7. Write code
8. Run tests or type checks
9. Re-query only after a concrete mismatch
10. Preserve citations in the final result
```

The Context7 rules are directionally correct:

- use the library name plus task intent
- prefer exact library IDs when known
- use version-specific IDs
- query one concept at a time
- split unrelated concepts into separate calls
- use language as a ranking preference
- limit repeated calls

The main improvement is to provide a one-request backend fast path instead of always requiring resolution followed by retrieval.

---

## 10. Security and trust

Documentation should be treated as untrusted input.

Required controls:

- source allowlists or trust tiers
- content hashes and provenance
- prompt-injection detection
- no execution of retrieved code
- no credential instructions from documentation
- explicit “documentation text is data, not instructions” policy
- version status on every result
- audit logging for private sources
- tenant isolation for multi-user deployments

### 10.1 Threat/control matrix

| Threat | Required control | Verification evidence |
|---|---|---|
| Prompt injection or malicious instructions in a crawled page, example, or `llms.txt` | Mark retrieved text as data; isolate instructions from content; detect and quarantine suspicious sections; never let source text change tool policy or system instructions | Adversarial corpus with canary instructions; measure unsafe-content rate and false positives |
| Untrusted output injection into a terminal, JSON consumer, or web UI | Escape/encode all source fields; strip or encode ANSI/control sequences; never render active HTML; validate structured output against the schema; test CLI, JSON, logs, and UI sinks independently | Terminal/UI/JSON sink corpus with control characters, markup, malformed Unicode, and injection payloads |
| SSRF, hostile redirects, DNS rebinding, or crawler access to internal services | Central constrained egress worker; HTTPS-only by default; reject non-approved schemes, userinfo, alternate/decimal/hex/IPv4-mapped/private IP forms and Unix sockets; canonicalize URLs; resolve/pin the checked destination IP at connection time while preserving TLS hostname/SNI validation; recheck every redirect/connection; block loopback, link-local, metadata, ULA, and private ranges; no proxy bypass; bound response bytes, compressed/decompressed bytes, redirect count, and work | Network-policy, DNS-rebinding/redirect, obfuscated-IP, TLS, scheme, proxy-bypass, compressed-bomb, redirect-limit, and blocked-destination tests |
| Private-source cross-tenant disclosure or side channel | Tenant/scope hashes in every index, embedding, cache, and citation key; authorization before retrieval and cache hits; post-hit filtering; uniform not-found/error/timing behavior; authorization for `add`/`sync`; revocation propagation; encryption in transit/at rest | Cross-tenant result, existence, count, timing, ingestion, cache-poisoning, and revocation tests plus audit-log review |
| Secrets in source content, queries, logs, or caches | Secret redaction before persistence and output; no credentials in snippets; short-lived least-privilege tokens; configurable retention and deletion | Synthetic-secret corpus, log/cache scans, deletion verification |
| Sensitive user query or private source transmitted to a remote service | Classify query/source content before any egress and redact before embedding/reranking; local-only by default for sensitive/private material; every final LLM, classifier, agent host/SDK, embedding, reranker, proxy, retry, and telemetry path is covered by the same egress decision; remote providers require an allowlist and contractual zero-training/retention, residency, cache, and deletion guarantees; unknown policy fails closed; prefer POST bodies or stdin over URL/argv query transport; disable URL/body tracing; never persist raw queries, rewrites, embeddings, snippets, retries, credential-bearing source URLs, or secret-bearing errors; never use raw sensitive text as a cross-tenant cache key; record only a keyed fingerprint/redacted decision | Synthetic secrets/PII, shell-history/process-list, URL/log/telemetry, final-LLM/SDK-error, embedding/reranker/provider-cache, local-routing, deletion, and remote-egress tests |
| Local storage, backup, swap, or crash-dump leakage | Per-user no-follow ownership/mode checks for SQLite/WAL/journal/temp/index/blob/log paths; encrypted database/blobs/backups, mmap/page-cache spill, and temporary artifacts with OS-keystore/KMS-managed keys; secure temp cleanup and crypto-erasure/key revocation; disable core dumps; isolate credential helpers and shared caches; refuse unsupported/network filesystems unless encryption/locking semantics are verified; test second-user, backup, snapshot, swap, mmap, and crash paths | Filesystem/permission, WAL/temp, backup/snapshot, swap/core-dump, mmap/network-filesystem, key-rotation/deletion, and shared-workstation tests |
| Parser or dependency compromise | Network-disabled dedicated unprivileged parser sandbox with no host mounts/sockets; approved sources are handed off as read-only byte streams or content-addressed files, never as host paths/credentials; disable external XML entities, YAML tags/aliases, OpenAPI external `$ref`, notebook output/execution, Markdown includes, HTML subresources, and recursive link fetching by default; canonicalize/reject hostile URLs; bound archive compressed/decompressed bytes, file count, recursion, depth, CPU, memory, and time; reject path traversal/unsafe symlinks; Git/package ingestion is non-executing with hooks, submodules, smudge/LFS filters, credential helpers, install scripts, and external commands disabled; dependency pinning; quarantine on parse failure | Format-specific adversarial fixtures, malformed-file/archive corpus, URL canonicalization, symlink/submodule/path-traversal tests, hook/filter/script tests, supply-chain inventory, and worker escape tests |
| Code-mode escape or unsafe retrieved examples | Rootless sandbox; drop Linux capabilities; `no_new_privs`, seccomp, namespaces, pinned and provenance-checked image; read-only filesystem by default; no host sockets; no ambient secrets; network disabled by default; explicit brokered egress approval; CPU, memory, process, file, and wall-clock limits | Sandbox escape, exfiltration, fork-bomb, filesystem, capability, image-integrity, and timeout tests |
| Denial of service through expensive queries or refreshes | Per-tenant quotas, concurrency limits, query/result/token budgets, refresh backoff, circuit breakers, and bounded reranking | Load tests with quota and recovery measurements |
| Stale, deleted, or revoked documentation | Source revision/content hash on every chunk; signed or expected publisher/repository identity in the source manifest; enforceable trust tier for mutable websites and user URLs; durable revocation/tombstone ledger checked independently of active generation; generation/scope in every index, embedding, cache, and citation key; deletion propagation to indexes, caches, logs, replicas, and model/provider stores; explicit stale/fallback status | Signature/identity and trust-policy tests, deletion/version-fallback tests, revocation-side-channel tests, and stale-answer measurement |
| Partial refresh, mixed generations, or rollback failure | Build immutable staging generation; validate source counts, hashes, ACLs, parser output, and retrieval health; atomically switch one active-generation pointer through durable CAS/transaction; pin each request to one generation manifest; retain prior generation only if it is not revoked; prohibit rollback across tombstones; define reader grace periods and purge behavior | Kill refresh at every stage, consistency/read-after-publish, rollback, revocation, and deletion propagation tests |

The generation manifest is the read contract: it lists every raw blob, normalized artifact, parser version, ACL/scope map, embedding, index, cache namespace, replica high-water mark, and hash. A read obtains one manifest at admission and all `search`, `open_doc`, and `verify_doc` operations use that generation; dangling or mixed-generation references fail closed. A monotonic external revocation ledger/high-water mark is checked at admission, immediately before every provider/index/cache call, and again at the linearization point before emitting or writing a result. It has precedence over rollback, in-flight completion, replica lag, backups, and downstream provider retention; a revocation race discards the result. Recovery with an RPO that may omit recent tombstones remains fail-closed until the revocation high-water mark is synchronized. Deletion requires acknowledgements from each store or crypto-erasure of the relevant key; the release manifest records the grace period and purge evidence.

### 10.2 Minimum code-mode sandbox contract

Code mode is an optional execution environment, not a trusted extension of the host. A conforming implementation must provide:

1. A documented supported isolation backend and OS matrix (for example, a rootless OCI runtime on supported Linux hosts or a VM-backed equivalent elsewhere). Feature-detect the required primitives at startup and fail closed—disable code mode rather than silently running unsandboxed.
2. A dedicated **rootless** unprivileged process/container or equivalent isolation boundary with dropped capabilities, `no_new_privs`, seccomp, and separate namespaces.
3. A pinned, provenance-checked runtime image, read-only base filesystem, and ephemeral writable workspace.
4. No access to host sockets, metadata services, browser profiles, or ambient environment secrets.
5. A signed, one-time brokered `docs` capability bound to tenant, actor, canonical authorization scope/scope fingerprint, request, sandbox identity, generation, authorization epoch, audience, operation/quota, expiry, and nonce. Consumption is atomic against durable linearizable nonce/revocation state; broker restart, failover, backup restore, replica rollback, reuse across requests/sandboxes/forks/retries, or canceled/permission-revoked sessions is rejected. The capability accepts only the server-derived request context and allowlisted operations, and the sandbox cannot call the index or credential store directly.
6. Network disabled by default; if enabled, an allowlisted proxy with connection-time destination checks and request/response budgets enforced both by the broker and inside the sandbox.
7. Hard limits for wall time, CPU, memory, child processes, file size, output tokens, and concurrent calls.
8. Structured, redacted telemetry with a keyed query fingerprint (never raw query), actor/tenant, source fingerprint, version, hash, decision, and failure identifiers, but never raw secrets.
9. An operator-visible kill switch and deterministic cleanup after cancellation.

Retrieved code examples may be displayed as data, but they must not execute automatically. These controls are acceptance criteria, not optional hardening.

Initial code-mode support is limited to a declared Linux/architecture/runtime matrix with rootless user namespaces, seccomp, cgroups, and the selected OCI backend. macOS, Windows, Docker Desktop, ARM variants, and hosts lacking any required primitive are unsupported until a backend-specific VM/isolation implementation and escape-test suite is registered. Startup self-tests verify the matrix and fail closed; there is no “best effort” unsandboxed fallback.

### 10.3 Operational and governance contract

For private or multi-user deployments, define before launch: identity-to-tenant mapping, authorization decision point, retention duration, deletion SLA, audit-log access, incident response owner, quota policy, source revocation process, backup/replica deletion behavior, availability SLO, RTO/RPO, key-rotation schedule, tamper-evident audit-log storage, monitoring/alerting, and index/schema migration/rollback procedure. Also require signed parser/runtime artifacts, an SBOM and vulnerability/patch SLA, key ownership and KMS lifecycle, per-tenant queue fairness/isolation, provider/subprocessor retention and residency contracts, authorization-failure/stale-lag/SSRF/quarantine/revocation dashboards, and exercised incident runbooks/notification paths. Test backup restoration and deletion from replicas before launch. “MCP governance” is only a valid claim when these deployment controls exist outside or alongside the protocol.
The default hosted-profile gate is: 99.9% monthly availability, RTO ≤1 hour, RPO ≤15 minutes, zero fail-open behavior for authorization/KMS/revocation/audit dependencies, source-revocation propagation ≤15 minutes, and a warm local search p95 ≤2 seconds (with cold sync measured separately). Owners are assigned by role before collection: product owner for scope/ToS, security owner for threat gates, platform owner for SLO/restore, privacy/legal owner for retention/residency, and evaluation owner for preregistration/grading. Alert thresholds, evidence retention, and exceptions are recorded in the release manifest; these are proposed gates, not claims that they have passed.

Context7’s vendor documentation describes layered malicious-content detection, source filtering, and policies (E13); these are vendor-described controls, not independent assurance. Community discussion also highlights that `llms.txt` and documentation can eventually become prompt-injection targets.

References:

- [Context7 data safety at E13](https://context7.com/docs/security/data-safety.md)
- [Context7 data privacy at E13](https://context7.com/docs/security/data-privacy.md)
- [Hacker News `llms.txt` discussion](https://news.ycombinator.com/item?id=48410589)

---

## 11. Benchmark plan for a new implementation

### Task categories

1. Known library, known version, exact API lookup
2. Known library, unknown page, natural-language how-to
3. Unknown or niche library discovery
4. Version migration comparison
5. Multi-library comparison
6. Agent coding task with a deterministic verifier

### Arms

```text
A. No retrieval
B. Raw page fetch
C. CLI + help
D. Naïve MCP
E. Filtered/lazy MCP
F. Code mode
G. New hybrid system
```

Default baseline assignment is frozen before collection: A is the retrieval-value baseline; C is the local shell-capable interface baseline; E is the hosted/governed interface baseline; and F is evaluated only for code-mode workflows. G is the candidate hybrid system. A competitor may replace C or E only through a documented pre-collection capability-manifest review; no baseline may be selected after results are observed.

### Quality metrics

- answer correctness
- executable/test pass rate
- citation precision
- citation recall
- requested-version correctness
- stale-answer rate
- unsafe-content rate

### Efficiency metrics

- fresh input tokens
- cached input tokens
- output tokens
- total billed cost
- model turns
- tool calls
- p50/p95 latency
- timeout/failure rate
- retrieved context tokens
- index-build cost

### Retrieval metrics

- Recall@1/5/10
- MRR
- nDCG
- context precision
- context recall
- duplicate context
- unexplained version fallback

### Fairness rules

- same model
- same prompt
- same frozen documentation snapshot
- paired task order randomized with a recorded seed; no arm is systematically run first
- same authentication conditions
- same timeout
- same output budget
- capability parity: same source set, version scope, query allowance, network reachability, retry policy, and result budget; if not possible, label the comparison deployment-realistic rather than protocol-matched
- capability manifest: equivalent operations, concurrency, CLI help/skill content, tool-schema/system-instruction exposure, sandbox/runtime budget, and output semantics; hidden adapter retries or parallel calls are prohibited in the matched arm and disclosed in the realistic arm
- separate index-build cost from query cost
- report cached and uncached costs separately
- default preregistration: 50 unique tasks per category, 5 runs per task/arm, blocked by site and category, with at least 10 independent library/site clusters per category; repeated runs are averaged within task and are not treated as five independent observations. Before collection, simulate the paired task-level estimator for a one-sided −5-point non-inferiority margin at baseline success 0.85, site/library ICC 0.10, 80% power, and α=.05; increase the number of independent tasks/sites if that target is not met
- repeat stochastic agents several times with recorded model/provider/version and seed where supported
- use paired confidence intervals

### 11.1 Capability and cache accounting

Run two explicitly labeled comparisons:

| Comparison | Required parity | Interpretation |
|---|---|---|
| Capability-matched protocol comparison | Same retrieval corpus/backend, source permissions, version scope, query budget, result/token budget, network path, timeout, retry policy, model, output schema, and arm affordances. Constrain shell/filesystem/subprocess/credential access, concurrency, sandbox/runtime, CLI help/skills, tool schemas, and system instructions to an equivalent manifest; only the interface adapter changes | Evidence about adapter overhead under controlled conditions |
| Deployment-realistic comparison | Each arm uses its normal production backend and integration, but all deviations are listed before the run | Evidence about end-to-end choices; cannot be attributed to protocol alone |

For every attempt, record cache state at each layer: model prompt cache, agent/tool-result cache, HTTP/CDN/service cache, retrieval/index cache, and OS/filesystem cache. Use separate cache namespaces per arm, define cold-cache eviction and warm-up requests, log hit/miss status, and prevent one arm from warming another arm’s cache. Randomize cold/warm block order. Use a fresh isolated agent session and fixture per attempt; if a provider cache cannot be independently evicted, isolated, or observed, label its state **unknown**, do not call the run cold, and either use an independently cacheable provider or report a predeclared sensitivity limitation. Report verified-cold, verified-warm, and unknown-state results separately.

### 11.2 Auditable experiment protocol

The benchmark is not complete until another team can reconstruct its denominator and rerun its primary comparison. Publish a manifest containing:

1. **Frozen corpus:** source URL, source revision or retrieval timestamp, commit/tag where available, content hash, parser version, and the exact pages/chunks exposed to every arm.
2. **Task ledger and sampling:** task ID, category, expected answer/API, acceptable citations, requested version, language, difficulty rationale, and whether a deterministic verifier exists. Define the target estimand as the mean success over the declared task population, not an arbitrary mix of prompts. The default target population gives equal weight to the six categories and equal weight to tasks within category; publish any alternative category weights before collection. Build a sampling frame of libraries/sites and tasks, publish stratum quotas and selection probabilities for popularity, version churn, language, documentation format, and task type, and use inverse-probability weights if sampling is unequal. Hold out libraries/sites from tuning and task construction, but make their frozen documentation available through the evaluation fixture during the run; do not give the agent the answer key or task-generation metadata. Keep at least 30 tasks per category; use the 50-task default in the decision rule when possible.
3. **Attempt ledger:** one row for every planned task × arm × run, including model/version, provider, prompt hash, seed if supported, arm, task ID, start/end time, cache state, tool calls, hidden model calls, tokens by fresh/cache-read/cache-write/output class, embedding/reranker calls, cost, outcome, timeout, crash, retry, and grader result.
4. **Randomization:** use a preregistered balanced schedule over the seven arms: assign each task/replicate a pseudorandom permutation of the arm list, rotate the starting position across tasks with a Latin-square schedule, and validate that each arm occupies every order position across the tasks in each category/site block within one assignment. Five repetitions use five distinct rotated positions; across tasks each arm is balanced within one position. Record the PRNG/Latin-square seed. Run attempts serially within a block or record a fixed concurrency window; freeze provider/model versions and include time/service-load blocks. Use a fresh isolated agent session and fixture for every attempt; define cache eviction, prompt-cache behavior, and sleep/warm-up rules before running.
5. **Failure policy:** count every initial launch in the primary denominator. Do not replace a failed attempt with a retry. Report retries and excluded failures separately with reason codes; run a predeclared sensitivity analysis that treats failures as task failures.
6. **Grading:** use a versioned rubric for correctness, executable behavior, citation precision/recall, requested-version correctness, completeness, and unsafe-content handling. A citation is precise only when it entails the claimed fact and points to the authorized source snapshot/version; recall is required claims with at least one acceptable citation divided by all claims requiring citation. De-identify outputs and require blinded grading; strip arm names, tool names, timing, and token counts from grader material. A missing/empty output is a failed response. Calibrate two graders on a held-out set, predefine material-error, citation, and unsafe-severity rules and adjudication; require κ ≥0.80 and raw agreement ≥0.90, otherwise re-calibrate and re-grade the affected block before analysis (no post-result waiver). Exact-path grading must list all acceptable paths.
7. **Statistics:** pre-register primary endpoints, minimum detectable effect, clustering unit, estimator, confidence method, and analysis. Use a paired cluster bootstrap with the task as the sampling unit and site/library as a cluster, or a predeclared randomization test; use a one-sided 95% lower bound (equivalent to a two-sided 90% interval) for the −5-point non-inferiority decision. Report two-sided 95% intervals for descriptive secondary metrics, and correct or label secondary/multiple comparisons. Do not infer general superiority from overlapping or unreported uncertainty.
8. **Contamination and safety:** verify that every network request goes through the intended fixture/proxy; include adversarial documentation, revoked versions, private-source authorization, tenant existence/count/timing side channels, sensitive-query egress, prompt injection, parser-format attacks, SSRF, oversized pages, atomic-generation rollback, data-residency violations, and code-mode escape cases. Disable provider training/online learning where available, isolate provider caches, record retention and residency policy, and use cold-only runs when isolation cannot be verified. Prefer post-cutoff, private, or newly generated holdout material where permitted; record model-training cutoff/provider information, document the remaining contamination uncertainty, and run a sensitivity analysis when contamination is plausible. Do not use benchmark questions or source snapshots from the tuning corpus, and keep task generation, answer keys, and holdout metadata unavailable to the evaluated agent while exposing only the frozen holdout documents needed for retrieval.

The minimum decision report must show raw counts, valid and failed denominators, exclusions, cache state, tool/model versions, per-category results, uncertainty intervals, and the complete reproduction command. A single aggregate token number is insufficient.

### 11.3 Pre-registered scoring and decision rule

Use this default rule unless a replacement is registered before data collection:

For task `t`, let `ȳ_t,a = (1/5) Σ_r y_t,a,r` be the mean of the five initial run outcomes for arm `a`; a retry is not one of these five outcomes. Let `w_t` be the preregistered inverse-probability weight normalized within category. The primary arm estimate is `R_a = (1/6) Σ_category c [Σ_t∈c w_t ȳ_t,a / Σ_t∈c w_t]`, with equal category weights; the paired difference is `R_candidate − R_baseline`. Version correctness, citation precision, citation coverage, and latency use the same task/category weighting with their opportunity-level numerators and denominators. The default interval is a 10,000-resample paired cluster bootstrap: sample library/site clusters with replacement within each category, retain all tasks and five-run means in a sampled cluster, apply the frozen weights, and take the 5th percentile for the one-sided 95% lower bound or 95th percentile for an upper bound. Provider/time blocks are included as a cluster or fixed effect according to the preregistered sampling frame; the choice cannot be made after seeing results. Run a simulation of this exact estimator to verify coverage and power.

- **Primary quality endpoint and estimator:** create one row for every planned task × arm × run. An initial crash, timeout, unauthorized result, unsafe result, or missing required citation is a failure; retries never refill this quality denominator. For executable tasks, success requires the deterministic verifier to pass. For answer tasks, two blinded graders score a versioned rubric: all required claims correct, no material false claim, required citation present and points to the correct source/version, and no unsafe instruction followed. A disagreement is adjudicated; partial-credit scores are secondary. Average the five run-level binary outcomes within each task, then report the equal-category/equal-task weighted mean over the declared task population; confidence intervals operate on task-level values clustered by site/library.
- **Primary safety endpoint:** maintain a safety-opportunity ledger for every initial launch, including final model output, every tool/source response exposed to the model, code-mode/broker output, citation bundle, and every private-source authorization decision; a crash/timeout/no-output remains a quality failure and is never silently removed from the ledger. Separate deterministic adversarial zero-event gates from population-rate estimates. Count one opportunity per response/sink for unsafe-content events and one authorization decision per private-source access attempt for unauthorized-content events; deduplicate multiple manifestations within an opportunity, retain severity, and report opportunity/event counts by sink. Require zero confirmed secret disclosures and zero cross-tenant disclosures. For independent opportunities use a one-sided Clopper–Pearson upper bound; for site/tenant-clustered opportunities use the specified cluster bootstrap. The independent zero-event approximations require at least 299 opportunities for a 1% upper bound and 598 for a 0.5% upper bound; clustered designs require a preregistered effective-sample simulation. If the opportunity count is smaller, the release gate is not evaluable and cannot pass.
- **Primary efficiency endpoint and cost scope:** calculate per-attempt model cost from fresh/cache-read/cache-write/input-output tokens plus embedding/reranker, transport, and hidden model calls. Let `C_attempts` be the sum of every initial and retry attempt; let `C_workload = C_attempts + amortized one-time/index/operations cost`, with no retry double-counting. Report `C_attempts / planned rows`, `C_attempts / successful run rows`, and `C_workload / completed unique tasks`; a completed unique task means at least 3 of 5 preregistered runs pass. If the success count is zero, cost per success is undefined/infinite, not omitted. Report p95 latency separately, including failures as censored/failed under the preregistered rule.
- **Primary comparison:** select and freeze the baseline before data collection: no-retrieval for retrieval value, and a named best existing capability-matched local/hosted arm for an interface choice. Use a one-sided 95% lower bound for the paired task-level success difference; the candidate is quality-non-inferior when it is no worse than **−5 percentage points** and the same declared weighted task-level estimators meet these absolute floors: task success ≥85%; requested-version correctness ≥99% over explicit-version opportunities with an available pinned source (unavailable-version requests are a separately reported fallback/abstention stratum and cannot make the gate evaluable); citation precision ≥95% over emitted citations; and citation coverage ≥90% of answer tasks that require citations. A zero eligible denominator or zero emitted citation does not pass. It is efficiency-superior only when the one-sided upper bound for `C_workload / completed unique tasks` is below **1.0×** the baseline, the point estimate is at least **20% lower**, and the one-sided upper bound for the paired p95 latency ratio is no more than 1.25× the baseline or meets the preregistered scenario SLO; a timeout is recorded as failure with latency set to the declared timeout ceiling. If any safety/quality floor fails, or neither condition is met, report fail/inconclusive rather than PASS.
- **Multiplicity and clustering:** the quality endpoint is primary; category, latency, token, retrieval, and cache analyses are secondary and labeled as such. Bootstrap or randomization intervals are paired by task and clustered by site/source where applicable. A decision is not made from a single favorable category.
- **Power and stopping:** use the preregistered 50-task/category × 5-run default, with task as the sampling unit and site/library as the cluster; verify by simulation the actual one-sided −5-point non-inferiority test under baseline success 0.85, the declared ICC, arm-pair correlation, category weights, and at least 10 clusters/category before collection. Do not stop early based on observed results. If the target is not met, increase independent clusters/tasks or label the result underpowered and do not generalize it.

### 11.4 Benchmark acceptance gates

Do not make a general interface recommendation unless all of these are met:

- **Quality non-inferiority:** the selected arm meets the predeclared correctness and version/citation thresholds against the strongest baseline.
- **Efficiency evidence:** any token/cost or latency advantage survives paired uncertainty analysis and is reported per successful task.
- **Reliability:** timeout, crash, retry, and partial-result behavior are measured, not silently removed.
- **Security:** the threat corpus and sandbox tests in §10 pass the predeclared safety thresholds.
- **Operational viability:** TCO, freshness lag, deletion propagation, quota behavior, and recovery are measured for the intended deployment scenario.

Do not combine token counts from unrelated studies. Compare within the same model, agent, task corpus, and accounting method.

Useful evaluation references:

- [ContextBench](https://euniai.github.io/ContextBench/)
- [CodeRAG-Bench](https://aclanthology.org/2025.findings-naacl.176/)
- [AgentDocsBench](https://pypi.org/project/agentdocsbench/)
- [Zvec agent benchmarks](https://zvec.org/en/docs/zvec-grep/benchmarks/)

---

## 12. Final recommendation

Build in phases:

### Phase 0: evidence and safety gate

- freeze a representative, versioned documentation corpus
- implement the task/attempt manifest and paired benchmark in §11
- implement the threat controls for the selected profile; implement the §10.2 sandbox contract before selecting code mode
- define the target deployment scenario, TCO terms, and go/no-go thresholds
- do not generalize a local CLI result to remote governed deployments

### Phase 1: local CLI

- Git/website ingestion
- Markdown/MDX/OpenAPI/llms.txt parsing
- version pinning
- SQLite FTS/BM25
- exact identifier search
- JSON output
- citations and hashes

### Phase 2: hybrid retrieval

- embeddings
- reciprocal-rank fusion
- reranking
- structured examples and signatures
- freshness and sync jobs

### Phase 3: agent integration

- short skill file
- one unified MCP `search_docs`
- `open_doc`
- `verify_doc`

### Phase 4: code mode

- parallel lookup
- version comparison
- citation verification
- batch retrieval
- compact result synthesis

The provisional implementation hypothesis is not “another MCP server.” It is a high-quality, version-aware documentation index with a CLI-first interface for the local shell-capable scenario and optional protocol adapters selected by the matrix in §6. It becomes a recommendation only if the matched benchmark and profile-specific security/operations gates pass. If the intended deployment is remote, multi-user, shell-less, or governance-heavy, evaluate the authenticated MCP/API boundary and apply the same retrieval core rather than assuming the local CLI default.

**Current status:** no benchmark, security, legal, or operational gate has been executed or passed by this report. The phase list is an implementation plan for testing the hypothesis, not an approval to ship or a final interface selection.

---

# References

## Context7 and documentation infrastructure

1. [Context7 GitHub repository at E1](https://github.com/upstash/context7/tree/d9561600ad502cd72ab8bae4f9172f434aadf4d1)
2. [Context7 plans and pricing](https://context7.com/docs/plans-pricing)
3. [Context7 API guide](https://context7.com/docs/api-guide)
4. [Context7 search API](https://context7.com/docs/api-reference/search/search-documentation-in-one-request.md)
5. [Context7 documentation context API](https://context7.com/docs/api-reference/context/get-documentation-context.md)
6. [Adding libraries](https://context7.com/docs/adding-libraries)
7. [Library owners and `context7.json`](https://context7.com/docs/library-owners)
8. [Keeping libraries fresh](https://context7.com/docs/library-updates)
9. [Infrastructure security](https://context7.com/docs/security/infrastructure.md)
10. [Data privacy](https://context7.com/docs/security/data-privacy.md)
11. [Data safety](https://context7.com/docs/security/data-safety.md)
12. [Context7 CLI at E1](https://raw.githubusercontent.com/upstash/context7/d9561600ad502cd72ab8bae4f9172f434aadf4d1/packages/cli/README.md)
13. [Context7 MCP rules at E1](https://raw.githubusercontent.com/upstash/context7/d9561600ad502cd72ab8bae4f9172f434aadf4d1/rules/context7-mcp.md)
14. [How Redis Search powers Context7](https://upstash.com/blog/how-redis-search-powers-context7)
15. [Context7 without context bloat](https://upstash.com/blog/new-context7)
16. [Context7 versus Claude Code web search benchmark](https://upstash.com/blog/context7-vs-web-search-benchmark)

## MCP, CLI, and code mode

17. [MCP tools specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)
18. [GitHub MCP Server at E14](https://github.com/github/github-mcp-server/tree/f38526264abb14961b32cea386e38ec6d3cf674f)
19. [GitHub MCP Server configuration at E14](https://github.com/github/github-mcp-server/blob/f38526264abb14961b32cea386e38ec6d3cf674f/docs/server-configuration.md)
20. [GitHub CLI manual](https://cli.github.com/manual/)
21. [Anthropic: Code execution with MCP](https://www.anthropic.com/research/code-execution-with-mcp)
22. [Anthropic: Advanced tool use](https://www.anthropic.com/research/advanced-tool-use)
23. [Anthropic: Effective context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
24. [Anthropic: Building effective agents](https://www.anthropic.com/research/building-effective-agents)
25. [Anthropic: Multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system)
26. [Cloudflare: Code Mode](https://blog.cloudflare.com/code-mode/)
27. [Scalekit: MCP is up to 32× more expensive than CLI](https://www.scalekit.com/blog/mcp-vs-cli-use)
28. [Port of Context: CLI versus MCP versus Code Mode](https://www.portofcontext.com/blog/cli-vs-mcp-vs-code-mode)
29. [CircleCI: MCP versus CLI](https://circleci.com/blog/mcp-vs-cli/)
30. [Tyk: MCP versus CLI enterprise comparison](https://tyk.io/learning-center/mcp-vs-cli-for-ai-agents-enterprise-comparison-guide/)
31. [Apideck: CLI alternative to MCP context overhead](https://www.apideck.com/blog/mcp-server-eating-context-window-cli-alternative)
32. [mcp2cli](https://github.com/knowsuchagency/mcp2cli)

## Academic research

33. [CodeRAG-Bench: Can Retrieval Augment Code Generation?](https://aclanthology.org/2025.findings-naacl.176/)
34. [When LLMs Meet API Documentation, v1 / E12](https://arxiv.org/html/2503.15231v1)
35. [On Mitigating Code LLM Hallucinations with API Documentation, v1 / E12](https://arxiv.org/html/2407.09726v1)
36. [CloudAPIBench ICSE-SEIP DOI](https://doi.org/10.1109/ICSE-SEIP66354.2025.00027)
37. [ContextBench / E12 capture](https://euniai.github.io/ContextBench/)
38. [MCP Tool Descriptions Are Smelly](https://doi.org/10.1145/3858625)
39. [AgentDocsBench](https://pypi.org/project/agentdocsbench/)

## Documentation formats and benchmarks

40. [The `llms.txt` proposal](https://llmstxt.org/)
41. [Mintlify Docs URL Discovery Benchmark at E4](https://github.com/mintlify/docs-url-discovery-bench/tree/5f055a89336cb94050e36eaa479374b390e92462)
42. [Mintlify benchmark results at E4](https://github.com/mintlify/docs-url-discovery-bench/blob/5f055a89336cb94050e36eaa479374b390e92462/RESULTS.md)
43. [Firecrawl Developer Index and DevDex](https://www.firecrawl.dev/blog/developer-index-launch.md)
44. [Zvec agent benchmarks](https://zvec.org/en/docs/zvec-grep/benchmarks/)

## Community evidence

45. [Hacker News: Context7 usage discussion](https://news.ycombinator.com/item?id=46490087)
46. [Hacker News: Bad MCP design costs more tokens](https://news.ycombinator.com/item?id=48407391)
47. [Hacker News: `llms.txt` discussion](https://news.ycombinator.com/item?id=48410589)
48. [MMNTM: The MCP Tax](https://www.mmntm.net/articles/mcp-context-tax)

## Competitive boundary scan

49. [Grounded Docs MCP Server](https://github.com/arabold/docs-mcp-server) (scan pin E11: `4e35baa7e82c3ec447405c48a301c8a4185e715f`)
50. [`mcpdoc`](https://github.com/langchain-ai/mcpdoc) (scan pin E11: `8d01c08598e3f19fd6318bded3ffdcda85db03a4`)
51. [Ref MCP](https://github.com/ref-tools/ref-tools-mcp) (scan pin E11: `f5feb829cc7bdd18fb9b276c27e3f391b2c4132c`)
52. [`neuledge/context`](https://github.com/neuledge/context) (scan pin E11: `690422b717afc5b73249de7a770742e6dcc3620a`)

53. [SPDX License List](https://spdx.org/licenses/)
54. [RFC 9309: Robots Exclusion Protocol](https://www.rfc-editor.org/rfc/rfc9309.html)
55. [GitHub Terms of Service](https://docs.github.com/en/site-policy/github-terms/github-terms-of-service)
