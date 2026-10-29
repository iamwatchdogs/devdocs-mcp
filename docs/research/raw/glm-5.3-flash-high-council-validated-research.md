# Research Report: Developer-Docs Tooling for Agentic Workflows — Context7 Internals, MCP vs CLI, Code Mode, and Retrieval Architecture

**Mode:** Deep | **Date:** 2026-10-26 (revised 2026-10-27) | **Sources:** 65 | **Status:** Complete

---

## Executive Summary

This report evaluates how a free, open, version-correct developer-documentation tool should be built for coding-agent workflows, and whether MCP, a CLI, or "Code Mode" is the right interface. The evidence base combines a raw-data audit of the only controlled CLI-vs-MCP benchmark published to date, pinned source-code analysis of Upstash Context7, peer-reviewed retrieval research, and 2025–2026 practitioner measurements.

Four findings drive the recommendation. First, Context7's open-source repository is a thin client — two MCP tools (`resolve-library-id`, `query-docs`) plus a CLI and SDK — over a closed, hosted retrieval index; "self-hosting" the server does not self-host the data, and the free tier is capped at 1,000 API calls per month — a limit cut ~92% in January 2026 and partially reverted days later [30][31][32][63][64]. Second, the strongest direct comparison (Scalekit, n=68 audited runs) shows MCP with a 43-tool catalog costing 4.3–32× more tokens than a shell-based CLI on identical GitHub tasks, but our audit found the delivered study undercuts its own pre-registration (3–5 runs for the MCP condition instead of the pre-registered 30; a Mann-Whitney implementation where Wilcoxon was pre-registered), and its monthly dollar extrapolation cannot be reproduced from its own stated pricing and token data [1][2][3][4]. Third, the bloat is a *loading policy* problem, not a protocol property: the MCP specification does not require full-catalog exposure, GitHub's MCP server ships toolset filtering, and dynamic tool discovery cuts static toolset tokens by ~91–97% at the cost of 2–3× more tool calls [50][53][18][6]. Fourth, retrieval quality and version correctness — not interface choice — dominate docs-domain outcomes: Context7's issue tracker shows stale results, and DocPrompting, Self-Route, and context-degradation studies show small, version-pinned, well-chosen chunks beat context stuffing [14][15][33][36][37][38].

**Primary Recommendation:** Build a protocol-agnostic local retrieval core (version-pinned corpus, hybrid symbol-first retrieval, source-cited snippets) and expose it first as a CLI for shell-equipped coding agents, with a 2–4-tool MCP adapter as a second interface; do not clone Context7's hosted backend, and do not adopt Code Mode for a two-operation docs domain without a measured win. The differentiation must be earned against existing open alternatives (Finding 6): provenance on every span and refresh that cannot strand users.

**Confidence Level:** Medium-high on Context7 architecture and the schema-bloat mechanism (multiple clusters); medium on interface rankings (no controlled dev-docs-interface benchmark exists; one GitHub-operations benchmark, with caveats); medium on Code Mode claims (peer-reviewed at the core [58]; headline vendor numbers remain internally generated [54][55], one disclosed negative result).

---

## Introduction

### Research Question

The user is planning to build a Context7-style tool: up-to-date, version-specific developer documentation delivered to coding agents. Four questions were posed. What are the internal workings of the Context7 MCP server? What is "Code Mode" for MCP, and what evidence supports it? Is a CLI the better delivery vehicle for developer docs in agentic workflows? And what design best augments official developer documentation into an agentic workflow? A framing premise was supplied: "GitHub MCP takes most of the context, while gh-cli takes none, and they both get the job done" — a premise this report tests rather than assumes.

The question matters because agent tooling is consolidating around two delivery shapes — typed MCP tools and plain shell CLIs — and the choice is frequently argued from vendor marketing rather than controlled evidence. A wrong choice at the architecture level (hosted dependency, wrong interface, bloated tool catalog) is expensive to reverse once an ecosystem of users depends on it.

### Scope & Methodology

This is a deep-mode research run executed across 2026-10-26, with two continuity sessions. We investigated six parallel paths: (1) Context7 internals from pinned source code and official pricing; (2) Code Mode and code-as-action execution evidence, including community implementations consulted but not load-bearing ([21]–[25]); (3) CLI-vs-MCP interface evidence including agent-interface research (SWE-agent ACI [51]) and terminal-agent benchmarks (Terminal-Bench); (4) documentation-retrieval research (DocPrompting, RAG-vs-long-context, context degradation, llms.txt); (5) practitioner evidence from Hacker News, GitHub issue trackers, and engineering blogs; and (6) a raw-data audit of the Scalekit mcp-vs-cli benchmark repository at pinned commit f131dd8, including independent recomputation of every published median and a review of its statistical analysis code.

We consulted 65 distinct sources (60 in the original run; 5 added in the 2026-10-27 revision to close review-identified gaps, recorded in `batch_008.json`): peer-reviewed papers and NeurIPS/ICLR/ICML/USENIX proceedings, the official MCP specification (2025-06-18), pinned source repositories (upstash/context7, github/github-mcp-server, scalekit-inc/mcp-vs-cli-benchmark, freeCodeCamp/devdocs), vendor documentation and pricing pages, engineering blogs with reproducible methods, and community threads. All quantitative results were normalized within-study only; no cross-benchmark leaderboard was produced. Claims are extracted to a machine-readable ledger (`claims.jsonl`) whose `cited_source_ids` and `evidence_ids` linkage is populated programmatically; vendor-only numbers are labeled as such, and the evidence store (`evidence.jsonl`, 150 rows) retains the supporting quotes with locators. The ledger's automated support check still flags uncited process and synthesis sentences — those are prose transitions and labeled judgments rather than sourced facts, and every quantitative claim carries an inline citation.

Excluded from scope: a universal MCP-vs-CLI verdict; unverified download/star metrics; closed-backend internals of Context7 (labeled as unknown rather than guessed); scraping of restricted documentation; and implementation of the proposed tool during this run.

### Key Assumptions

- **Primary target is a coding harness with a shell.** Most agentic coding tools (Claude Code, Codex CLI, OpenCode, Cursor terminal mode) already provide a bash/shell tool. The CLI path's "zero marginal tool schema" claim holds only where that shell tool already exists; a harness without one pays for it differently. Where this assumption matters, it is flagged.
- **Docs operations are read-only and public.** Local, single-user documentation lookup does not need delegated per-user OAuth or tenant isolation. Multi-tenant products change the calculus (see Finding 7 and Recommendations).
- **Model and corpus can be held constant in a future interface benchmark.** Without that control, interface effects and retrieval effects are confounded; no published study has yet done this for the docs domain.
- **"Free" means no required hosted service.** Local indexing, storage, update bandwidth, and optional local embedding compute still cost something; Context7-style hosted APIs impose monthly call caps and key requirements instead.
- **Current published comparisons do not directly measure the docs-lookup scenario.** Transfer from GitHub-operations benchmarks and code-generation retrieval studies is labeled per claim.

---

## Main Analysis

## Finding 1: Context7 Is a Thin Open-Source Client Over a Closed Hosted Index — Not a Self-Hostable Retrieval System

The single most decision-relevant fact for anyone planning a Context7-like tool is what the repository actually contains. At pinned commit d956160 (repo HEAD on 2026-10-26), `upstash/context7` is a MIT-licensed pnpm monorepo whose packages — `mcp`, `cli`, `sdk`, `opencode`, `pi`, `tools-ai-sdk`, `admin-mcp` — are all *clients* of a hosted service [30]. The MCP server registers exactly two tools: `resolve-library-id` ("Resolves a package/product name to a Context7-compatible library ID") and `query-docs` ("Retrieves and queries up-to-date documentation and code examples from Context7 for any programming library or framework"), with zod input schemas for `query`, `libraryId`, and `language` [30]. Both tools forward requests to Upstash's hosted API: `searchLibraries()` calls `https://context7.com/api/v2/libs/search` and `fetchLibraryContext()` calls `/v2/context`, overridable only by pointing `CONTEXT7_API_URL` at a different *API endpoint*, not at a local index [30]. There is no parser, indexer, embedder, or vector store anywhere in the repository [30]. Any official description of how the backend crawls, chunks, or embeds documentation is a vendor claim about a closed system, not inspectable code [30].

