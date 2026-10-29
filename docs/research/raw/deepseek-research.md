## Executive verdict

**Build the retrieval engine, not another MCP façade.** Context7’s defensible value is its documentation ingestion, version-aware index, reranking, and token-budgeted response shaping — not the MCP protocol itself. The MCP server is only a thin delivery layer over a private backend.

For your use case, the strongest design is:

> **A CLI-first, version-aware documentation retrieval core with a thin MCP adapter, optional Code Mode gateway for large OpenAPI/multi-backend surfaces, and a mandatory evaluation harness.**

Do **not** default to Code Mode for a 2–4-tool documentation server. Code Mode’s published gains come from hiding **hundreds** of tool schemas; a docs server has very few tools, so the main cost is the retrieved content, not the tool definitions.

And the “GitHub MCP vs `gh` CLI” analogy is real but **not generalisable to all MCP servers**. GitHub MCP is a 93-tool surface; a docs MCP is typically a 2–4-tool surface. The overhead scales with surface area, not with the protocol label.

---

## 1. What Context7 actually does internally

### Public surface: two tools, deliberately small

Context7 exposes:

- `resolve-library-id(libraryName, query)` — resolves a library name into a canonical Context7 ID such as `/vercel/next.js` or `/vercel/next.js/v16.2.9`.
- `query-docs(libraryId, query, language?)` — returns ranked documentation snippets and code examples for that library and question.

The CLI mirrors the same two-step model: `ctx7 library` then `ctx7 docs`, with `--json`, version-specific IDs, and language ranking. Context7 also supports a **CLI + Skills** mode, where the agent uses the CLI instead of MCP and no MCP server is registered. This is important: Context7 itself does not treat MCP as the only correct interface.

### Backend: private indexing and retrieval pipeline

The public repo explicitly says the API backend, parsing engine, and crawling engine are private. Published evidence describes:

- **Library index in Redis Search**: over 110,000 libraries stored as JSON documents, with title, description, source URLs, stars, trust score, snippet counts, versions, and update dates. Search weights title, description, and URL differently; verified libraries and popularity get boosts.
- **Vector retrieval** over documentation snippets, plus server-side reranking.
- **Versioned IDs** and version-specific documentation, e.g. `/org/project/version`.
- **Source-quality metadata** such as source reputation and benchmark score, which the model is instructed to consider when selecting a library.

The old architecture was a repeated loop: resolve ID → fetch docs → model inspects → fetch again. The new architecture accepts a query in both calls, does filtering and reranking server-side, and returns only the most relevant pieces. Context7 reports:

- average context tokens down from ~9.7k to ~3.3k (**−65%**),
- latency down from ~24s to ~15s (**−38%**),
- tool calls down from 3.95 to 2.96 (**−30%**).

These are **vendor self-reported** numbers, so treat them as directional. But the design lesson is solid: **move retrieval filtering out of the agent loop and into the server**.

### Pricing and “not completely free”

Context7’s Free plan includes **1,000 API calls/month** across MCP, CLI, Search API, and REST API, plus all public libraries. Pro is **$10/seat/month** with 2,000 calls/seat; additional calls are **$5 per 1,000**. Private repositories are billed separately at **$5 per 1M parsed tokens**. When Free is exhausted, requests stop until the next month, with 20 bonus calls/day.

Full on-premise deployment — including the parser, API server, MCP server, and private vector database — is an **Enterprise** offering, not a free self-hosted option.

So the free tier is genuinely usable for low-volume individual work, but it is not “free at scale,” and the ranking/indexing internals are not open to modification.

### What this means for your build

Cloning the MCP interface is easy. Cloning the **data product** is hard:

- crawling and parsing many documentation formats,
- version resolution,
- chunking that respects headings and code blocks,
- metadata and trust scoring,
- hybrid retrieval,
- reranking,
- token budgeting,
- evaluation at library level.

