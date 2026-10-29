# Deep Research: Context7-style Dev-Docs Tooling for Agentic Workflows
## MCP vs CLI vs Code-Mode vs llms.txt — Evidence Review & Normalized Comparison

*Compiled 2026-10-26. All claims sourced from vendor engineering blogs, GitHub repos,
Hacker News / Reddit threads, and peer-reviewed preprints. Vendor-reported numbers are
flagged as such.*

---

## 1. How Context7 actually works (internals)

Context7 (Upstash) is not magic — it is a **hosted retrieval pipeline + a 2-tool MCP facade**:

1. **Ingestion pipeline** (repo: upstash/context7-legacy): parse docs (.md/.mdx/.rst/.ipynb)
   -> use LLMs to extract code snippets + write concise metadata -> embed into a vector DB
   (Upstash Vector). Result: a registry of ~104,000+ libraries.
2. **Runtime** (2 MCP tools only): `resolve-library-id` (fuzzy name -> library ID) and
   `get-library-docs` / `query-docs` (topic query -> ranked snippets). A community deep-dive
   on r/ClaudeAI reconstructed the loop: the agent calls resolve-library-id with a best-guess
   keyword, picks a match, then calls get-library-docs one or more times until satisfied.
3. **2026 architecture revision ("Context7 without context bloat")**: the server now does
   its own multi-step search/filtering internally and returns only the pieces that answer
   the question, instead of making the LLM iterate.
   - Reported (vendor-internal benchmark, 80+ coding questions across Next.js/FastAPI/
     Playwright/tRPC/Docker; Claude Haiku simulated agent; Claude Sonnet as LLM judge):
     - Tokens: ~9.7k -> ~3.3k avg context tokens/query (**-65%**)
     - Latency: 24s -> 15s (**-38%**)
     - Tool calls: 3.95 -> 2.96 per query (**-30%**)
     - Quality: slightly improved on their 1–10 helpfulness judge scale.
4. **Cost structure**: free tier was cut from ~200 req/day (~6,000/mo) to **500/mo** in
   Jan 2026 (later raised to **1,000/mo**, +20 bonus calls/day after cap). Pro: $10/seat/mo
   for 5,000 calls ($10 per extra 1,000); private repo parsing $5/M tokens. There is also a
   **CLI + Skills mode** (`npx ctx7 setup`) — i.e., Context7 itself now ships a non-MCP path.
5. **Security surface**: because the MCP tools are read-only content pipes, the attack
   vector is *context poisoning* — poisoned "custom rules" in a registry entry ride into the
   agent's context and the agent executes them (Noma Security "ContextCrush" write-up).

## 2. Code-mode MCP servers (execution-as-interface)

Pattern: instead of exposing N tool schemas, expose a tiny meta-interface; the model writes
code; a sandbox runs it; only final results return to context.

| Implementation | Model writes | Runs in | Reported evidence |
|---|---|---|---|
| Anthropic "code execution with MCP" | TypeScript | client-side env | Google Drive->Salesforce workflow: **150k -> 2k tokens (-98.7%)** |
| Cloudflare Code Mode (`@cloudflare/mcp`) | JS | isolated Worker | Full Cloudflare API: **~244k tokens w/ endpoint-per-tool vs ~1k in code mode**; results capped ~6k tokens/call |
| Bifrost / Maxim gateway | Python (Starlark) | sandbox in gateway | input tokens **-58.2% (96 tools), -84.5% (251), -92.8% (508)**, 100% pass rate held; notes ~7k tokens for a typical 4-server MCP setup just in tool defs |
| Speakeasy Dynamic Toolsets | natural language search | server | up to **160x** token reduction vs static toolsets, 100% success maintained (vendor bench) |
| Stainless SDK-code-mode | TS | sandbox | 31-eval banking-API suite; SotA accuracy vs Cloudflare code mode, Anthropic programmatic tool calling, and dynamic meta-tools |
| FastMCP `CodeMode` transform / mcp-server-code-execution-mode / mcp-use / Port of Context (Rust CLI) | Python/JS | Podman/Deno/E2B/local | OSS implementations; Port of Context is explicitly a **local CLI-first** code-mode runner |

Criticisms worth noting: an AWS-heroes piece calls code mode "the long-tail escape hatch,
not the front door" — validation/approval tokens, policy config, and sandboxing add real
operational complexity; Anthropic itself flags sandbox/infra overhead as the trade-off.

## 3. CLI-for-dev-docs evidence