The API surface confirms the hosted boundary and reveals something useful for an open alternative: Context7 now documents a full REST API — `GET /api/v3/search` (search without resolving a library first), `GET /api/v2/libs/search`, `GET /api/v2/context`, `POST /api/v1/refresh`, plus ingestion endpoints for GitHub/GitLab/Bitbucket repositories, OpenAPI specs, llms.txt files, websites, Confluence, and Notion [31]. All of it requires `Authorization: Bearer CONTEXT7_API_KEY` [31]. Library IDs support version pinning in two syntaxes (`/vercel/next.js/v16.2.9` or `/vercel/next.js@v16.2.9`) [31], which is the right design — but the index behind the pin is Upstash's.

The pricing quantifies the "not completely free" premise. The Free plan is $0 with **1,000 API calls per month** (search and context calls each count), public repositories only; when blocked, users get 20 bonus calls per day until the monthly reset [32]. Pro is $10 per seat/month with 2,000 calls per seat/month, never blocked, $5 per 1,000 calls overage, and private-repo parsing at $5 per 1M tokens [32]. Enterprise adds self-hosted *deployment* among SOC-2/SSO features [32] — deployment of the service, still not an open index. For a heavy coding agent that queries docs several times per task, 1,000 calls/month is a real constraint: a single long session can consume dozens of calls. The cap is also volatile: in January 2026 Upstash cut the free tier from roughly 6,000 requests/month (approximately 200 requests/day) to 500 — a 92% reduction — before reverting to 1,000/month on January 16 — the article links the revert to community pushback without stating it as cause [63]; community reports also describe a 60 requests/hour limit on the free tier [64]. A repricing of this kind is exactly the hosted-dependency risk a local corpus is structurally immune to.

Two operational observations follow from the code and issue tracker. First, authentication is delegated to Clerk (OAuth authorization server `https://clerk.context7.com`), and rate-limit error messages funnel users toward the plans page and dashboard [30] — the free tier is instrumented for conversion. Second, and more telling for architecture decisions, Context7 itself has converged on multi-interface delivery: the README's setup command offers "CLI + Skills or MCP mode" [30], the repo ships a `ctx7` CLI whose `docs` command hits the same hosted API, and the skill files that teach agents to use that CLI total roughly 9.9 KB (2,008 B SKILL.md + 5,896 B references + 2,377 B rules) versus ~4.5 KB for the MCP-path skill and rule files [30]. The lesson is not that CLI beats MCP or vice versa — it is that Context7's own maintainers treat the interface as a swappable surface over one shared retrieval service, which is exactly the architecture this report recommends (Finding 8).

The hosted index also has a documented quality problem. The project's own issue tracker shows 75 issues matching "stale," including a reproduction where Context7 resolved Retrofit to a mirror repository and returned Retrofit 2.11.0 documentation when the latest stable was 3.0.0 [14][15], plus rate-limit-driven processing bugs [16]. This is a structural risk of crawl-time indexes — with the caveat that our evidence here is two clusters (one product's issue tracker and a 2011 JDK study [48]), not a survey of such systems: the tool's correctness is bounded by its crawler's freshness, and users cannot fix it locally. An open, local alternative that pins docs to released versions removes the crawl-freshness failure mode for pinned versions by construction; it does not eliminate freshness work — ingest lag and cold-start ingestion for not-yet-indexed libraries become the local tool's own problems (Gap 6).

**Implications.** Building "a Context7 clone" by forking the repo yields none of the value — the value is the index. A credible open tool must own the ingestion pipeline: version-pinned sources, deterministic parsing, and provenance metadata. Context7 can still be used *as a remote provider* behind the same interface for comparison or fallback, within its call caps.

**Sources:** [30], [31], [32], [14], [15], [16], [26]

---

## Finding 2: The Only Controlled CLI-vs-MCP Benchmark Shows Large Schema-Driven Cost Gaps — and Its Weaknesses Teach How to Benchmark Properly

Scalekit's March 2026 study is, to our knowledge, the only published controlled comparison running identical tasks through a CLI agent and an MCP client on the same model [1]. The setup: Claude Sonnet 4, same prompts, three conditions — CLI (bare bash tool, model must recall `gh` from training), CLI+Skills (bash plus an ~800-token tips file), and MCP (GitHub's Copilot MCP server with all 43 tool schemas injected) — across five deterministic, verifiable read-only tasks against `anthropics/anthropic-sdk-python` [1]. Published medians: 1,365 tokens (CLI) vs 44,026 (MCP) on the simplest task, a 32× gap; 4–32× across the five tasks [1]. Of 25 scheduled MCP runs, 7 failed with TCP-level `ConnectTimeout` to the remote server; CLI runs had no such failures because `gh` runs locally [1].

Because the harness and result pack are open source, we audited the raw data at pinned commit f131dd8 rather than trusting the blog [2]. The audit confirms the headline token medians exactly (recomputed MCP:CLI median total-token ratios per task: 32.3, 19.6, 8.8, 6.7, 4.3 [2]) and adds material caveats the blog omits:

- **The result pack is incomplete.** 68 stored result files (25 CLI, 25 CLI+Skills, 18 MCP) against 75 scheduled runs; the 7 failed MCP runs are absent from `results/raw`, so failure-rate analysis rests on the blog's account, not stored records [2].
- **`task_completed` is not a verifier verdict.** In `BaseAgent`, `task_completed = (error is None) and len(final_text) > 0` — presence of *any* final text counts as completion [2]. All 68 stored runs show `task_completed=true`; this is transport success, not answer correctness [2].
- **Verification is partial in public.** Four of five tasks use a `contains` check that we independently re-ran against every stored output — all pass [2]. Task 4 is LLM-judged, but no judge verdicts are in the public pack, so the strongest reliability claim ("CLI 100% vs MCP 72%") mixes transport failures with an unverifiable correctness layer [1][2].
- **The delivered study undercuts its own pre-registration.** `METHODOLOGY.md` specifies 30 runs per agent per task, a paired Wilcoxon signed-rank test, and Bonferroni correction [3]. The delivered pack has n=3–5 for the MCP condition (both CLI conditions ran the full 5), and `stats.py` implements Mann-Whitney U inside a function named `wilcoxon_test`, with a t-interval computed over position-paired differences — a paired CI alongside an unpaired test [4]. The blog's "p < 0.05" claims therefore do not follow from the pre-registered analysis, and n=3–5 cannot support them at the claimed effect precision. Two caveats cut in opposite directions: (a) the methodology file is pinned in the same commit as the results, so its priority over data collection is presumed, not proven — "pre-specified in the same commit" is the strictly defensible claim; (b) the audit understated one defect — the CI code silently truncates the larger group by zipping the arrays, so the interval and the test are not even internally consistent.
- **The dollar extrapolation does not reproduce.** The blog does document its workload assumption — the monthly figures are explicitly stated for "running 10,000 operations per month" [1]. At the blog's own stated uncached pricing ($3/M input, $15/M output), the published token medians imply $0.005–$0.034 per CLI run and $0.10–$0.25 per MCP run — i.e., $51–$342 vs $1,005–$2,537 per 10,000 runs [2]. The published "$3.20 vs $55.20 per month" is therefore 16–107× lower (CLI) and 18–46× lower (MCP) than its own token data and pricing imply — more than an order of magnitude for every task. The most plausible unaccounted factor is prompt caching, which the blog never mentions; regardless of cause, the figure is not reproducible from the disclosed assumptions [1][2].

The most transferable result is not the ratio but the *mechanism*: the MCP condition's cost is dominated by 43 tool schemas injected into every turn — the agent uses one or two [1]. Input tokens are re-billed on every model call in a multi-turn session, so a static catalog converts into a per-turn tax that grows with conversation length. Note also the direction of the CLI+Skills condition: adding an ~800-token skills file *raised* cost on three simple tasks (CLI 1,365→4,724 tokens on task 1) yet *lowered* it on the most complex task (8,750→6,860) while cutting tool calls — instruction payload trades off against exploration [2]. Both the Scalekit result and our recomputation are *uncached* accounting; provider prompt caching discounts repeated schema tokens on cache reads, which is the most likely reason the only caching-aware practitioner benchmark found 2–3× rather than 4–32× [20]. The spread between this report's two main cost datapoints is plausibly dominated by cache treatment, and the uncached ratios should be read as an upper bound on the mechanism's cost.