If you build only an MCP wrapper around web search, you will not beat Context7 on quality. If you build a focused, evaluation-driven index for **your stack**, you can beat it on relevance, version precision, privacy, and cost for that scope.

---

## 2. Code Mode: powerful, but not the right default for a docs server

### What Code Mode actually solves

Code Mode replaces a large tool catalog with a small set of meta-tools. The model searches for tools, inspects schemas, writes code that calls them in a sandbox, and returns only the final result. Intermediate results stay in the sandbox rather than flowing through the model context.

Cloudflare’s OpenAPI Code Mode server exposes only `search` and `execute`; the OpenAPI document stays outside the model context unless the search code returns part of it, and final responses are capped at roughly 6,000 estimated tokens.

Anthropic describes the same pattern as exposing MCP tools as a code API in a sandbox. In its example, tool-definition and intermediate-result overhead drops from **150,000 tokens to 2,000 tokens** — a 98.7% reduction.

### Where the gains actually come from

Bifrost’s benchmark is a useful scale test:

| Tool count | Input-token change with Code Mode | Cost change | Latency change |
|---:|---:|---:|---:|
| 96 tools | −58.2% | −55.7% | −26.8% |
| 251 tools | −84.5% | −83.4% | −6.8% |
| 508 tools | −92.8% | −92.2% | −17.1% |

Pass rate stayed at or near 100%. These numbers are strong evidence that Code Mode helps **large tool catalogs**.

But a documentation server with `resolve`, `search`, `fetch`, and maybe `get_symbol` has **four tools, not 96**. There is very little schema bloat to eliminate. The real context cost is the documentation payload returned by those tools — and Code Mode does not automatically make the payload smaller unless your generated code filters it before returning.

FastMCP’s own documentation is candid about the trade-off: **tokens vs. round-trips**. More discovery steps mean less wasted context but more latency and API calls; fewer steps mean more upfront detail and more wasted context. Cloudflare also labels Code Mode **experimental**.

### Verdict on Code Mode

| Scenario | Code Mode fit |
|---|---|
| 2–4 docs tools, normal library lookup | **Poor default.** Adds sandbox and discovery round-trips without meaningful schema savings. |
| Hundreds of library-specific operations | **Good.** Hides the catalog and lets code orchestrate. |
| Large OpenAPI spec with many endpoints | **Strong.** Spec stays outside context; code filters before returning. |
| Multi-server docs gateway across dozens of backends | **Reasonable.** Use as a gateway, not as the core docs server. |
| Simple exact-signature lookup | **Unnecessary.** A CLI or one MCP tool is faster and cheaper. |

**Recommendation:** build a plain CLI/MCP contract first. Add Code Mode only as an optional **gateway** for large OpenAPI surfaces or many documentation backends. Do not make it the primary abstraction.

---

## 3. CLI for dev docs: the near-zero resident-context option

The strongest argument for a CLI is not that MCP is always bad. It is that a CLI adds **almost no resident tool schema**. Simon Willison’s widely cited example: the GitHub MCP server defines 93 tools and consumes about **55,000 tokens** before any work begins, while `gh` CLI achieves the same functionality with near-zero schema cost because frontier models already know how to use it. GitHub’s own `cli-proxy` work reports GitHub MCP injecting roughly **22 tool definitions, ~10–15K tokens**, into every turn.

But this is not a universal law. A controlled comparison across seven agent scaffoldings and five models found that **scaffolding matters more than the interface**, with paired MCP-to-CLI cost ratios ranging from **0.43× to 29×**; two scaffoldings without MCP support completed all runs using only the CLI, and were 5×–28× cheaper than MCP-supporting scaffoldings. A separate 50-task study found that CLI helps less capable models on retrieval-heavy tasks, but is neutral or worse on lighter tasks, and that frontier models perform similarly across interfaces.

So the evidence supports a nuanced conclusion:

- **CLI is usually the best default in shell-capable coding harnesses.**
- **MCP is not inherently expensive** when the tool surface is small or the host defers tool loading.
- The correct question is not “CLI or MCP?” but “what surface area does this harness actually need?”

### What a good docs CLI looks like

Context7’s CLI is already a useful reference:

```bash
ctx7 library react "How to clean up useEffect with async operations"
ctx7 docs /facebook/react "How to clean up useEffect with async operations" --json
ctx7 docs /vercel/next.js/v16.2.9 "How to set up app router"
ctx7 docs /websites/twilio "How to send an SMS" --language Ruby
```

It returns code snippets and prose with headings, and supports JSON for scripting. It also installs a skill so the agent knows when and how to call the CLI, which is important: **CLI without a skill depends on the model already knowing the command**.

For your own tool, I would design the CLI around **retrieval primitives**, not around a single monolithic “ask” command:

```bash
docs resolve <package> --ecosystem npm|pypi|go|cargo --version <range> --json
docs search <package>@<version> "<query>" --language ts --max-tokens 4000 --json
docs symbol <package>@<version> <symbol> --json
docs page <doc-id> --section <heading> --max-tokens 6000 --json
docs open <url> --max-tokens 8000 --json
docs index <source> --name <package> --version <range>
docs eval <suite> --json
```

Contract:

- **stdout is JSON only** in `--json` mode; diagnostics go to stderr.
- Stable schema with `schema_version`.
- Exit `0` on hit, `2` on no result, `3` on upstream error, `4` on budget exceeded.
- Include `source_url`, `version`, `fetched_at`, `content_hash`, and `citation_id` for every result.
- Include `confidence` and `miss_reason` so the agent knows when to escalate.

This CLI is not a second-class citizen. It should be the **canonical retrieval core**. The MCP server should call the same core, not reimplement it.

---

## 4. A better way to feed official docs and argument details into an agentic workflow

The weakest part of most docs MCP servers is not the protocol. It is the **retrieval contract**: they return prose chunks when the agent needs an exact signature, version, parameter constraint, or deprecation notice.

I would design around **argument-aware documentation retrieval**.

### 4.1 Resolve versions from the project, not from the model’s memory

Do not ask the LLM to guess the library version. Read:

- `package.json` / lockfile for npm,
- `pyproject.toml` / lockfile for Python,
- `go.mod` for Go,
- `Cargo.toml` / `Cargo.lock` for Rust.

Then resolve the package identity from the registry, not just the human-readable name. Context7 already supports versioned IDs such as `/vercel/next.js/v16.2.9`, but the agent still has to choose the right version. Your tool can make that automatic.

### 4.2 Retrieve at three levels: index → section → symbol

A single “query docs” call encourages dumping. A better contract is progressive disclosure:

1. **Index level** — headings, symbols, and page metadata. Very cheap.
2. **Section level** — the relevant heading and its prose/code.
3. **Symbol level** — exact signature and argument details, extracted from source or structured docs.

This mirrors how Redis Docs MCP works: it chunks long pages by `##` heading, embeds each chunk, and exposes `search`, `fetch`, and `ask` rather than one giant retrieval call.

### 4.3 Make argument details a first-class result type

For “how do I call this function?” the agent does not need five pages of prose. It needs:

```json
{
  "kind": "symbol",
  "package": "next",
  "version": "16.2.9",
  "symbol": {
    "name": "useRouter",
    "kind": "function",
    "signature": "useRouter(): AppRouterInstance",
    "overloads": [],
    "params": [],
    "returns": {
      "type": "AppRouterInstance"
    },
    "deprecated": false,
    "since": "13.0.0",
    "source_url": "https://github.com/vercel/next.js/.../use-router.ts",
    "docs_url": "https://nextjs.org/docs/app/api-reference/functions/use-router"
  },
  "examples": [
    {
      "language": "tsx",
      "code": "const router = useRouter(); router.push('/dashboard');",
      "source_url": "..."
    }
  ],
  "confidence": 0.94,
  "tokens": 180
}
```

