# Deep Research: Context7 Internals, Code Mode, CLI, and Better Doc-Design for Agents

**Channels used via agent-reach:** GitHub (`gh` CLI — repo metadata, READMEs, star counts), Hacker News (Algolia public API — stories + comment trees), web/Jina Reader (vendor eng blogs, docs, pricing), Bilibili (`bili-cli` — Chinese-dev sentiment), V2EX (public API). Reddit and Twitter/X were **unavailable** (no explicit cookies configured; `mcporter`/Exa and OpenCLI not installed), so community sentiment here comes from HN + Bilibili + independent blogs. I flag that as a limitation rather than papering over it.

**Dating and sourcing note.** All counts (GitHub stars, HN points/comments, video plays) and prices are **as of 2026-10-26** and will drift. Community claims retrieved through `agent-reach` search (Bilibili, V2EX) are labelled as *search results* rather than linked to the individual item, because direct item URLs were not always retrievable; vendor claims link to the specific post.

---

## 0. Read this first: how to normalize the numbers (they are NOT comparable)

The single biggest trap in this space is that every headline number measures a *different thing* against a *different baseline*. Mapping them:

| Claim | Source | What it actually measures | Baseline it beats | Holds task success constant? |
|---|---|---|---|---|
| "99.9% reduction, ~1,000 tokens" | Cloudflare | **Schema/init tokens only** | A hypothetical native MCP for a 2,500-endpoint API — **1.17M tokens** (figure is source-inconsistent; see note below) | No |
| "98.7%, 150k→2k tokens" | Anthropic | **Tool-definition + intermediate-result tokens** | Naive "load all tools upfront" | No (illustrative workflow) |
| "65% tokens ↓ (9.7k→3.3k), 38% latency ↓" — source-reported; rounded averages imply ≈66% | Context7 | **Estimated tokens in returned documentation per question** (not total model context) | Context7's *own previous architecture* | No — vendor-internal quality scoring, **not** a success-preservation claim |
| "96.7%/91.2% input token reduction, up to 160x" | Speakeasy | **Input tokens**, 40→400 tools | Static toolset (all tools loaded), same tasks | Single-arm — 100% reported for its own approach; baseline success not stated |
| "100% success, MCP 2.5% cheaper / 23% faster" | Mario Zechner | **Cost + time + success**, **terminal-control tasks** (not docs lookup) | Same tool as CLI vs MCP | Design controlled; **per-arm denominator unstated** |
| "GitHub MCP = 26,644 schema tokens; CLI = 0" | Blocks.ai | **Init tokens only**, deterministic | Same task, different transport | N/A (pure measurement) |
| "Performance degrades as input length grows" | Chroma "Context Rot" | **Accuracy vs input length**, 18 models | Held-out controlled tasks | No — holds task **complexity** constant, measures *degradation* (see rule 2) |

> **Source-discrepancy note (Cloudflare denominator).** The Cloudflare article body states an equivalent native MCP server "would consume **1.17 million tokens**". The *same article's* meta description says "over **2 million** tokens". (I also saw larger secondary figures cited, but could not retrieve a primary item link for them, so I do not quote a precise value.) Because 1.17M is the denominator of the headline 99.9% reduction, that headline should be read as **approximate and source-inconsistent**. I use 1.17M (the figure the article's own calculation is stated against) and flag the discrepancy rather than silently adopting one value.

**Rules I applied to avoid misleading you:**
1. **Vendor "X% saved" ≠ transferable.** Context7's 65% is vs *itself*. Cloudflare's 99.9% is vs a *straw-man MCP*. Never compare across rows.
2. **Matched success evidence is thin — and thinner than it looks.** Zechner's controlled design reports 100% for both arms (CLI vs MCP), but the source does **not** state the per-arm run denominator, so even this is **not a clean matched comparison**. Speakeasy reports **100% for its own dynamic toolset** but not the static-toolset baseline's success rate — *single-arm* evidence. Chroma holds task **complexity** constant while measuring accuracy degradation — a *different* control, **not** evidence that a context-reduction approach preserves success. Context7's quality figure is vendor-internal scoring, not success preservation.
3. **Separate "init/schema tokens" from "per-call result tokens" from "intermediate tokens."** They have different fixes.
4. **Context differs.** "40 tools" ≠ "2,500 endpoints"; "terminal control" ≠ "docs lookup." I call out the scenario for each.
5. **Recency:** Context7's own blog (June 2026) and Cloudflare's server-side Code Mode (Feb 2026) supersede their 2025 versions.
6. **Single-vendor data points are labelled as such.** A measurement one vendor publishes about its *own* artifact is an illustration of what is possible, not a typical or general property of the approach.

---

## 1. Context7 internals — what it actually is