Independently, a separate practitioner benchmark of GitHub operations (published with an HN discussion and caching accounted for) found GitHub MCP 2–3× more expensive than the CLI with a 12× gap on a multi-step CI-failure investigation [20]; an enterprise-IT writeup reports a ~35× token ratio for an Intune device-export task but with no model, repetitions, tokenizer, or correctness verification disclosed — our audit classifies it as an uncontrolled anecdote [12], and the OnlyCLI "35×" post concedes mixed provenance ("published benchmarks by independent researchers and our own measurements") with a 93-tool server assumed [5]. Secondary roundups that cite Scalekit and Jannik as their sources are syndications of the same clusters, not independent evidence [13].

**Implications.** For a docs tool the analogy to "43 GitHub tools" is weak — Context7 exposes exactly two tools, so a *narrow* docs MCP server's static schema cost is a few hundred tokens, not tens of thousands. The Scalekit numbers indict *large static catalogs*, not the MCP protocol. And the study's own flaws (pre-registration abandoned, missing failure records, non-reproducible dollar figures) are a template for what a credible interface benchmark must do better (Finding 8).

**Sources:** [1], [2], [3], [4], [5], [12], [13], [20]

---

## Finding 3: Token Bloat Is a Loading-Policy Problem, Not a Protocol Property

The MCP specification (2025-06-18) is explicit that tool exposure is a client-side policy choice: tools are "model-controlled," but "implementations are free to expose tools through any interface pattern that suits their needs — the protocol itself does not mandate any specific user interaction model" [50]. `tools/list` supports pagination and `listChanged` notifications, meaning the protocol has first-class support for dynamic, incremental catalogs [50]. The spec also assigns security obligations that matter later in this report: servers must validate inputs, rate-limit invocations, and sanitize outputs; clients should validate results before passing them to the model and log usage for audit [50].

The ecosystem has converged on this reading. Lunar.dev's widely shared rebuttal states it plainly: the CLI-vs-MCP gap "is a comparison of 1 tool in context vs 43. Load 43 CLI tool definitions into the system prompt and you get the same overhead. The 30x gap is a property of naive tool loading, not of MCP" [6]. GitHub's own MCP server ships toolset filtering (`--toolsets repos,issues,...`), per-tool selection (`--tools`), and read-only mode, documenting that "enabling only the toolsets that you need can help the LLM with tool choice and reduce the context size"; notably, the *default* when no toolsets are specified is a subset, not all 50+ tools [53]. Even so, a user filed issue #1683 asking for exactly the CLI's properties: less verbose prompts and repository-context focus "similar to how `gh` CLI operates" [17] — evidence that even filtered MCP catalogs carry more scaffolding than practitioners want.

Measured dynamic-discovery results are strong but vendor-adjacent. Speakeasy's dynamic toolsets (search-first, load schemas on demand) report input-token reductions of 96.7% on simple and 91.2% on complex tasks versus static toolsets with 40–400 tools, at the cost of 2–3× more tool calls and ~50% longer execution — a real trade, not a free lunch; vendor-adjacent measurement with model and trial counts not disclosed [18]. The vendor-neutral lesson: static catalogs above a few dozen tools are dominated by schema bytes (Speakeasy estimates schemas at 60–80% of static toolset token usage [18]); below that threshold — as with a 2-tool docs server — the entire debate is nearly moot. An HN comment on the "MCP Token Trap" thread captures the direction of travel: "Both Claude and OpenAI solved the mcp context bloat issue with on-demand tool discovery" [19].