- **gh vs GitHub MCP** (the exact comparison you raised): HN practitioners report GitHub
  MCP's tool spec alone burns **~54k tokens**; `gh` costs ~0 schema tokens because the model
  already knows CLI patterns from pretraining, and output is composable (`| jq`, `| head`).
  One benchmark (browser automation, cited by CircleCI) measured **+33% token efficiency and
  77 vs 60 task-completion score for CLI vs MCP**, with the gap largest in multi-step
  debugging where MCP ran out of context mid-task.
- **Hardened practitioner consensus (HN "When does MCP make sense vs CLI?")**: hybrid is the
  norm — MCP for stateful external integrations with auth/session needs; CLI for file/git/
  build/test work; let agents write shell scripts to chain operations.
- **Failure-mode asymmetry**: CLI failures are explicit (exit code + stderr); MCP failures
  can be silent. Reproducibility: "when Claude does something unexpected with Jira, I can
  run the same command and see exactly what it saw."
- **Dev-docs-specific CLI/standards layer**:
  - `llms.txt` spec (llmstxt.org): small index file at docs root, links to LLM-friendly
    (.md) pages; "detail lives behind the links, fetched only when needed." Adopted by
    X, Ping Identity, Fern (auto-generated), GitHub (as an API-style variant).
  - `get-llms` (npm CLI): fetches llms.txt for all package.json deps to local disk —
    zero context cost at query time, works offline; explicitly positions itself as the
    "simpler, cheaper" alternative to Context7.
  - `mcpdoc` (LangChain): reads llms.txt and exposes fetch_docs as MCP — the MCP/CLI hybrid.
  - Statespace: search engine over llms.txt docs, exposed via web/SDK/MCP/**CLI**.
  - Known weakness (Google Cloud blog): llms.txt quality is inconsistent — ADK's own
    llms.txt contained no links, breaking navigation; llms-full.txt is often too massive.
  - GitHub's API-style llms.txt shows the opposite trade-off: richer capability, but only
    capable multi-hop agents can use it; simple fetch-subagents can't.

## 4. Academic grounding

- **Gorilla (arXiv:2305.15334, Berkeley)**: retrieval-grounded API calling (docs fed at
  test time) lets a 7B finetuned model **surpass GPT-4** on API-call generation and
  "substantially mitigates hallucination" — the foundational evidence that
  *docs-in-context beats parametric memory* for library APIs.
- **ToolRet (arXiv:2503.01763)**: 7.6k retrieval tasks over a 43k-tool corpus; even strong
  IR models perform poorly at tool retrieval, and **low retrieval quality directly degrades
  task pass rate** — evidence that retrieval quality, not protocol choice, is the binding
  constraint.
- **RULER** (long-context effective length) — models degrade well before their nominal
  context limits; stuffing full docs is worse than targeted retrieval.
- **ORD-QA** (IEEE/ACM ICCAD 2025): domain docs QA benchmarks need customized RAG
  (fine-tuned embeddings + reranker) to beat off-the-shelf pipelines — generic chunk+embed
  is not enough for vertical docs.

## 5. Normalized benchmark table (beware: different workloads)

All numbers below are *input/context-token reduction vs each vendor's own baseline* —
they are NOT directly comparable across rows. Normalized columns make the workload explicit.

| Approach | Workload | Baseline | Result | Normalized metric | Quality held? | Source type |
|---|---|---|---|---|---|---|
| Context7 v2 vs v1 | 80+ real coding questions, 2-tool docs MCP | v1: 9.7k tokens, 24s, 3.95 calls/query | 3.3k tokens, 15s, 2.96 calls/query | -65% tokens/query, -38% latency, -30% calls | Yes (LLM judge, slight improvement) | Vendor-internal |
| Anthropic code execution | 1 data-move workflow (Drive->Salesforce) | 150k tokens | 2k tokens | -98.7% | Implied | Vendor blog |
| Cloudflare code mode | Full Cloudflare API surface (~2,500 endpoints) | ~244k tokens (endpoint-per-tool mode) | ~1k tokens | -99.6% | n/a (same API) | Vendor repo |
| Bifrost Code Mode | 96 / 251 / 508 connected tools | static tool defs | meta-tools + code | -58% / -84.5% / -92.8% input tokens | Yes (100% pass rate) | Vendor bench |
| Speakeasy dynamic toolsets | large API tool catalogs | static toolsets | search + JIT schemas | up to 160x (up to -99.4%) | Yes (100% success) | Vendor bench |
| CLI vs MCP (browser automation) | multi-step debugging tasks | MCP client | CLI subprocess | +33% token efficiency; 77 vs 60 completion | Higher for CLI | Third-party benchmark (cited) |
| GitHub MCP spec load | connecting to 1 server | — | ~54k tokens | fixed connection cost | — | Practitioner-measured (HN) |
| 4-server MCP setup | typical Claude Code session | — | ~7k tokens/message in tool defs | recurring per-message tax | — | Gateway vendor measurement |
| Solo.io progressive disclosure | MCP gateway tool ads | full schemas | progressive disclosure | -91% tokens | — | Vendor bench |
| FastMCP + TOON-style encodings | structured payloads | verbose JSON | compact encodings | up to -98% on repetitive data | — | Community |

**Normalization caveats**: (1) %-reduction numbers shrink the bigger the baseline bloat —
a 98.7% cut of a pathological 150k workflow and a 65% cut of an already-lean 9.7k docs
query are different absolute savings; (2) "quality held" claims are vendor-judged except
the CLI/MCP benchmark; (3) tool-count scaling (Bifrost) measures the *schema tax*, which is
near-zero for Context7's 2-tool design anyway.

## 6. Scenario-based decision matrix

| Your scenario | Best-fit interface | Why |
|---|---|---|
| Interactive coding assistant (inner loop), common libraries | **CLI / skills** (or 2-tool MCP with tight outputs) | Schema tax ~= 0; model pretrained on CLI; composability; cheapest context |
| Huge API surface / many tools (500+) | **Code mode or dynamic toolsets** | Token cost bounded by what's read, not what exists |
| Stateful external services needing auth, audit, sessions | **MCP** | Centralized auth, structured I/O, audit logs; CLI reproduces this badly |
| Batch/CI agent runs at scale | **CLI + local index** | No per-request metered API; deterministic; cacheable; auditable |
| Private/internal docs | **Self-hosted index + thin MCP or CLI** | Context7 Pro charges for this; local embeddings avoid per-token parsing fees |
| Rapidly-evolving APIs near model cutoff | **Live retrieval (MCP/CLI over fresh index)** | HN: MCP "stop gap" value highest here (e.g., Svelte 4->5 breakages) |
| Prompt-cache-sensitive long sessions | **Prefer CLI/static files** | Dynamic MCP tool injection mid-conversation busts prompt caches (HN) |

## 7. Recommendation for building a Context7-class tool

The evidence converges on a **hybrid, CLI-first design**:

1. **Core = local CLI + local index** (parse docs like Context7's pipeline; embed in a local
   vector store; `tool docs query <lib> <topic>`). Zero schema tax, zero per-request metered
   cost, works offline/CI, gh-style composability. This answers your GitHub-MCP-vs-gh
   intuition: for docs, the CLI is the right spine.
2. **Thin MCP wrapper** exposing exactly 2 tools (resolve + query) that shell out to the same
   core, for MCP-native clients. Cost: ~2 small tool defs — the schema tax that kills
   GitHub-MCP-style servers barely exists at 2 tools.
3. **Adopt llms.txt as an ingestion source** (plus GitHub repo crawling as fallback) — it is
   becoming the publisher-sanctioned channel (X, Ping, Fern, GitHub), but handle the
   quality variance the Google Cloud blog documented.
4. **Server-side result refinement** (Context7 v2 lesson): do the iterate-and-filter loop
   inside your tool; return only answer-bearing snippets (~3k tokens/query target). Consider
   optional code-mode execution for multi-hop docs assembly (Anthropic/Cloudflare pattern),
   but treat it as an escape hatch, not the front door.
5. **Benchmark like Context7 does**: 80+ realistic questions, simulated agent harness,
   measure tokens/query, tool calls/query, latency, plus LLM-judged answer quality —
   and report absolute tokens/query, not just % reductions, so your numbers are comparable
   to the table above.
6. **Security**: you are a content pipe — sanitize/segment docs against instruction
   injection (ContextCrush showed even 2 read-only tools are a viable poisoning vector).

## 8. Is MCP the right choice at all?

For a docs tool specifically, the verdict from evidence: **the protocol matters far less
than the retrieval loop design.** Context7 works well not because it is MCP but because it
(a) is only 2 tools, (b) does server-side filtering, (c) returns pre-digested snippets.
A CLI with the same retrieval core delivers the same context economics minus the schema tax,
at the cost of losing plug-and-play discovery for MCP-native clients — which is why shipping
both (one core, two facades) is the dominant emerging pattern (ctx7 CLI+Skills, mcpdoc,
Statespace, Port of Context all do exactly this).