**Corpus & pipeline.** Not "just a RAG." Each library goes through: **Parse** (extract code snippets from docs) → **Enrich** (an LLM adds short explanations + metadata) → **Vectorize** (embeddings) → **Rerank** (custom scoring) → **Cache** (Redis). — [Upstash, "Introducing Context7"](https://upstash.com/blog/context7-llmtxt-cursor)

**Two pivots that define the current product:**

**(a) Server-side reranking (the "no context bloat" rewrite).** Originally it was a 2-step LLM-driven search (`resolve-library-id` → `get-docs`) where the model kept re-calling the server, each call inflating the window. They moved filtering/ranking to *their* infrastructure with fast reranking models, and gave both API methods a `query` string so the reranker can prune. Published results: **returned-documentation tokens per question 9.7k → 3.3k (−65%, source-reported; the displayed rounded averages imply ≈66%)**, **latency 24s → 15s (−38%)**, **tool calls 3.95 → 2.96**, quality slightly up. Note on the arithmetic: the source *states* a 30% tool-call reduction but the averages it displays imply ≈**25%** ((3.95 − 2.96) / 3.95), and it states −65% tokens while 9.7k → 3.3k implies ≈**66%**; I report both the source-stated and the implied figures rather than reconciling them, because the discrepancies are in the source and do not reconcile with the displayed averages. Their own framing: *"filtering and ranking documentation is cheaper, faster, and more predictable on our infrastructure than asking a general-purpose reasoning model to do it."* — [Upstash, "Context7 Without Context Bloat"](https://upstash.com/blog/new-context7)

**(b) The self-healing research flywheel.** This is the part **not shown in the implementations reviewed here**. They tried **synchronous** research (spin a sandbox, clone the repo, run an agent to answer the query in parallel with vector search). It worked — hard-question score **4 → 6.5/10** — but cost **~30× per query** and blew latency to **3–4 minutes**, so they reverted it in two days. Instead they made it **asynchronous**: their always-on benchmark scores every answer; low scores trigger background repo-clone research; answers are written to a `dynamic-context` vector index with an **expiry** so stale entries regenerate. Result: hard-question score **4.0 → 6.4/10**, and **~17% of served snippets now come from `dynamic-context`** — this is Context7's **production-use signal** (how much the index is *used*), not proof of answer quality: the benchmark that triggers the research is the same loop that scores it, and they say so explicitly. The reported score increase is **vendor-reported** and depends on that internal benchmark — not independent validation. — [Upstash, "How Context7 Researches Its Own Weak Spots"](https://upstash.com/blog/context7-research)

**Interface surface.** MCP tools `resolve-library-id` and `query-docs`; API `searchLibrary` + `getContext`; and — importantly for you — it now ships **two modes from one setup**: **CLI + Skills** (`ctx7 library`, `ctx7 docs`) *with no MCP at all*, or MCP. Hosted at `https://mcp.context7.com/mcp`, OAuth 2.0, free API key for higher limits. Packages: `@upstash/context7-mcp`, `ctx7` CLI (Node ≥22.13), `@upstash/context7-sdk`, AI-SDK tools. Repo: **63,720 stars**, MIT, created 2025-03-26. — [github.com/upstash/context7](https://github.com/upstash/context7)

**Pricing / the "not completely free" problem — with the real history:**

| | Free | Pro |
|---|---|---|
| Price | $0 | **$10 / seat / mo** |
| Included API calls (Search + Context) | **1,000 / month** | 2,000 / seat |
| Overage | Blocked; **20 bonus calls/day** while blocked | $5 / 1,000 |
| Private repos | ✗ | $5 / 1M tokens |
| Enterprise | — | Custom: SOC-2, SSO, **self-hosted** |

— [context7.com/plans](https://context7.com/plans)

The free tier was cut hard: **~200 requests/day (~6,000/mo) → 500/mo (−92%)** on 13 Jan 2026, then bumped to **1,000/mo** on 16 Jan 2026 after backlash. The co-founder's stated reason is the crux for you: **"they've added an LLM call on every request for better quality/less context bloat, which increased their costs."** — [Dev Genius, "Context7 Quietly Slashed Its Free Tier by 92%"](https://blog.devgenius.io/context7-quietly-slashed-its-free-tier-by-92-16fa05ddce03)

> **So: a plausible source of Context7's differentiation is *not* MCP** (this is the report's **hypothesis** about where the value sits — not an evidence-backed claim of defensibility, since no comparative moat evidence is presented and the quality figures are vendor-reported). What it plausibly rests on is (1) a maintained, version-pinned, LLM-enriched corpus, (2) a server-side reranker that spends *their* cheap-model budget so *you* don't spend your expensive-model budget, and (3) a flywheel that repairs its own weak spots. MCP is just the delivery pipe — and they even offer a no-MCP CLI mode.

**Independent critique, same source, high cred.** Mario Zechner — **author of the MCP TypeScript SDK** — on Context7 specifically:

> *"Another popular choice is Context7... I found some of my own libraries in there and the snippets are utterly useless. What works better is having Claude clone the dependency's repository, then let a sub-agent analyze it and generate a concise user manual for the task at hand."* — [mariozechner.at](https://mariozechner.at/posts/2025-08-15-mcp-vs-cli/)

Note the irony worth internalizing: **his "better way" is Context7's own async research flywheel, done at the edge instead of centrally.** The two accounts suggest the *same design pattern* — not two independent validations of one result. Context7's score comes from its own benchmark loop, so it is not independent confirmation of Zechner's proposal.

---

## 2. Code Mode for dev docs — notable reported results

**Client-side Code Mode (Cloudflare, Sep 2025).** Convert MCP tools → a **TypeScript API**; the model writes code against it in a sandbox; only `console.log()` output returns. Core insight: *"LLMs have seen a lot of code. They have not seen a lot of 'tool calls'."* Code lets you **compose** — intermediate results never round-trip through the model. Also: **server-side reranking isn't the only trick — code execution filters before returning.** — [Cloudflare, "Code Mode: the better way to use MCP"](https://blog.cloudflare.com/code-mode/)

**Anthropic's independent version (Nov 2025).** Present MCP servers as a **file tree of `.ts` files**; agent lists/reads only what it needs (progressive disclosure). Two token drains fixed: (1) tool definitions loaded upfront, (2) intermediate results flowing through context (their example: a 2-hour transcript passing through twice ≈ +50k tokens). **150,000 → 2,000 tokens (98.7%)**. Adds: privacy-preserving (tokenize PII before it reaches the model), persistent state via files, and **saved skills** — agent writes working code once, reuses it. Explicitly warns: code execution needs sandboxing, resource limits, monitoring — *"operational overhead and security considerations that direct tool calls avoid."* — [Anthropic, "Code execution with MCP"](https://www.anthropic.com/engineering/code-execution-with-mcp)

**Server-side Code Mode (Cloudflare, Feb 2026) — two tools for 2,500+ endpoints.** The API is exposed as **`search()` + `execute()`** (~1,000 tokens). The model writes JS against the OpenAPI spec *inside* the sandbox; **the spec never enters context**. Native MCP equivalent = **1.17M tokens** (source-inconsistent — see the Section 0 note; the same page's meta description says "over 2 million"). Reduction **99.9%** (tiktoken-measured), therefore also approximate. Their comparison of context-reduction strategies is the most useful taxonomy I found:

| Approach | Mechanism | Stated limitation |
|---|---|---|
| Client-side Code Mode | model writes TS against typed SDK in client sandbox | agent must ship with sandbox access |
| **CLI** (their examples: OpenClaw, Moltworker + MCPorter) | convert MCP → CLI; self-documenting, progressive disclosure | **needs a shell**; "much broader attack surface" |
| Dynamic tool search | surface a small relevant subset | needs a maintained+evalled search function; matched tools still cost tokens |
| **Server-side Code Mode** | fixed 2 tools, work happens server-side | — |

They conclude server-side Code Mode "combines their strengths: fixed token cost regardless of API size, **no modifications needed on the agent side**, progressive discovery built in, and safe execution in a sandboxed isolate." (Read "fixed token cost" as the **tool-definition / initialization footprint** — the spec stays out of context — **not** a fixed *total* per-task token cost, which still depends on the generated code and returned results.) — [Cloudflare, "give agents an entire API in 1,000 tokens"](https://blog.cloudflare.com/code-mode-mcp)

**The non-code-mode competitor with the best-controlled numbers: Speakeasy "Dynamic Toolsets."** Three tools — `search_tools` (semantic **+ a category overview in the description**, the fix for "the model never thinks to search"), `describe_tools` (lazy schema load), `execute_tool`. Benchmarked across **40–400 tools**, simple + complex tasks, **Claude Sonnet 4.5**:

- input tokens **−96.7% (simple) / −91.2% (complex)**; total tokens **−96.4% / −90.7%**; vendor headline **"up to 160×"** — read this as a **maximum** across its benchmark cases, not an average: the reported average reductions imply ≈**30×** (simple) and ≈**11×** (complex) fewer input tokens. The source does not label which case produces the 160× maximum.
- **100% success rate** at every size and complexity
- costs: **2–3× more tool calls**, **~50% slower**
- key insight: **input schemas are 60–80% of static-toolset tokens** — so *lazy schema loading is the whole game*

Their explicit anti-Code-Mode argument: Code Mode works, but *"requires significant engineering investment and domain expertise that most teams simply don't have."* — [Speakeasy](https://www.speakeasy.com/blog/how-we-reduced-token-usage-by-100x-dynamic-toolsets-v2)

**Open-source, local Code Mode exists** too (portofcontext / `pctx`, Deno sandboxes; HN 76 pts). Confirmed in its thread: agents handle roughly **60–80 tools** before degradation; TypeScript chosen because *"it's the most token efficient way to pass types and signatures to an LLM"*; and the recurring plaint — *"47 tools = 141k tokens consumed before you write a single word."* — [HN 45917182](https://news.ycombinator.com/item?id=45917182)

---

## 3. CLI for dev docs — the evidence, and where it breaks

**The schema-overhead part of your GitHub MCP vs `gh` intuition is supported by this measurement.** Blocks.ai measured schema tokens deterministically with **Anthropic's `count_tokens` on `claude-sonnet-4`** (they ran it twice, identical results):

| Path | Tools | Init schema tokens | Tokens/tool |
|---|---|---|---|
| **CLI (no skill)** | – | **0** | – |
| Filesystem MCP | 14 | 2,614 | 187 |
| A lean 15-tool MCP | 15 | 3,185 | 212 |
| **SKILL.md (prose guidance)** | – | **12,755** | – |
| **GitHub MCP (default toolsets)** | 44 | **14,243** | 324 |
| **GitHub MCP (all toolsets)** | 85 | **26,644** | 313 |

— [Blocks.ai](https://blocks.ai/blog/mcp-vs-cli-context-window-cost)

Corroborating third-party figures they cite: Chawla **~55,000 tokens ≈ $0.16/session ≈ $1,600/day at 10k sessions**; Perplexity's CTO: MCP descriptions consumed **72% of the context window** before any work; Scalekit: MCP **4×–32× more tokens per call** than CLI; CircleCI's browser-automation benchmark: CLI **33% better token efficiency, 77 vs 60 task completion**. *(All four are **secondary figures reported by Blocks.ai**; I did not independently verify each primary source, so treat their units and baselines as unconfirmed — they may measure different quantities.)*

**And a write-up from the Chinese dev community (Bilibili):** *"MCP 太烧 token？我让 AI agent 改用 hf 命令行，实测平均省 6 倍 token"* — "MCP burns too many tokens? I switched my agent to the `hf` CLI, measured **~6× average token savings**." This is a **search-derived, un-linked item** (retrieved via `agent-reach`'s `bili-cli` search; the specific video URL was not retrievable), so treat the ~6× figure as an **unverified anecdote**, not a measurement. Context7 is also covered there, and an alternative framing shows up: *"比 Context7 更靠谱：Exa-code 靠 1B+ GitHub 与 Stack Overflow 减少幻觉."*

**But CLI is not a free lunch — the nuance that matters most (from a controlled A/B whose per-arm denominator is unstated):** All figures below are **source-reported and non-comparable** (the per-arm run count and aggregation are unverified), so read them as background on *whether* an overhead exists — not as a ranking.

Mario Zechner built the *same* tool as both MCP server and CLI and ran a **3 tasks × 4 tools × 10 reps** design, fresh Claude Code, cost via `/cost`, judged by Claude. **Denominator caveat (unresolved):** the source reports the design product (120) but does **not** state whether that counts paired trials, runs per interface, or total runs across both. Because the per-arm denominator is unverified, I report his success/cost/latency figures as **denominator-ambiguous** and **exclude this result from any matched CLI-vs-MCP success claim** — it is background on transport overhead only. **Scope: these were terminal-control tasks (debugging, a Python REPL, project analysis) — not documentation lookup.** The result is evidence about *transport overhead for terminal tools*, and should not be read as docs-retrieval success evidence:

- **MCP vs CLI = reportedly a wash on success.** Both reported **100% success**; MCP reported **23% faster** and **2.5% cheaper**.
- **MCP's hidden advantage — the *agent safety layer*.** In the same experiment, CLI calls triggering Claude Code's **malicious-command detection** reportedly consumed **1.3–2M Haiku tokens** versus **~35k** for the MCP equivalent, plus more round trips. (Denominator-ambiguous, like the figures above; these are **safety-scan tokens**, not retrieval tokens.)
- **Task complexity flips the winner:** simple tasks → `tmux` beats the bespoke tool by **10–22% on cost**; complex tasks → the bespoke tool is **39% cheaper** than `tmux`.
- His verdict: *"if you're building a tool from scratch and your users already have a shell tool available, just make a good CLI. It's simpler and more portable... Once you have a well-designed, token-efficient CLI tool, adding an MCP server on top of it is very straightforward."*

**Measurement boundary on the CLI row's "0":** that is the CLI's *own incremental schema* cost in Blocks.ai's measurement — it excludes any preloaded shell-tool definition or other harness-provided context, which Blocks.ai did not measure. Do **not** read "0" as the total initialization cost of using a CLI inside an agent.

**Also note the `SKILL.md` row: 12,755 tokens.** That figure is **one vendor's own skill file (n=1), measured by that vendor about its own artifact** — so it illustrates *a* failure mode for an oversized skill, not a typical or inherent skill size; a lean skill is far smaller. Treating it as representative would repeat the vendor-authored-evidence weakness this report warns about in Section 0.

**Where MCP genuinely wins (from the same sources, not vendor spin):**
- **No shell in the harness** — the deciding factor in Cloudflare's taxonomy.
- **Credential custody / governance — as an *implementation* property, not a protocol one.** A server-side facade can keep credentials out of model-visible arguments by holding them server-side; a CLI can achieve the same with a host-managed credential helper. The cited framing (a key the model "never holds") describes a *deployment* design, not something the protocol guarantees — and the CLI-plus-facade pattern below is itself a way to get it. (Blocks.ai names custody + **composability** as the two axes that matter; which side each lands on depends on how you build it.)
- **Stateful operations — a capability, not a default.** A server *can* maintain session state, but a server's statefulness depends on its implementation and deployment; nothing in the protocol makes it automatic, and a CLI can hold state too (files, a daemon, a scratch dir). Treat statefulness as a design choice on both sides.
- **Mario's own caveat:** CLI is *"too verbose"* in some domains; MCP is sensible when no good CLI exists.

**A useful reframe (my synthesis — the three sources make related but not identical points, so this is not a documented consensus):** the real axis is **eager vs lazy**, not **MCP vs CLI**. A bloated MCP and a sprawling `--help` tree are *expected to fail for the same reason* (too much context disclosed up front); a lean MCP and a CLI+skill are *expected to share the same low-overhead property*. Whether success is actually **equivalent** across MCP and CLI is **untested** — no common benchmark exists (see Section 5). **Design hypothesis (to test, not established):** a CLI may need agent-facing guidance for discovery and output conventions — the same progressive disclosure, auth consistency and token efficiency a server-side interface supplies via schemas — which a skill layer would then provide. Whether this holds, and what it costs in context, is exactly what the §5.1 workflow-cost measurement should settle. (The report's own hypothesis, not a cited finding.)

---

## 4. A better way to design docs into an agentic workflow

Combine the loads only the *design* sources prove out.

**(a) Why "just dump the docs" fails — measured.** Chroma's *Context Rot* (Hong, Troynikov, Huber, Jul 2025) evaluated **18 LLMs** (GPT-4.1, Claude 4, Gemini 2.5, Qwen3) holding task **complexity** constant while varying only input length: **performance degrades non-uniformly as input grows**; simple Needle-in-a-Haystack scores hide it, and **non-lexical** retrieval (NoLiMa-style) and **topically-related distractors** degrade far worse than irrelevant filler. Real docs are *made of* distractors. — [research.trychroma.com/context-rot](https://research.trychroma.com/context-rot)

**(b) Don't wrap endpoints — build few, workflow-shaped tools.** Anthropic's *Writing effective tools for agents*: agents aren't computers — *"if an LLM tool returns ALL contacts... it's wasting its limited context"*. Consolidate: `search_contacts` not `list_contacts`; `schedule_event` not `list_users`+`list_events`+`create_event`; **`get_customer_context` not `get_customer_by_id`+`list_transactions`+`list_notes`**. Namespace (`asana_search`, `jira_search`). Return **high-signal only** — drop `uuid`/`mime_type`, and an exposed **`response_format: concise|detailed` enum cut one example 206 → 72 tokens (~⅓)**. Paginate/filter/truncate; Claude Code caps tool responses at **25,000 tokens**. And evaluate with **held-out sets**, tracking calls, tokens, errors — they *found and fixed* a real bug (Claude appending "2025" to web-search queries) only through eval. — [Anthropic, "Writing effective tools for agents"](https://www.anthropic.com/engineering/writing-tools-for-agents)

**(c) The 2–3 tool shape that has evidence.** `search` (semantic + category overview so the model knows to search) → `describe`/`expand` (lazy, only when needed) → `execute`. Speakeasy reports **100% success** across its tested toolset sizes and task complexities — a **single-arm** result. Treat it as success *evidence*, not proof that success is *preserved* against another interface: it is Speakeasy's own 100% on Speakeasy's own tasks, the source does not state the static-toolset baseline's success rate, and no matched "one tool per page" baseline is given — so it **cannot be ranked** against Zechner's 100%/100% wash or any other study. On Code Mode: Speakeasy **argues** its own approach is cheaper to implement than Code Mode (which, they say, needs engineering investment most teams lack) while running ~50% slower — but this is **Speakeasy's own position, not a measured head-to-head**: no common benchmark of the two was published, so treat the implementation-cost comparison as an inference from a vendor with a stake in the answer.

**(d) Rank/rerank server-side; spend cheap-model budget to save expensive-model budget.** Context7's own −65% is the clean example, and it's the reason their free tier shrank — **you must budget for that cost yourself if you build it.**

**(e) For hard questions, clone-and-synthesize asynchronously.** Mario prefers repo-clone + sub-agent manual over Context7 snippets; Context7 does exactly this server-side with a `dynamic-context` cache + **expiry**. Version-pin, expire, regenerate.

**(f) Keep docs visible to the main model for interactive coding — an *untested heuristic*, not a gated recommendation.** For interactive coding, documentation visible to the model alongside the user's code *may* produce more grounded results; hiding retrieval behind a sub-agent may suit bulk/agentic runs. This is the report's **inference**, with no cited support and no direct link retrievable here, and the §5.1 protocol does **not** isolate it — so treat it as a hypothesis, not evidence. To test it, add a **retrieval-visibility condition** (docs inline vs hidden behind a sub-agent) as an extra factor in the pilot.

**(g) `llms.txt` is a supply-side format, not a retrieval fix.** Anthropic notes LLM-friendly docs are "commonly found in flat llms.txt files." But the 206-pt / 171-comment HN thread is almost entirely about *incentives* — who volunteers to make scraping easier (*"who would volunteer to make their sites easier to scrape?"* / Simon Willison: *"I have a bunch of open source projects that I want LLMs to know all of the details of"*), plus poisoning and consent economics. It solves *publishing*, not *"which snippet answers this question."* Good as a source; not a design. — [HN 41439983](https://news.ycombinator.com/item?id=41439983)

---

## 5. Normalized comparison, scoped to *your* scenario

**Scenario assumed:** you're building a Context7-like tool for dev docs, consumed by coding agents/harnesses that **(mostly) have a shell**, with cost and context as first-class concerns. **Scope note:** Sections 2–3 include API-scale (OpenAPI/1000s of endpoints) and terminal-control evidence, which is *background* for the transport choice, not evidence about interactive, version-aware documentation lookup. The table below separates the columns by quantity type precisely because entries were measured on different toolsets, sources and dates; read each cell with its scope, not across rows.

**Column key:** *Initialization footprint* = **component-level** initialization cost at first use — a measured token count where the source provides one (e.g. the CLI value is *CLI-schema only*, shell/harness overhead **excluded**), otherwise "not reported" with a qualitative note. Full workflow cost is a **separate** metric (§5.1), so do not compare this column against whole-workflow figures; *Tools* = number of tools/verbs exposed (note: Cloudflare's 2,500 figure counts **endpoints**, not tools — kept in its own cell so the columns stay same-unit); *Scope* = what was actually measured; *Success evidence* = whether the source reports task success; the *Estimated cost* ratings are **author estimates** from the listed requirements, not measured comparisons.

| Approach | Initialization footprint | Tools | Scope | Success evidence | Cost factors to *you* (build / run / UX — author estimate) |
|---|---|---|---|---|---|
| **GitHub MCP** (Blocks.ai measurement — *not* necessarily one tool per doc) | 14,243 (44 tools); 26,644 (85 tools) — *both deterministic*. Chawla's ~55,000 is an **earlier config that predates the slim-down** and is not comparable; 1.17M is **Cloudflare's hypothetical** for a 2,500-endpoint API, not this scenario | 44–85 (measured, Blocks.ai GitHub MCP default/all toolsets); CF's 2,500 is an **endpoint** count, not a tool count | Blocks.ai: schema tokens via `count_tokens`; CF: counterfactual | — | build effort & UX **unmeasured** (UX judgement: likely worst) |
| **CLI + skill** | CLI-specific schema: **0** (Blocks.ai measurement; excludes shell/harness context); skill size: **not reported generally** (one measured vendor skill: 12,755 tokens, n=1) | n/a (shell verbs) | Blocks.ai CLI row; skill figure is n=1 vendor artifact | **No docs-lookup success comparison exists.** Zechner's controlled 100% is *terminal-control* tasks | **low**; needs shell; agent safety heuristics |
| **Dynamic toolset (search/describe/execute)** | not reported (3 tools) | 3 | Speakeasy benchmark, 40→400 tools, Sonnet 4.5 | 100% reported for its own approach (baseline success not stated), −91–97% tokens; no matched "one tool per page" comparison | medium: maintain + eval a search fn |
| **Code Mode (client)** | not reported (typed SDK, pruned on demand) | not reported (typed SDK — an entry point, not a tool count) | Cloudflare/Anthropic 2025 write-ups | not success-benchmarked publicly | high: sandbox infra |
| **Code Mode (server-side)** | ~1,000 reported (2 tools) — the native-MCP comparator is the hypothetical part | 2 | CF comparator is hypothetical at **2,500 endpoints** | not success-benchmarked publicly | high; **zero agent-side change** |
| **Context7 today** | not reported (2 tools) | 2 | vendor-internal quality benchmark; 17% production-use signal (see §1) | vendor-internal quality scoring (not held-out) | $/call borne by them; free tier 1k/mo |

**Scenario-specific verdicts:**
- **Coding agents with a shell, docs are per-library, usage is interactive →** **CLI + a lean skill — one *candidate* (not yet a ranking) to be evaluated in §5.1, not a measured result. No cited result demonstrates CLI superiority for this scenario**; the choice stays unresolved until the matched pilot clears its gates. The headroom is real (CLI's *own schema* cost is ~0), but zero *init* tokens is not zero *workflow* cost: measure the **full context cost in your harness** — shell-tool overhead, skill text, `--help`/command-discovery output, and returned docs — before adopting. Two CLI taxes are real and *untested for this scenario*: (i) agent safety-heuristic re-scans (mitigate with host-specific allowlists/hooks), (ii) verbosity (commit to bounded output like `--lines N`). Neither mitigation has a measured cost in any cited source.
- **Harness has no shell →** **MCP**, but only in the **2–3 tool lazy shape**. **If instead you need credential custody/audit →** that is a **deployment** property, not a protocol one (§3): compare a **server-side** design that holds credentials server-side against a **CLI with host-managed credential helpers**, and choose by the required audit controls.
- **Source is API-scale/structured (OpenAPI, 100s–1000s of endpoints) →** **server-side Code Mode** (`search`/`execute`) or **Dynamic Toolset**. Never a tool per endpoint.
- **Small, stable doc set →** a tight **`SKILL.md`/`llms.txt` in-repo** may beat everything — but keep it lean; the 12,755-token Blocks skill is *an illustration* of the failure mode, not a typical size.
- **"Context7 isn't free" →** correct, and the reason is structural: they run an **LLM call per request** + reranking + background research. Building your own *moves that bill to you*. What you would be reproducing is the **corpus + reranker + flywheel**, not the protocol (hypothesis, per §1) — so budget for those, not for MCP plumbing.
- **MCP vs CLI:** the cited evidence supports **"CLI-first" as a hypothesis, not "the measured answer."** Zechner's controlled experiment reports 100% for both arms on *terminal-control* tasks with MCP marginally faster and cheaper — but the **per-arm denominator is unstated** (§3), so read it as **transport-overhead background, not a matched success-equivalence result**. His CLI preference is **his stated judgement** about simplicity and portability, plus his observation that adding an MCP server over a good CLI is straightforward. The **CLI-plus-thin-MCP-facade hybrid was not benchmarked by any cited source**; its governance benefit (credential custody behind the facade) is an architectural argument. Treat the hybrid as the design to *test*, and let the evaluation below decide.

### 5.1 Minimal interface sketch (hypothesis to test)

A concrete shape to benchmark — deliberately small in *interface* surface, though the evaluation itself is real work (see the pilot note at the end):

1. `resolve(library, version_hint)` → canonical library id + **pinned version** (return the requested version or flag a mismatch; never silently substitute).
2. `query(id, question, k)` → **k ranked excerpts**, each carrying `{source_url, library, version, last_updated}`.
3. **Output contract:** bounded (`k ≤ 5`, capped tokens/excerpt), source-linked, stable schema; `--format=json|text`.
4. **CLI:** `docs resolve`, `docs query`. **MCP facade (optional):** 2 tools mapping to the same functions, adding credential custody + audit.
5. **Comparison arms (minimum):** **CLI alone**, **CLI + thin MCP facade**, and **one lazy MCP / dynamic-toolset baseline** — each on the **same corpus, version behaviour, model, prompt and harness**. **Hold the retrieval implementation constant across arms** (one shared retrieval core, wrapped three ways), so the comparison isolates **interface/transport** rather than retrieval quality — otherwise a win is confounded with a better search function. **Keep the two comparisons separate and report them separately, because they answer different questions and must not be pooled:**
     - **(A) Transport-isolation test (causal):** the three shared-retrieval-core arms above. This is the only comparison that can attribute a difference to the *interface*.
     - **(B) Product comparison (non-causal, optional):** a live arm against the real incumbent or another shipping system (e.g. Context7's MCP mode, or **Context7's own CLI + Skills mode**, `ctx7`, no MCP). Their retrieval differs, so arm (B) cannot hold retrieval constant — report it as **real-world context only**, never pooled into arm (A)'s numbers and never used for a causal ranking. Include Context7's MCP mode or Code Mode only where a comparable implementation exists; report arms that are unavailable or non-comparable as such rather than scoring them. **Scope of the result:** unless a controllable Context7/Code-Mode incumbent is included, the experiment selects **among the prototype interfaces only** and does **not** establish superiority over Context7 or Code Mode.
6. **Eval gate — reproducible protocol** (this is the decision gate, not an afterthought):
   - **Question set:** ~150–200 real doc questions **total** (development + held-out), **stratified** by task type (API lookup, version-specific behaviour, breaking-change/migration, conceptual) and by library.
   - **Ground truth:** each question pins an **expected source and exact version** up front.
   - **Scoring:** *answer correctness* (pass criteria stated per question) and *version correctness* scored **separately**; define explicit handling for version mismatch, unsupported version, and silent fallback.
   - **Controls held fixed:** model, **task-prompt wording**, harness, and a frozen **source snapshot**. **Fair arm-specific guidance:** hold task wording identical but give each arm the **minimum production-ready integration material it needs** (MCP tool schemas/descriptions; a CLI skill / `--help` / command conventions), **frozen before held-out runs**. Count all of that metadata in the **workflow-context-cost** metric and report it with the results — so no arm is under-instructed and guidance quality is not confounded with interface choice.
   - **Reproducibility:** repeat runs where outputs vary (report variance); **manually adjudicate a sample** of the judge's decisions to bound judge error.
   - **Scoring rubric:** per-question rubric for *answer correctness* (full / partial / wrong, with "partial" defined up front) and *version correctness* (exact / mismatch / unsupported / **silent fallback**). **Aggregation is binary:** a question *counts as correct* only at **full**; **partial counts as not-correct** for the primary gate (a weighted partial-credit score is a **secondary** metric only). Ambiguous answers adjudicated by a **two-reviewer + tie-break** procedure.
   - **Metrics:** task success, version correctness, tokens, calls, latency, safety-review overhead — plus the **full workflow context cost** (shell-tool overhead, skill text, help/discovery output, returned docs), not just CLI init tokens.
   - **Held-out split & composition:** keep a **held-out set** fixed until configurations are frozen; tune only on the development split. With ~150–200 questions total, a ~70/30 split gives **≈45–60 held-out questions**. Fix the library set first (**3 libraries** assumed) and allocate explicitly — each cell is **(library × task type)**, minimum **2** per cell:

     | Library | API lookup | Version-specific | Migration | Conceptual | Row total |
     |---|---|---|---|---|---|
     | L1 | 2 | 2 | 2 | 2 | 8 |
     | L2 | 2 | 2 | 2 | 2 | 8 |
     | L3 | 2 | 2 | 2 | 2 | 8 |
     | **Column total** | 6 | 6 | 6 | 6 | **24** |

     The 45–60 held-out budget covers this 24-question floor with room to grow the sparsest cells. **Cells that cannot meet the minimum are dropped and reported inconclusive** — never folded into the headline rate without a note. Report per-cell and per-stratum results.
   - **Version-availability cases (required)** — every arm runs against the **same versioned corpus snapshot**; fix this policy *before* writing questions:

     | Requested pin | Expected behaviour | Scored as |
     |---|---|---|
     | Exactly available | Return that version's excerpt | Correct |
     | **Absent** (not in corpus) | **Refuse** and name the nearest available versions | Refusal = correct; silent substitution = **wrong** |
     | **Ambiguous / alias** (e.g. `latest`, `v2`) | Resolve per the declared rule and **state the resolved version**, else ask for clarification | Correct if resolution is stated and matches the rule |
     | **Breaking-change / migration** | Identify **source and target** versions and cite both | Correct only if **both** versions are right |

     Score **retrieval of the correct versioned source separately** from final-answer correctness, and never credit a silent fallback.
   - **Sizing caveat + stopping rule (do not skip):** whether ≈45–60 held-out questions can resolve a 5-point margin is an **open question, not a settled fact** — the required count depends on the **paired disagreement rate, confidence level and power**. So **pre-register the sizing procedure**: pilot first, estimate the discordance rate, declare a **confidence level (e.g. 95%) and power target (e.g. 80%)**, then **calculate** the held-out count for the 5-point margin. Set a **maximum feasible count (cap)** up front. If the calculated requirement exceeds the cap, the study **cannot resolve the pre-declared margin** — report *that* as the result; do **not** widen the margin to fit (the margin is a product requirement, not a capacity knob). A different margin is a **separate, newly justified study**.
   - **Incumbent baseline — the thing you would actually replace.** Default to **your current documentation workflow** (whatever the team uses today), run on the same held-out questions and episodes against a **matched corpus/version snapshot** where feasible. Use a **lean MCP baseline** only as a *separate interface baseline* for prototype selection. If the real incumbent cannot be matched, do **not** treat a prototype-only gate as evidence to replace it — report the adoption comparison as **inconclusive**, or limited to the dimensions that can be matched.
   - **Aggregation target (pre-register):** state whether strata are **equally weighted** or **weighted to an expected workload mix**, and use that one rule for both the gate and the interval. Report the weighted primary result **and** per-stratum results; if no defensible workload mix exists, label the aggregate as applying **only to the benchmark's chosen composition**.
   - **Interactive episodes — a *required* gate, not an aside.** A small set of **multi-turn coding episodes** (fixed repo + doc versions, concrete completion rubric): **count** ≈12–20 episodes, held-out like the questions (≥3 per task type); **scoring** per-episode pass/fail on **task completion**, plus per-turn latency and accumulated context cost; **aggregate/repeat/uncertainty** as for questions. **Adoption requires clearing an episode-level non-inferiority gate too**, with a pre-declared margin in **percentage points** (e.g. **≤5 points below the incumbent**, converted to a whole-episode count only at analysis time — a fixed "1 episode" margin would mean ~8% on a 12-episode set and ~5% on a 20-episode one, so do not fix it in episodes) — an interface is **not** adopted on lookup correctness alone if it fails to help agents finish tasks. **Episode sizing follows the same pre-registered procedure as the question set** (pilot to estimate paired episode discordance; declare confidence level, power target, margin and a feasible *episode* cap; then calculate the required episode count). If the required count exceeds the cap, declare the episode gate **inconclusive in advance** and restrict the adoption claim to lookup performance. If the episode set cannot reach the required count, mark episode results **exploratory** and **restrict the adoption claim to lookup performance** (say so explicitly).
   - **Uncertainty & repetition:** fix the **per-question aggregation rule in advance** (e.g. *majority of 3 runs*, or *any-run-success*), so each question yields **one binary outcome per arm**. Use a **paired per-question comparison** against the incumbent and report a **bootstrap (or exact) confidence interval** on the correctness difference — resampling **question-level pairs with each question's repeats kept together**. Repeats characterise run variance; they do **not** add independent questions. Treat uncertainty as a result. **Decision direction:** use a **one-sided lower confidence bound** at the pre-registered level (e.g. 95%); the arm clears the gate only if that **lower bound exceeds the negative margin** (the worst plausible deficit is within the margin). State the interval method (bootstrap vs exact) up front.
   - **Pre-registered decision rule (set _before_ running):** *correctness gates first* — require non-inferiority to the **named incumbent** by the declared margin (e.g. answer- and version-correctness each no more than 5 points below) on the held-out set; failures and silent version fallbacks count as **wrong**, not passes. Among arms that clear both gates, choose by **full workflow context cost**, with latency and call-count as tie-breakers. Report every arm against these thresholds, and report "inconclusive" when the gate is too uncertain to decide. **Tie-break cost metric (define it now):** the tie-breaker is **full workflow context tokens per completed task** (shell/skill/discovery/returned-doc tokens included), with **median per-turn latency** second. Among arms that are statistically indistinguishable on the gates, pick the lower full-workflow cost; if that is also a tie, pick the simpler implementation and say so. **Multiple candidates — decide this *before* data collection, and it must match your intent.** (i) If you intend to **choose among** several arms, give **every candidate the same multiplicity-adjusted level** (e.g. Bonferroni across the arms you are choosing between) and select the arm with the best **adjusted** lower bound — this both allows a winner and controls the false-pass rate. (ii) If instead you have already **committed to one** candidate, gate only that one at the unadjusted level and report the others as **descriptive**. Do **not** select the "strongest" arm from the results and gate it uncorrected. State which path you took.

### 5.2 Decision table — when each interface is justified

| Condition | Choose | Why |
|---|---|---|
| Shell present; docs per-library; interactive use | **Test** lean CLI + small skill as one of the **unranked candidates**; adopt only if it clears the §5.1 gates | No CLI-specific schema tokens (Blocks.ai); total cost still depends on shell, skill, discovery, and returned docs — see §5.1 |
| No shell available in the harness | MCP, 2–3 tool lazy shape | The harness cannot run a CLI at all |
| Credential custody / audit required | Server-side creds **or** CLI + host-managed credential helper | Custody is an **implementation** property, not a protocol one (§3) — choose by the required audit controls |
| API-scale/structured source (100s–1000s of endpoints) | **Test as candidates** (not yet chosen): Server-side Code Mode (reported ~1,000-token, 2-tool footprint) or Dynamic Toolset (init footprint not reported — inference); adoption still requires clearing the §5.1 gates | A fixed **initialization/tool-definition** footprint independent of API size is **reported** for Cloudflare's 2-tool design (its 1.17M comparator is **hypothetical**); total per-task tokens stay **output-dependent**; for Dynamic Toolset the fixed-cost claim is an **inference** |
| Small, stable doc set | In-repo lean `SKILL.md`/`llms.txt` | Plausible low-cost option for a small stable corpus; cost and adequacy **unmeasured** — check in §5.1 |
| Need version correctness + source traceability | Any — but interface must return version + source | Only testable via the held-out eval |
| Decide CLI-alone vs CLI + thin MCP facade | Tie-break by the measured eval (5.1 step 5) | No cited source measured the hybrid |

**What no source proves (be honest with yourself):** I found no cited source that publishes a single held-out benchmark running **CLI vs dynamic toolset vs Code Mode vs Context7 on the same doc-lookup tasks with success rates** (scoped to the sources reviewed here — I make no claim about the literature as a whole). Claude/Cloudflare/Speakeasy/Context7 each benchmark only themselves. That gap is exactly the eval **you** should build first. It is not free, but it is tractable: the §5.1 protocol (a ~150–200-question stratified set, a held-out split sized in a pilot, pinned versions, a fixed model/harness/snapshot, a named incumbent and non-inferiority gate, and full workflow context cost). **Pilot first** — a small run (≈20 questions, one library) to estimate question-curation and adjudication effort as well as statistical power before committing to the full set; treat the full protocol's cost as **to be estimated**, not assumed small. That's the only number that will settle this for *your* harness, and it is the gate on every recommendation in Section 5.