**Implications.** The user's premise — "GitHub MCP takes most of the context, while gh-cli takes none" — is directionally right but mechanically wrong. GitHub MCP's default is *not* "most tools" anymore, and the CLI is not free: a shell tool definition exists in the harness, `gh` skill/instruction files carry tokens (Context7's own CLI skill pack is ~9.9 KB [30]), model prior knowledge of `gh` is doing unpaid work that does not generalize to a novel docs CLI, and output tokens (file contents, large JSON) count in both worlds. The correct statement is: *marginal* schema cost of one more CLI command is zero in a shell-equipped harness, while a *large static MCP catalog* taxes every turn. A narrow docs server (2–4 tools) sits near zero in both cases, which is why interface choice for this domain should be decided on other axes — retrieval quality, reliability, offline behavior, and composability (Findings 5–8). We also engage the counter-conclusion we had cited only for its numbers: kunchenguid's benchmark ends by recommending "none of the above" for GitHub operations [20]. That recommendation does not transfer to this domain — for docs lookup, "none" means relying on training-data knowledge of exactly the API drift the tool exists to correct, and the retrieval corpus, not the interface, is the artifact under evaluation.

**Sources:** [50], [53], [6], [18], [17], [19], [30]

---

## Finding 4: Code Mode and Code-As-Action — Real Pattern, Vendor-Scale Numbers, and a Documented Negative Result; Wrong Default for a Two-Tool Domain

Code Mode exposes servers as typed code APIs that the model calls inside a sandbox, discovering definitions progressively, so intermediate results never enter the conversation [23][54]. Cloudflare's flagship post demonstrates the two-meta-tool shape (`search()` over generated TypeScript stubs, `execute()` in a worker sandbox) but reports **no token counts, percentage reductions, or benchmark methodology** — our full-text check found only qualitative claims [23]; the "~1,000 tokens vs 244,000" figures circulating in secondary coverage trace to a different vendor's gateway writeup, not Cloudflare's measurement [13]. Anthropic's "code execution with MCP" article supplies the most-quoted number — "reduces the token usage from 150,000 tokens to 2,000 tokens—a time and cost saving of 98.7%" — but it is an illustrative Google Drive + Salesforce transcript, vendor-internal, not a published benchmark [54].

Anthropic's own product documentation, examined closely, is more informative than the headline because it contains both wins and a negative result. Programmatic tool calling "improved performance by an average of 11 percent while using 24 percent fewer input tokens" on agentic search benchmarks and cut billed input ~38% on a 75-tool internal eval [55]; but on τ²-bench "programmatic tool calling left scores unchanged and cost roughly 8 percent more" — a vendor-reported negative result whose per-task trial counts are not disclosed [55]. The same docs state savings are a function of catalog size: "requests whose tools array contains 10 to 49 tool definitions see typical token savings of 20 to 40 percent" [55] — implying near-zero headroom below ~10 tools. The Tool Search Tool loads only the 3–5 needed tools (>85% reduction on a five-server, 58-tool, ~55K-token-definitions example), improved internal MCP-eval accuracy (Opus 4: 49%→74%), and — critically for scope — states that direct tool selection "degrades once you exceed 30–50 available tools" [56][57] — a vendor-reported operating guideline rather than a measured benchmark, used below only as an order-of-magnitude threshold.

The academic record supports on-demand loading for large catalogs and composition-heavy work, with honest limits. CodeAct (ICML 2024, 17 LLMs): up to 20% higher success than JSON/text actions on M3ToolEval (82 instances; GPT-4-0613: 67.1 CodeAct vs 56.1 JSON vs 45.1 text) with up to 30% fewer actions — yet on atomic API-Bank calls closed-source models favored JSON (GPT-4: 82.0 JSON vs 75.4 CodeAct), locating the advantage in *multi-step composition*, not per-call quality [58]. RAG-MCP (2025) triples tool-selection accuracy (43.13% vs 13.62%) with ~50% fewer prompt tokens over a 4,400-server registry, but used one non-frontier model and candidly reports retrieval precision degrading beyond ~100 candidates [59]. MCP-Zero reports 98% token reduction on APIBank while selecting among 2,797 tools across 308 servers [60] — a single-paper result with no independent replication, cited as an upper-bound illustration rather than a calibrated anchor. Community implementations of local code mode call it "a clever middle ground between using dozens of context-munching MCP servers and just giving the agent access to your command line" [21]; skeptics call it "needless complexity and failure points" [22].

The transfer limit is decisive for this report. A docs-lookup tool has 2–4 operations and small outputs: below Anthropic's own 10-tool savings threshold [55], far below the 30–50-tool degradation point where discovery machinery starts paying [57], with no composition fan-out of the kind that gives CodeAct its edge [58], and no large intermediate payloads to keep out of context. Code Mode's costs (sandbox provisioning, code-gen errors, added latency, expanded security surface — which Anthropic itself flags [54]) arrive with essentially none of its benefits at this scale. Speakeasy's dynamic toolsets, which capture most of the token win without code execution, make code mode the *third* choice for narrow domains [18].

**Implications.** Treat code-as-action as a scaling tool: adopt it when the docs tool grows into pipeline work (grepping whole API trees, cross-referencing multiple libraries' source and docs) or when serving many backends through one surface — and gate the adoption on the Finding 8 experiment, using Anthropic's τ²-bench negative result as the reminder that "obvious" wins sometimes invert on measured tasks.

**Sources:** [23], [13], [54], [55], [56], [57], [58], [59], [60], [21], [22], [18], [19]

---

## Finding 5: CLI Reliability Advantages Are Real but Narrow; Both Interfaces Fail Differently

The Scalekit audit separates two failure classes that blog discourse conflates [1][2]. Transport completion: MCP failed 7/25 scheduled runs on TCP timeouts to a remote server; CLI (local process) had none [1]. Answer correctness: unverifiable in public for task 4, and for the other four tasks every stored run passed the `contains` check — so the "100% vs 72% reliability" headline is mostly a *network topology* statement, not a statement about models using tools [1][2]. This distinction matches the τ-bench framing that one-shot success and repeated-trial reliability (pass^k) are different metrics — a system can transport perfectly and still answer inconsistently [8].

The CLI's advantages — no remote dependency, composable with pipes/grep, model prior knowledge of popular tools — are real for local, single-user contexts. Its costs are equally real and usually unstated: the harness still pays for a shell tool definition; skill files cost tokens (9.9 KB for Context7's CLI path [30]); a novel docs CLI has *no* model prior knowledge, so instruction files are mandatory, not optional; and error-driven retries on a learned-by-trial CLI can silently multiply turns. HAL's cross-benchmark rollout data (21,730 rollouts, ~$40,000) demonstrates why per-run cost accounting matters: within-benchmark cost spread reaches three orders of magnitude depending on model, scaffold, and budget — cost claims without that context are noise [11]. "AI Agents That Matter" adds the field-level warning that accuracy-only reporting plus missing standardization makes most agent cost claims non-reproducible [7].

Security posture also differs by deployment, not by interface per se. For a *local developer tool acting as its own user*, ambient CLI credentials are fine. For a *product acting on behalf of customers*, the Scalekit argument applies: per-user OAuth, tenant isolation, and structured audit trails are what MCP's authorization model (via OAuth-protected servers) provides and raw shell access does not [1]. The OpenClaw ecosystem incidents (exposed instances, malicious community skills, session hijacking; Anthropic subsequently blocked subscription usage [29]) illustrate the failure mode of ambient-credential agents at scale — though for a read-only docs tool the blast radius is small.

**Implications.** For the docs MVP (local, read-only, single-user), CLI reliability advantages are decisive on transport and irrelevant on auth. Ship CLI first; add MCP for clients/harnesses without a shell and for remote deployments where audit and auth boundaries matter.

**Sources:** [1], [2], [8], [30], [11], [7], [29]

---

## Finding 6: Retrieval Quality and Version Correctness — Not Interface — Determine Whether a Docs Tool Helps

The causal evidence that documentation retrieval improves code generation is old and solid [33]. DocPrompting (ICLR 2023) retrieves official docs before generation and improves CodeT5 by +2.85 pass@1 (52% relative) on CoNaLa and up to +6.9 exact match on Bash `tldr` [33], with the motivating observation that "publicly available source-code libraries are continuously growing and changing. This makes it impossible for models of code to keep current with all available APIs by simply training" [33][34] — the exact problem a docs tool addresses. BEIR establishes that BM25 is the robust zero-shot baseline and that dense retrievers tuned on one domain often underperform it out-of-domain, with rerankers/late-interaction models best on average [39] — i.e., build hybrid retrieval with exact/lexical first for API symbols, dense for paraphrase, cheap rerank on top.

Against context stuffing, retrieval wins on both cost and often accuracy. NVIDIA's ICLR 2024 result: a 4K-context model with retrieval matches a finetuned 16K-context model [35]. Google DeepMind's Self-Route (EMNLP 2024 Industry): routing queries to RAG-first-with-LC-fallback cuts cost 39–65% at −0.2 to −2.2% accuracy, with >50% of queries (up to 82%) answerable from retrieved chunks alone [36]. The degradation studies explain why smaller, better-chosen context wins: GPT-3.5-Turbo multi-doc QA drops from 75.8% (relevant doc first) to 53.8% at the middle position (the setting's nadir is 52.9%) — below its 56.1% closed-book score [37]; Chroma's Context Rot finds consistent degradation with input length across 18 models, with even one distractor hurting performance — while cautioning its tasks are simple and "not exhaustive of real-world use cases" [38]. For a docs tool the design consequence is concrete: return few, well-placed, version-correct chunks with source citations; never dump the manual.

Version correctness is the differentiator Context7 gets right in interface (pinned IDs [31]) but struggles with in practice (stale-result issues [14][15], rate-limit-locked refreshes [16]). It is also an old, studied problem: FASE 2011 empirically documented JDK documentation lagging API changes and misleading users [48]. The open ecosystem offers the building blocks: DevDocs maintains per-version, offline-downloadable doc corpora with monthly rebuilds [45]; Next.js pins `@doc-version: 16.4.3` in its llms.txt [43]; Mintlify generates recursive `_llms` indexes and `.md` page endpoints [44]. llms.txt itself is a 2024 Answer.AI proposal — a markdown index with background and links, explicitly for inference-time use [40][41] — but adoption is partial (Anthropic, Next.js, React, Mintlify publish it; MDN does not [42]), near-zero across the wider web (0.2% of 1,500 crawled sites [28]), and major AI platforms largely ignore it [49]; community reaction from 2024 questioned who would volunteer to make scraping easier [27]. So llms.txt is a useful *ingest source*, not a strategy.

One counter-hypothesis deserves explicit treatment: frontier models' post-training knowledge already covers popular libraries' APIs, so retrieval's marginal value may be smaller today than DocPrompting's 2022-era gains suggest [33]. Two considerations keep retrieval load-bearing for this domain. First, the documented failure mode is precisely *staleness at version boundaries* — migration questions ask about releases newer than any training cutoff [33][48]. Second, the agent cannot know which of its recalled API details are stale; retrieval with provenance is the only channel that can be checked against a pinned version, which is exactly what the migration and unanswerable-query task classes in Finding 8 test. We flag this as an unmeasured question, not a settled one.

Existing open alternatives cover slices of this space, and a 2025–2026 wave now targets it directly — a competitive fact the first draft of this report failed to check. arabold/docs-mcp-server ("Grounded Docs," MIT-licensed) is explicitly positioned as "an open-source alternative to Context7, Nia and Ref.Tools," stores "each library version separately," and serves full-text search with optional vector search over a local SQLite corpus via MCP, CLI, and web UI [61]. LangChain's mcpdoc exposes llms.txt files to IDEs through `fetch_docs`/`list_doc_sources` tools with domain allowlisting, though the repository was archived in September 2026 [62]. GitMCP serves GitHub-repo docs over MCP with llms.txt or full-repo modes but publishes nothing on freshness or versioning [46]; DeepWiki (Cognition) provides AI-generated repo wikis with line-level source citations and an MCP server [47] — good for code-grounded answers, not version-pinned official docs. A practitioner built a local-first alternative specifically in response to the January 2026 free-tier cut [64]. The "version-correct official-docs corpus" gap is therefore *contested, not unowned*: the differentiation a new tool must earn is provenance on every returned span (URL + version + commit), deterministic refresh that cannot strand users behind rate limits, and per-version pinning as a first-class query dimension — several of which Grounded Docs already partially implements [61].

**Licensing and terms of service are a first-class design constraint, not a footnote.** A redistributable corpus of official documentation is a copyright artifact: a central consideration in fair-use analysis is transformativeness — "does the AI model transform the input data into something new, or does it reproduce it?" — and a tool that returns verbatim doc spans sits on the reproduction end of that axis (analysis sourced from a practitioner legal guide, not legal advice) [65]. Site terms of service can prohibit AI ingestion even where robots.txt is silent, and circumventing anti-bot measures risks statutory damages regardless of whether the underlying data is public [65]. Practical consequences: per-library license gating at ingest (CC-BY-style attribution metadata carried into every citation; redistribution-restricted doc sets excluded from any published corpus or served only from the user's own local fetch), documented provenance for every span (an emerging compliance expectation in its own right [65]), and a bring-your-own-corpus path so the tool never ships a corpus it has no right to redistribute. The first draft carried this only as a four-word parenthetical; it belongs in the architecture.

**Implications.** Invest the engineering in the corpus pipeline and retrieval, not the interface. The interface is a week of work; the index is the product.

**Sources:** [33], [34], [39], [35], [36], [37], [38], [14], [15], [16], [31], [48], [45], [43], [44], [40], [41], [42], [28], [49], [27], [46], [47]

---

## Finding 7: Documentation Content Is an Untrusted Input — Design for Prompt Injection and Corpus Poisoning From Day One

A docs tool inserts third-party text into an agent's context, adjacent to the agent's instructions. The security literature treats this channel seriously [9]. AgentDojo (NeurIPS 2024 D&B) operationalizes indirect prompt injection evaluation with 97 realistic tasks and 629 security test cases, measuring benign utility, utility-under-attack, and attack success rate [9]. PoisonedRAG (USENIX Security 2025) shows a retrieval corpus is an attack surface: five malicious texts in a corpus of millions achieve 90% attack success for a target question, and evaluated defenses were insufficient [10] — for a docs tool, the analog is a poisoned docs page, mirror repo, or malicious `llms.txt` entry steering the coding agent. The MCP specification's own security section requires servers to sanitize tool outputs and clients to validate results before passing them to the model, and to treat tool annotations as untrusted [50].

Practically, a docs tool's mitigations are tractable because the domain is narrow and read-only: strip non-content markup at ingest; never execute fetched code; mark every doc span with its source URL and version so the model (and the user) can attribute claims; treat search-query LLM-rewrites with suspicion (an HN commenter on Upstash's own "no bloat" redesign asked why an LLM-rewritten query is safer than the raw query [26]); and keep the tool read-only so a successful injection cannot pivot into state changes. Rate-limit independence (local corpus) is itself a security property — Context7's rate-limit-locked refresh pipeline produced a stuck-"processing" bug [16], and availability failures push users toward mirrors of unknown integrity.

**Implications.** Corpus provenance + read-only scope + output sanitization are the security MVP. This is also an argument for the local-corpus architecture: the operator controls ingestion, so poisoning requires compromising the operator's sources rather than a multi-tenant crawl pipeline.

**Sources:** [9], [10], [50], [26], [16]

---

## Finding 8: No Published Head-to-Head Exists for the Docs Scenario — a Normalized Experiment Is the Missing Evidence

Every quantitative claim surveyed above is from a different task family: GitHub operations (Scalekit [1], kunchenguid [20]), enterprise device management (Jannik [12], uncontrolled), tool-selection accuracy on large catalogs (RAG-MCP [59], Speakeasy [18]), code generation with docs (DocPrompting [33]), long-context QA (Self-Route [36], Lost in the Middle [37]), and synthetic recall (Context Rot [38]). Normalization rules follow from the field's own methodology critiques: report within-study ratios only; never pool absolute token counts across models, prompts, catalogs, and harnesses; count cumulative billed tokens (re-billed per turn), not just final context size; and express cost per *verified success* including failures — the τ-bench pass^k metric exists precisely because single-attempt success overstates reliability [8][7]. HAL's finding that harness choice alone moves results by up to 7 points warns that an interface benchmark must pin the harness [11][52].

The experiment that would settle the interface question for this domain is well-defined: one local retrieval core, three thin interfaces (CLI, 2–4-tool MCP, Code Mode), identical model snapshot, system prompt, corpus, versions, budgets, network state, and retry policy; task classes covering exact-symbol lookup, conceptual search, version migration, cross-library composition, source-code fallback, unanswerable queries, and poisoned-doc cases; metrics of verified answer correctness, version accuracy, abstention, cumulative and peak tokens, cost per suite success, invalid-call rate, p50/p95 latency, and attack success; ≥3 models, multiple trials, paired task-level statistics (the Scalekit pre-registration — 30 runs per cell, paired Wilcoxon, Bonferroni — is the right shape even though its own delivery missed it [3][4]). We flag honestly that this section is a specification, not a runnable artifact: no harness code, task files, or judge rubric accompany this report, and the Scalekit audit itself demonstrates that pre-registrations can be abandoned in delivery — the same risk applies to this plan until the harness ships with the tool (Recommendation 5).

Until someone runs that, the defensible architecture is the one that makes the experiment cheap: a protocol-agnostic core (CLI + MCP over the same retrieval engine) lets the same corpus serve every interface and makes each one measurable rather than ideological.

**Sources:** [8], [7], [11], [52], [3], [4], [55], [18], [33], [36], [37], [38], [1], [20], [12]

---

## Synthesis & Insights

### Patterns Identified

**Pattern 1: The interface debate is a proxy war over loading policy.** Every "CLI beats MCP" or "Code Mode beats both" result traces to the same mechanism — how much schema and instruction text enters context per turn — not to the transport itself [1][6][18][50]. The MCP spec delegates that policy to clients [50]; GitHub's server ships filters [53]; Anthropic, Cloudflare, and OpenAI productize dynamic loading [19][23]. An open docs tool should therefore treat *what enters context* as the primary design variable and the interface as a thin adapter over it.

**Pattern 2: The value is always in the corpus, never in the client.** Context7's repo is a thin client; its value is the hosted index [30][31][32]. DocPrompting's gains came from retrieving the right official docs [33]; Self-Route's cost savings came from routing to small retrieved chunks [36]; Context7's failures come from a stale crawl [14][15]. Whoever owns version-correct, provenance-carrying doc data owns the product.

**Pattern 3: Vendor claims and reproducible evidence occupy different tiers — and the community now audits.** The strongest numbers in this report came from artifacts that could be pinned and recomputed (Scalekit raw results [2], Context7 source at a commit [30], issue counts via the search API [15]), while the loudest claims (35× [5][12], "99.9% reduction" [13], Cloudflare's qualitative "striking" [23]) either lack methods or dissolve under audit. A credible tool launch should publish its own eval artifacts.

### Novel Insights

**Insight 1: A two-tool docs MCP server makes the entire CLI-vs-MCP schema debate nearly moot — but the CLI still wins on a different axis: failure topology.** With 2–4 tools the schema tax is negligible either way; the durable CLI advantages are local-process reliability (no remote timeout class [1]) and offline corpus control (no rate-limit ceiling [32]). The durable MCP advantages are non-shell clients and future multi-tenant governance [1]. This reframes the user's question: choose interface by *deployment topology*, not by token arithmetic.

**Insight 2: Context7's own evolution validates the "one core, many interfaces" architecture.** A repo that grew a CLI, SDK, AI-SDK tools, and editor plugins around one hosted API [30] is evidence that the maintainers found the interface swappable — and the ~9.9 KB CLI skill pack shows even "zero-schema" CLIs carry an instruction payload once the tool is novel to models [30]. Budget for that file from day one.

**Insight 3: The stale-docs failure mode is an argument for version-pinned local corpora that the hosted model does not realize in practice.** A crawl-time index must chase HEAD; a local tool can pin to the exact released version a project uses, which is more reliable for migration questions ("what changed between v14 and v15") — the query type coding agents most need and hosted aggregators, in the documented cases, serve worst.

### Implications

**For the planned tool:** build the corpus pipeline (versioned ingestion → parse → symbol-aware chunking → hybrid retrieval → cited snippets) as the product; expose CLI and a 2–4-tool MCP adapter over it; treat Code Mode as a measured, optional third interface; support llms.txt and versioned doc trees as ingest sources; publish the eval harness.

**Broader:** the field is converging on progressive disclosure everywhere (tool search, dynamic toolsets, code mode, skills files) [19][18][23]. Tool authors should assume schemas load lazily and design descriptions for *retrieval* — precise names, distinct verbs, token-lean descriptions — rather than for a one-time catalog dump.

**Second-order:** if corpora become the moat, expect doc-platform convergence on machine-readable, version-pinned channels (llms.txt with version headers [43], `.md` endpoints [44]) — and an open tool should be built to ingest all of them plus plain HTML, because adoption remains partial [42][28][49].

---

## Limitations & Caveats

### Counterevidence Register

**Contradictory Finding 1: Dynamic tool discovery trades latency and call count for tokens.** Speakeasy measured ~2× more tool calls and ~50% longer execution alongside 91–97% token reductions [18]. Resolved by treating token and latency as separate axes in the decision gates (Finding 8). Impact: moderate — prevents reading token reduction as an unqualified win.

**Contradictory Finding 2: Long context sometimes wins on accuracy.** Self-Route shows fully-resourced long context consistently outperforms RAG on average [36]. Resolved by noting the cost delta (39–65%) and that a docs tool's queries are mostly answerable from small chunks (>50–82% routed to RAG [36]). Impact: minor for this domain, but honestly retained.

**Contradictory Finding 3: The Scalekit reliability gap may be infrastructural, not architectural.** All MCP failures were TCP timeouts to one hosted endpoint [1][2]; a local MCP server (stdio) would not exhibit them. Resolved by separating transport topology from protocol semantics (Finding 5). Impact: moderate — weakens "MCP is unreliable" claims while preserving "remote dependency is a reliability cost."

### Known Gaps

**Gap 1: No controlled dev-docs interface benchmark exists.** All interface numbers transfer from other task families; the required experiment (Finding 8) has not been run by anyone, to our knowledge. This is the report's largest single limitation.

**Gap 2: Code Mode's strongest academic support (CodeAct) predates modern MCP harnesses and used 2023-era models, and the vendor posts lack published measurements [23][58].** The Code Mode leg of this run was assembled from vendor articles, community implementations, and the academic pattern; an independent replication of the code-vs-JSON comparison on MCP workloads was not found.

**Gap 3: Context7's backend retrieval quality (chunking, ranking, embedding) is unobservable.** All backend behavior is inferred from outputs and issues, never from code [30]. We deliberately make no claim about its internals.

**Gap 4: Community benchmarks (kunchenguid [20], Jannik [12]) disclose partial methods.** They are used as corroborating clusters, not as primary quantitative anchors.

**Gap 6: Long-tail ingestion economics are unquantified.** The recommendation to build a version-pinned corpus pipeline covers the libraries a project already uses; it does not model the cold-start path — the first time an agent touches an un-ingested library, someone pays on-demand fetch/parse/index latency, and per-version storage grows with every pinned release. Designing the cold-miss UX (live fetch with provenance vs. refusal with a sync hint) and estimating storage/bandwidth are open engineering work this report specs but does not cost.

**Gap 5: This report's own audit has the limitations it criticizes.** The Scalekit recomputation is a single-repository, single-commit, single-timepoint audit by one team: no independent party has re-run it, the recomputed ratios carry no confidence intervals (they are point estimates from stored medians), and the benchmark's p-value pipeline was not re-executed end-to-end. Its cost extrapolation is uncached and uses an arbitrary 10,000-run multiplier; caching could compress the gap materially (Finding 2). And "pre-registered" is presumed from a same-commit methodology file, not proven by timestamps.

### Areas of Uncertainty

Pricing and rate limits change quickly; all Context7 figures are as retrieved on 2026-10-26 [32]. The MCP spec continues to evolve (2025-06-18 is the current version surveyed [50]). Leaderboard figures for Terminal-Bench 4.0 were not extracted into this report because the landing page does not expose task counts or stable numbers to cite [52]; it is used only as evidence that the field now measures cost and tokens alongside resolution rate.

---

## Recommendations

### Immediate Actions

1. **Build the corpus pipeline first.** Ingest versioned official docs (git tags where available; versioned llms.txt trees like Next.js [43]; Mintlify `.md` endpoints [44]; plain HTML fallback — with per-library license and ToS gating as a design constraint rather than a compliance footnote (Finding 6)), parse deterministically, chunk by header/API symbol, store `(library, version, source_url, commit_sha, content_hash, section_path, symbol, text)` with provenance. Rationale: Findings 1 and 6 — the index is the product; staleness is the dominant documented failure mode [14][15][48].
2. **Ship the CLI first, over a local index.** Commands: `resolve` (alias→library/version), `search` (query + filters, returns ranked handles + scores), `fetch` (handle + span/token budget → cited snippet), `list`, `sync`, `status`. JSON output, stable IDs, read-only. Rationale: the local core delivers reliability, offline operation, and no call ceiling regardless of interface (Findings 1 and 5); the CLI is the cheapest first surface for shell-equipped harnesses. Honest counterweight: MCP is the distribution channel of MCP-native clients (Cursor, Claude Desktop, VS Code), so for adoption in that ecosystem the adapter in Recommendation 3 is the path, not an afterthought.
3. **Add a 2–4-tool MCP adapter over the same core** (`resolve_library`, `search_docs`, `fetch_doc`) with token-lean descriptions, sized for lazy-loading clients. Rationale: Findings 3 and 8 — negligible schema cost at this width; covers MCP-only clients; makes the future interface experiment cheap.
4. **Treat content as untrusted from day one.** Sanitize ingest, cite every span, never execute fetched code, keep the tool read-only. Rationale: Finding 7 [9][10][50].
5. **Publish the eval harness with the tool.** The Finding 8 task suite (six classes incl. unanswerables and poisoned docs), paired statistics, cost-per-verified-success — shipped as runnable code with a pinned methodology file and retained raw results, because the Scalekit audit shows a pre-registration without artifacts is not credible. Rationale: Finding 8 and the field's reproducibility critique [7][11].

### Next Steps (1–3 months)

1. **Run the interface experiment** on the shared core (CLI vs 2–4-tool MCP vs Code Mode), 3 models, ≥5 trials, paired analysis; adopt Code Mode only if it wins cost-per-verified-success or p95 latency on multi-hop tasks without correctness or security regression. (Pre-stated, labeled decision gates — not literature-derived thresholds.)
2. **Adopt hybrid retrieval incrementally**: BM25/exact-symbol first (BEIR-grounded default [39]); add dense + reranker only if measured Recall@5/nDCG and end-task gains justify latency.
3. **Expose Context7 as an optional remote provider** behind the same interface (respecting 1,000 calls/month free cap [32]) for coverage comparison — but never as a hard dependency.

### Further Research Needs

1. Independent replication of dynamic-toolset/code-mode numbers on dev-docs workloads. 2. Abstention and no-answer behavior for docs retrieval (unanswerable-query handling is unbenchmarked in this domain). 3. Longitudinal staleness measurement: crawl-lag distributions across doc platforms. 4. Effect of skill/instruction-file size on CLI agent performance (the Scalekit CLI+Skills mixed result [2] deserves a proper factorial study).

---

## Bibliography

[1]  (2026). *MCP is up to 32× more expensive than CLI. Here’s why we still use it.*. . https://www.scalekit.com/blog/mcp-vs-cli-use (Retrieved: 2026-10-26)
[2]  (2026). *MCP vs CLI Benchmark public harness and result pack (pinned commit f131dd8)*. . https://github.com/scalekit-inc/mcp-vs-cli-benchmark/tree/f131dd8879315c88a66735dd3b97a8778b340df7 (Retrieved: 2026-10-26)
[3]  (2026). *Scalekit MCP vs CLI Pre-Registered Methodology (pinned commit f131dd8)*. . https://github.com/scalekit-inc/mcp-vs-cli-benchmark/blob/f131dd8879315c88a66735dd3b97a8778b340df7/METHODOLOGY.md (Retrieved: 2026-10-26)
[4]  (2026). *Scalekit benchmark statistical analysis implementation (pinned commit f131dd8)*. . https://github.com/scalekit-inc/mcp-vs-cli-benchmark/blob/f131dd8879315c88a66735dd3b97a8778b340df7/src/benchmark/analysis/stats.py (Retrieved: 2026-10-26)
[5]  (2026). *MCP Token Trap: Why Your AI Agent Burns 35x More Tokens Than a CLI*. . https://onlycli.github.io/OnlyCLI/blog/mcp-token-cost-benchmark/ (Retrieved: 2026-10-26)
[6]  (2026). *CLI vs MCP: You're Asking the Wrong Question*. . https://www.lunar.dev/post/cli-vs-mcp-youre-asking-the-wrong-question (Retrieved: 2026-10-26)
[7]  (2024). *AI Agents That Matter*. . https://arxiv.org/abs/2407.01502 (Retrieved: 2026-10-26)
[8]  (2024). *τ-bench: A Benchmark for Tool-Agent-User Interaction in Real-World Domains*. . https://arxiv.org/abs/2406.12045 (Retrieved: 2026-10-26)
[9]  (2024). *AgentDojo: A Dynamic Environment to Evaluate Prompt Injection Attacks and Defenses for LLM Agents*. . https://nips.cc/virtual/2024/poster/97522 (Retrieved: 2026-10-26)
[10]  (2025). *PoisonedRAG: Knowledge Corruption Attacks to Retrieval-Augmented Generation of Large Language Models*. . https://www.usenix.org/conference/usenixsecurity25/presentation/zou-poisonedrag (Retrieved: 2026-10-26)
[11]  (2025). *Holistic Agent Leaderboard: The Missing Infrastructure for AI Agent Evaluation*. . https://arxiv.org/abs/2510.11977 (Retrieved: 2026-10-26)
[12]  (2026). *CLI Tools vs MCP: Better AI Agents With Less Context*. . https://jannikreinhard.com/2026/02/22/why-cli-tools-are-beating-mcp-for-ai-agents/ (Retrieved: 2026-10-26)
[13]  (2026). *MCP vs CLI for AI Agents: Which One Should You Use in 2026?*. . https://www.firecrawl.dev/blog/mcp-vs-cli (Retrieved: 2026-10-26)
[14]  (2026). *[Bug]: Retrofit documentation is outdated: Context7 returns 2.11.0 instead of latest 3.0.0*. . https://github.com/upstash/context7/issues/3132 (Retrieved: 2026-10-26)
[15]  (2026). *upstash/context7 issue tracker: stale-docs and rate-limit volume*. . https://github.com/upstash/context7/issues?q=is%3Aissue+stale (Retrieved: 2026-10-26)
[16]  (2026). *Version stuck on 'processing' after hitting rate limit, retry says 'Tag already exists'*. . https://github.com/upstash/context7/issues/3225 (Retrieved: 2026-10-26)
[17]  (2025). *Addressing GitHub MCP's Key Issues: Token Scopes, Verbose Prompts, and Lack of Project Context*. . https://github.com/github/github-mcp-server/issues/1683 (Retrieved: 2026-10-26)
[18]  (2025). *Reducing MCP token usage by 100x – you don't need code mode*. . https://www.speakeasy.com/blog/how-we-reduced-token-usage-by-100x-dynamic-toolsets-v2 (Retrieved: 2026-10-26)
[19]  (2026). *MCP Token Trap: Why Your AI Agent Burns 35x More Tokens Than a CLI (discussion)*. . https://news.ycombinator.com/item?id=47543561 (Retrieved: 2026-10-26)
[20]  (2026). *I benchmarked GitHub CLI vs MCP vs Tool Search vs Code Mode — the best solution is none*. . https://medium.com/@kunchenguid/i-benchmarked-github-cli-vs-mcp-vs-tool-search-vs-code-mode-turns-out-the-best-solution-is-none-93528d5039e4 (Retrieved: 2026-10-26)
[21]  (2025). *Open Source and Local Code Mode MCP in Deno Sandboxes*. . https://news.ycombinator.com/item?id=45917182 (Retrieved: 2026-10-26)
[22]  (2025). *Show HN: I built an MCP server using Cloudflare's code mode pattern*. . https://news.ycombinator.com/item?id=45405584 (Retrieved: 2026-10-26)
[23]  (2025). *Code Mode: the better way to use MCP*. . https://blog.cloudflare.com/code-mode/ (Retrieved: 2026-10-26)
[24]  (2026). *Local Code Mode: Save 65-99% Context for MCP*. . https://gist.github.com/chenhunghan/27ef2fece7329ac7b88f815ce0c0fbb1 (Retrieved: 2026-10-26)
[25]  (2026). *Is MCP Dead? What We Learned on MCP, CLI, and Skills*. . https://milvus.io/blog/is-mcp-dead-cli-and-skills-for-ai-agents.md (Retrieved: 2026-10-26)
[26]  (2026). *Context7 Without Context Bloat*. . https://upstash.com/blog/new-context7 (Retrieved: 2026-10-26)
[27]  (2024). *Llms.txt (llmstxt.org proposal)*. . https://news.ycombinator.com/item?id=41439983 (Retrieved: 2026-10-26)
[28]  (2026). *I crawled 1,500 sites: 30% block AI bots, 0.2% use llms.txt*. . https://websiteaiscore.com/blog/case-study-1500-websites-ai-readability-audit (Retrieved: 2026-10-26)
[29]  (2026). *Tell HN: Anthropic no longer allowing Claude Code subscriptions to use OpenClaw*. . https://news.ycombinator.com/item?id=47633396 (Retrieved: 2026-10-26)
[30]  (2026). *upstash/context7 monorepo (pinned commit d956160, HEAD 2026-10-26)*. . https://github.com/upstash/context7/tree/d9561600ad502cd72ab8bae4f9172f434aadf4d1 (Retrieved: 2026-10-26)
[31]  (2026). *Context7 API Guide (pinned commit d956160)*. . https://github.com/upstash/context7/blob/d9561600ad502cd72ab8bae4f9172f434aadf4d1/docs/api-guide.mdx (Retrieved: 2026-10-26)
[32]  (2026). *Context7 Plans & Pricing (retrieved 2026-10-26)*. . https://context7.com/plans (Retrieved: 2026-10-26)
[33]  (2023). *DocPrompting: Generating Code by Retrieving the Docs*. . https://arxiv.org/abs/2207.05987 (Retrieved: 2026-10-26)
[34]  (2023). *shuyanzhou/docprompting — official data, models and retrieval code*. . https://github.com/shuyanzhou/docprompting (Retrieved: 2026-10-26)
[35]  (2024). *Retrieval meets Long Context Large Language Models*. . https://arxiv.org/abs/2310.03025 (Retrieved: 2026-10-26)
[36]  (2024). *Retrieval Augmented Generation or Long-Context LLMs? A Comprehensive Study and Hybrid Approach (Self-Route)*. . https://arxiv.org/abs/2407.16833 (Retrieved: 2026-10-26)
[37]  (2024). *Lost in the Middle: How Language Models Use Long Contexts*. . https://arxiv.org/abs/2307.03172 (Retrieved: 2026-10-26)
[38]  (2025). *Context Rot: How Increasing Input Tokens Impacts LLM Performance*. . https://www.trychroma.com/research/context-rot (Retrieved: 2026-10-26)
[39]  (2021). *BEIR: A Heterogeneous Benchmark for Zero-shot Evaluation of Information Retrieval Models*. . https://arxiv.org/abs/2104.08663 (Retrieved: 2026-10-26)
[40]  (2024). */llms.txt — a proposal to provide information to help LLMs use a website*. . https://www.answer.ai/posts/2024-09-03-llmstxt.html (Retrieved: 2026-10-26)
[41]  (2024). *The /llms.txt file, v2*. . https://llmstxt.org (Retrieved: 2026-10-26)
[42]  (2026). *Live adoption check of llms.txt endpoints (Anthropic, Next.js, React, Mintlify, MDN)*. . https://docs.anthropic.com/llms.txt (Retrieved: 2026-10-26)
[43]  (2026). *Next.js documentation llms.txt with version pinning*. . https://nextjs.org/docs/llms.txt (Retrieved: 2026-10-26)
[44]  (2026). *Mintlify docs llms.txt with recursive _llms indexes and .md endpoints*. . https://mintlify.com/docs/llms.txt (Retrieved: 2026-10-26)
[45]  (2026). *DevDocs — API documentation browser (offline, per-version docs)*. . https://github.com/freeCodeCamp/devdocs (Retrieved: 2026-10-26)
[46]  (2026). *GitMCP — remote MCP server for GitHub project documentation*. . https://gitmcp.io/docs (Retrieved: 2026-10-26)
[47]  (2025). *DeepWiki (Cognition) — AI-generated repo wikis with source citations*. . https://docs.devin.ai/work-with-devin/deepwiki (Retrieved: 2026-10-26)
[48]  (2011). *An Empirical Study on Evolution of API Documentation*. . https://dl.acm.org/doi/10.5555/1987434.1987473 (Retrieved: 2026-10-26)
[49]  (2026). *LLMs.txt: Why Brands Rely On It and Why It Doesn't Work*. . https://seranking.com/blog/llms-txt (Retrieved: 2026-10-26)
[50]  (2025). *Model Context Protocol specification: Tools (2025-06-18)*. . https://modelcontextprotocol.io/specification/2025-06-18/server/tools (Retrieved: 2026-10-26)
[51]  (2024). *SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering*. . https://arxiv.org/abs/2405.15793 (Retrieved: 2026-10-26)
[52]  (2026). *Terminal-Bench 4.0 leaderboard*. . https://www.tbench.ai/ (Retrieved: 2026-10-26)
[53]  (2026). *github/github-mcp-server README (retrieved 2026-10-26)*. . https://github.com/github/github-mcp-server (Retrieved: 2026-10-26)
[54]  (2025). *Code execution with MCP*. . https://www.anthropic.com/engineering/code-execution-with-mcp (Retrieved: 2026-10-26)
[55]  (2026). *Programmatic tool calling (Claude Platform Docs)*. . https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling (Retrieved: 2026-10-26)
[56]  (2025). *Introducing advanced tool use for Claude*. . https://www.anthropic.com/research/advanced-tool-use (Retrieved: 2026-10-26)
[57]  (2026). *Tool search tool (Claude Platform Docs)*. . https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool (Retrieved: 2026-10-26)
[58]  (2024). *Executable Code Actions Elicit Better LLM Agents (CodeAct)*. . https://proceedings.mlr.press/v235/wang24h.html (Retrieved: 2026-10-26)
[59]  (2025). *RAG-MCP: Mitigating Prompt Bloat in LLM Tool Selection via Retrieval-Augmented Generation*. . https://arxiv.org/abs/2505.03275 (Retrieved: 2026-10-26)
[60]  (2025). *MCP-Zero: Active Tool Discovery for Autonomous LLM Agents*. . https://arxiv.org/abs/2506.01056 (Retrieved: 2026-10-26)
[61]  (2026). *docs-mcp-server (Grounded Docs) — documentation indexing and search MCP server*. . https://github.com/arabold/docs-mcp-server (Retrieved: 2026-10-27)
[62]  (2026). *mcpdoc — llms.txt MCP server (LangChain; archived 2026-09-18)*. . https://github.com/langchain-ai/mcpdoc (Retrieved: 2026-10-27)
[63]  (2026). *Context7 Quietly Slashed Its Free Tier by 92%*. . https://blog.devgenius.io/context7-quietly-slashed-its-free-tier-by-92-16fa05ddce03 (Retrieved: 2026-10-27)
[64]  (2026). *I Built a Context7 Local-First Alternative With Claude Code*. . https://medium.com/@moshesimantov/i-built-a-context7-local-first-alternative-with-claude-code-eb14c9fd654f (Retrieved: 2026-10-27)
[65]  (2026). *Web Scraping AI Training Data: Legal & Practical Guide*. . https://tendem.ai/blog/web-scraping-ai-training-data-legal-practical-guide (Retrieved: 2026-10-27)

---

## Appendix: Methodology

### Research Process

This run followed an 8-phase deep-research pipeline across two sessions (2026-10-26). Phase 1–2 fixed scope, source hierarchy (peer-reviewed/spec/pinned-code = A; reproducible engineering = B; vendor measurements = C; anecdotes = D), and the normalization plan (within-study ratios only; no cross-benchmark pooling). Phase 3 ran six parallel investigation paths (subagents plus main-thread primary-source work): pinned-source audits via GitHub API (upstash/context7 at d956160; scalekit-inc/mcp-vs-cli-benchmark at f131dd8; github/github-mcp-server README), official pricing/spec pages, HN Algolia queries, GitHub issue-tracker counts, and targeted paper retrievals. Phase 4 triangulated every core claim across ≥3 clusters where available (e.g., schema-bloat mechanism: Scalekit raw data + Lunar.dev argument + Speakeasy measurement + MCP spec text). Phase 4.5 refined the outline after retrieval revealed that (a) only one controlled CLI-vs-MCP benchmark exists, elevating its audit to a standalone finding, and (b) llms.txt adoption data contradicted initial framing as a solution rather than an ingest source. Phase 5–7 synthesized, critiqued against the pre-registered quality gates, and refined. Phase 8 assembled this report progressively.

### Sources Consulted

**Total sources:** 65 registered in `sources.jsonl` (stable sha-based IDs; 60 registered in the original 2026-10-26 session, 5 added during the 2026-10-27 revision — see `batch_008.json`; per-record registration timestamps reflect the original session date), with 150 evidence rows in `evidence.jsonl` carrying exact quotes and locators. **Source types:** academic papers/proceedings (16), official specs/docs (7), code repositories (9), vendor docs/pricing (2), engineering/practitioner blogs with methods (11), industry blogs (7), issue trackers (4), forums/HN (5), vendor marketing (labeled C/D, 4). **Temporal:** 2011–2026, concentrated 2023–2026.

### Verification Approach

**Triangulation:** core architectural claims require three independent clusters (met for schema-bloat mechanism, Context7 client/server split, docs-retrieval efficacy); single-source claims are flagged in-line. **Raw-data audit:** the Scalekit benchmark's 68 stored result files were recomputed locally (medians, ratios, contains-verifier, cost model) — discrepancies with the blog are reported in Finding 2. **Pinned provenance:** every repository claim cites a commit SHA and file path. **Credibility:** vendor-only quantitative claims are labeled and never used as anchors. **Quality control:** claim-extraction, citation, and claim-support validators run on the final report; the final outputs are retained verbatim in `validator_final_pass.txt`. The claim ledger's `cited_source_ids` and `evidence_ids` linkage is populated programmatically from inline citations and evidence-store source IDs. The automated support checker is lexical: it marks process descriptions, transitions, and labeled synthesis as "unsupported by quote" even when they carry inline citations; those flags are expected; the genuinely unsupported factual claims that review surfaced were fixed before delivery, and the residual unsupported set is dominated by bibliography, metadata, and transition sentences. An external council review (three judges, two adversarial rounds, 2026-10-26/27) returned WARN on the first draft; this revision implements its fix list — and the first draft's QC sentence, which overstated the automation, was itself one of the corrected defects.

### Claims-Evidence Table

| Claim ID | Major Claim | Evidence Type | Supporting Sources | Confidence |
|----------|-------------|---------------|-------------------|------------|
| C1 | Context7 repo is a thin client over a closed hosted index; two MCP tools; MIT; REST API behind API keys; free tier 1,000 calls/mo | Pinned code + vendor docs | [30][31][32] | High |
| C2 | In the one controlled CLI-vs-MCP benchmark, MCP (43-tool catalog) cost 4.3–32× more tokens per task on identical GitHub tasks | Raw-data recomputation | [1][2] | High (mechanism), Medium (generalization) |
| C3 | Token bloat is a loading-policy choice; MCP does not mandate full-catalog exposure | Spec text + vendor implementations + measurement | [50][53][18][6] | High |
| C4 | Code-as-action helps large/compositional action spaces (with a documented negative result on tau2-bench); no evidence it helps a 2-tool docs domain | Academic + vendor docs (labeled) | [58][59][55][18] | Medium |
| C5 | Docs-retrieval quality and version pinning dominate outcomes; small cited chunks beat stuffing | Peer-reviewed | [33][35][36][37][38][39] | High |
| C6 | Hosted docs indexes exhibit structural staleness (documented for Context7 and, historically, JDK docs); local version-pinned corpora remove the crawl-freshness failure mode for pinned versions | Issue tracker + empirical SE literature | [14][15][16][48] | Medium (2 independent clusters under this report's rubric — the issue-tracker citations are one repository) |
| C7 | Docs content is an untrusted channel (injection, poisoning) | Peer-reviewed + spec | [9][10][50] | High |
| C8 | No controlled dev-docs interface benchmark exists; a normalized experiment is required | Absence finding | [7][8][11] + survey | High (as of 2026-10-26) |

**Confidence levels:** High = 3+ independent clusters, consistent; Medium = 2 clusters or single high-quality source with caveats; Low = single source or contradictions. C1–C5 and C7–C8 are High under this rubric; C6 is Medium (two clusters).

**Store mapping (auditable):** each row maps to its ledger entry in `claims.jsonl` — C1→`920454ed8f7e4ecc` (executive_summary), C2→`d8460463782031b8` (executive_summary), C3→`64112eee999836b5` (finding_3), C4→`7615592ba7927591` (finding_4), C5→`c755a7cbaa266fa0` (executive_summary), C6→`88c5f49efb614aac` (finding_1), C7→`52ffa4244c838539` (finding_7), C8→`b79093ae6b2b8aa9` (finding_8). All eight mapped claims carry populated `cited_source_ids` and `evidence_ids` linking to `evidence.jsonl` rows (machine-checked; see `c_table_map.json`). The ledger itself is append-only and retains superseded claim versions from earlier drafts.

---

## Report Metadata

**Research Mode:** Deep | **Total Sources:** 65 registered | **Word Count:** ~9,000 (main analysis) | **Research Duration:** two sessions 2026-10-26 + revision 2026-10-27 | **Generated:** 2026-10-26; **Revised:** 2026-10-27 (external council fix list applied) | **Validation Status:** final validator outputs retained in `validator_final_pass.txt`; council reports in `council/`