For a function with arguments:

```json
{
  "params": [
    {
      "name": "options",
      "type": "UseQueryOptions<T>",
      "required": false,
      "default": "{}",
      "constraints": "must include queryKey and queryFn",
      "description": "..."
    }
  ]
}
```

This is where AST/tree-sitter extraction is valuable. Tools like `agrep`, `livedocs`, and `semtree` already show that extracting signatures, types, and doc comments from source is dramatically cheaper than reading whole files — roughly 8×–50× context reduction in their reported examples.

A practical pipeline:

- **Structured docs first**: OpenAPI, JSON Schema, TypeDoc, Sphinx, MkDocs, Mintlify Markdown, `llms.txt`.
- **Source AST second**: tree-sitter for exported symbols, signatures, parameter types, defaults, deprecation tags, and doc comments.
- **Prose retrieval third**: hybrid search over headings and paragraphs for conceptual questions.

### 4.4 Hybrid retrieval, not vector-only

The evidence does not support “vector search always wins.”

A 116-question study across multiple agent harnesses found that **grep generally yielded higher accuracy than vector retrieval** in its first experiment, while overall scores depended strongly on harness and tool-calling style. Kapa’s benchmark found that a frontier model with only grep matched a tuned retrieval pipeline at 0.61, but took five times as long; an optimized agentic retriever scored 0.65 in about five seconds.

Redis Docs MCP’s production evaluation is a good example of hybrid retrieval working:

| Configuration | MRR | Hit@1 | Hit@3 | Recall@8 |
|---|---:|---:|---:|---:|
| Vector only | 0.816 | 0.75 | 0.88 | 0.93 |
| Hybrid, title field, RRF fusion | 0.836 | 0.71 | 0.96 | 0.99 |

Hit@3 improved by 0.08 and Recall@8 by 0.06, with retrieval latency staying at 0.5–0.7 ms p50.

So the right design is:

- **Exact/symbolic retrieval** for function names, config keys, error codes, CLI flags, and type names.
- **BM25/lexical retrieval** for technical terms.
- **Vector retrieval** for natural-language conceptual queries.
- **Reranking** over the fused candidate set.
- **Source authority and version match** as ranking features.

### 4.5 Citations must be generated from retrieval, not written by the model

Redis Docs MCP learned this the hard way: the model fabricated a documentation URL that was one path segment off. They stopped letting the model write citations and now return a `sources` array derived from what the retrieval pipeline actually fetched during the turn. Every citation ID comes from a real tool call and resolves through `fetch`.

Your contract should enforce the same:

- Every result has a stable `doc_id` and `source_url`.
- The model is instructed to cite `[doc_id]`, not to invent URLs.
- The server validates that every returned `doc_id` exists in the current index.
- `fetch(id)` resolves only IDs that were actually returned by `search`.

This is more reliable than asking the model to “cite sources.”

### 4.6 Budget the response, and tell the agent when you truncated

Microsoft Learn’s MCP server exposes a `maxTokenBudget` query parameter to cap search-response size; it truncates content to fit the budget. Cloudflare’s Code Mode server caps final responses at roughly 6,000 estimated tokens and marks truncated responses explicitly.

Your tool should do the same:

- `max_tokens` parameter with a sane default, e.g. 2,000–4,000.
- `detail=brief|standard|full`.
- `truncated: true` and `next_cursor` when content is cut.
- Never silently drop the most relevant snippet.

### 4.7 Fallback when retrieval fails

A good agentic workflow needs an explicit failure path. Context7’s own tools tell the model to stop after three calls and use the best result if retrieval is insufficient. That is a useful guardrail.

Your contract can do better with a **confidence and fallback** field:

1. Local exact symbol index.
2. Local hybrid docs index.
3. Official documentation page fetch.
4. Official GitHub source search.
5. General web search, clearly marked as lower authority.

The agent should escalate only when `confidence` is low or `miss_reason` says the local index does not cover the package/version. This avoids both endless retrieval loops and premature hallucination.

### 4.8 Treat documentation as untrusted data

Context7’s enterprise offering runs LLM-based classifiers to detect prompt injection and malicious snippets before content reaches the developer, and applies source policies and trust filters. The public README also disclaims accuracy, completeness, and security because libraries are community-contributed.

If you build this, treat retrieved docs as **untrusted input**:

- Strip or neutralise instruction-like text.
- Keep documentation content separate from system/tool instructions.
- Prefer official sources and signed/versioned snapshots.
- Log provenance, not user queries, if privacy matters.
- Scan for prompt-injection patterns before returning content.

---

## 5. Recommended architecture

```text
                    ┌──────────────────────────────┐
                    │        docs-core             │
                    │  resolve → retrieve → rank   │
                    │  → shape → cite → budget     │
                    └────────────┬─────────────────┘
                                 │
        ┌────────────────────────┼────────────────────────┐
        │                        │                        │
   ┌────▼─────┐           ┌──────▼──────┐          ┌──────▼──────┐
   │   CLI    │           │  MCP server │          │ Code Mode   │
   │ + skill  │           │ 2–4 tools   │          │ gateway     │
   └──────────┘           └─────────────┘          └─────────────┘
                                 │
                    ┌────────────▼─────────────┐
                    │ Index: SQLite FTS5 +      │
                    │ optional vector + rerank  │
                    │ + metadata + versions     │
                    └──────────────────────────┘
```

### Core components

1. **Ingestion**
   - Official docs Markdown / MDX / `llms.txt` / `llms-full.txt`.
   - GitHub repo docs and source.
   - npm/PyPI/Go/Cargo metadata.
   - OpenAPI/JSON Schema where available.
   - Version snapshots keyed by tag/commit.

2. **Parsing and extraction**
   - Heading-aware chunking.
   - Code-block preservation.
   - Tree-sitter symbol extraction.
   - Front-matter/metadata extraction.
   - Source URL and line mapping.

3. **Index**
   - SQLite FTS5 for exact/BM25.
   - Optional local vector index (`sqlite-vec`, LanceDB, or pgvector).
   - Metadata tables for package, version, source, trust, freshness, deprecation.

4. **Retrieval and ranking**
   - Hybrid candidate generation.
   - Reranker over top candidates.
   - Version and source-authority boosts.
   - Token-budgeted response builder.

5. **Interfaces**
   - CLI + skill: default for shell-capable coding agents.
   - MCP: 2–4 tools, thin adapter over core.
   - Code Mode: optional gateway for large OpenAPI or many backends.
   - REST/API: for custom integrations.

### Why this is better than a Context7 clone

- You own the ranking and can tune it for your stack.
- You can index private/internal docs without enterprise pricing.
- You can use local embeddings and avoid per-call vendor cost.
- You can expose the same core through CLI and MCP without duplication.
- You can evaluate retrieval quality directly instead of trusting a black-box ranking.

### Why it is also more work

- You own crawling, parsing, versioning, and freshness.
- You own retrieval quality and evaluation.
- You own security scanning for untrusted docs.
- You own index hosting and updates.
- Long-tail library coverage will be worse than Context7 unless you ingest broadly.

A realistic hybrid: **use Context7 free/paid as a long-tail fallback, and your local index for your core stack and private docs.** This gives you quality where it matters and coverage where you lack it.

---

## 6. MCP vs CLI decision matrix

| Harness / requirement | Best default | Why |
|---|---|---|
| Claude Code, Codex CLI, Gemini CLI, terminal agents | **CLI + skill** | Near-zero resident schema; models already know CLI patterns; easy to inspect. |
| GUI client, remote client, no shell | **MCP** | Structured tool calls; no shell dependency. |
| Many tools across many servers | **MCP + progressive discovery / Code Mode** | Hides tool schemas; reduces context. |
| 2–4 docs tools | **Plain MCP is fine** | Schema overhead is small; optimise the payload instead. |
| Typed permissions, audit, multi-tenant governance | **MCP** | Structured access control and credential isolation are MCP strengths. |
| Large OpenAPI spec | **Code Mode over OpenAPI** | Spec stays outside context; code filters/paginates before returning. |
| Both terminal and GUI users | **CLI + MCP from one core** | Share retrieval logic; choose surface per harness. |

The evidence does not support a universal “CLI is better” or “MCP is better” claim. The controlled studies show that **scaffolding, tool surface, and task type matter more than the protocol label**.

---

## 7. Build plan with evaluation gates

### Phase 0 — Evaluation harness first

Before writing the server, create a golden set:

- 20–50 real questions per core library.
- Include API-signature questions, argument-detail questions, migration questions, and “does not exist” questions.
- Label the correct source chunks and the correct answer.
- Measure retrieval separately from agent answers.

Redis Docs MCP’s evaluation design is a good model: a **pure retrieval harness** (deterministic, cheap) and an **agentic harness** (stochastic, captures tool calls), both against the same labelled dataset.

Context7’s own ZK experiment is a warning: adding Context7 reduced answer quality from **73.8% to 59%** on 10 framework questions, with 8 of 10 answers worse. Retrieval can hurt if it returns the wrong material. You must measure end-to-end answer correctness, not just “did it retrieve something.”

### Phase 1 — Ingestion and exact index

- Ingest official Markdown/GitHub docs for 5–10 libraries you actually use.
- Build SQLite FTS5 index with heading-aware chunks.
- Add tree-sitter symbol extraction for signatures.
- Implement `docs symbol` and `docs search`.
- Gate: exact-symbol retrieval recall ≥ 95% on your golden set.

### Phase 2 — CLI and JSON contract

- Implement `resolve`, `search`, `symbol`, `fetch`, `open`.
- Add `--json`, `--max-tokens`, `--language`, version resolution.
- Add citations and `confidence`.
- Gate: agent can complete docs tasks using only CLI; no MCP yet.

### Phase 3 — MCP adapter

- Expose 3–4 tools max:
  - `resolve_package`
  - `search_docs`
  - `get_symbol`
  - `fetch_doc`
- Thin wrapper over the same core.
- Add server instructions that tell the model when to use each tool and when to stop.
- Gate: MCP and CLI produce identical retrieval results for the same query.

### Phase 4 — Hybrid retrieval and reranking

- Add vector retrieval and rank fusion.
- Evaluate against vector-only and FTS-only baselines.
- Gate: hybrid improves Hit@3/recall without unacceptable latency.

### Phase 5 — Optional Code Mode gateway

- Only if you need large OpenAPI or many backends.
- Use Cloudflare-style `search` + `execute` or FastMCP CodeMode.
- Gate: Code Mode reduces tokens/latency versus plain MCP at your actual tool count, not at 500 tools.

---

## 8. Final recommendation

**If you are building for yourself or a small team:** build the CLI-first local index. It is free, private, version-aware, and near-zero resident context. Add MCP only if a GUI client needs it.

**If you are building a product for other agents:** build the core once, then expose CLI, MCP, and API. Do not make Code Mode the default. Do not clone Context7’s generic “query docs” interface; differentiate on **exact argument details, version precision, citations, and evaluation**.

**If you are choosing between Context7 and building:** use Context7 for long-tail coverage and quick validation; build your own for your core stack, private docs, and exact API argument retrieval. The hybrid is cheaper and better than either extreme.

The strongest published evidence says the interface is not the moat. **The retrieval contract, index quality, and evaluation loop are.** Build those first; the MCP or CLI wrapper is the easy part.
