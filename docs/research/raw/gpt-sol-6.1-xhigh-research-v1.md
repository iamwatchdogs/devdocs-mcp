# Context7-Style Developer Documentation for Agentic Workflows

## Executive conclusion

The strongest design is **a local-first documentation retrieval core with a CLI as the default interface, a small/lazy MCP adapter for hosts without shell access, and code mode for multi-step documentation workflows**.

Do not begin with a large always-loaded MCP server. The evidence indicates:

1. Retrieval quality comes from indexing, versioning, ranking, provenance, and freshness—not from MCP itself.
2. CLI interfaces avoid eager tool-schema costs and naturally support progressive disclosure.
3. MCP is valuable for remote authorization, structured access, and multi-user governance.
4. Code mode is valuable for loops, comparisons, batch retrieval, filtering, and citation verification.
5. Documentation retrieval should return compact signatures and examples first; full pages should be loaded only when necessary.

This report evaluates Context7’s public architecture, MCP versus CLI, code-mode MCP, developer-documentation ingestion, and agentic workflow design. Vendor benchmarks are separated from academic and community evidence.

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
- a maximum of three documentation calls per question
- explicit single-concept query guidance

The MCP server is therefore primarily a transport and policy adapter, not the indexing system itself.

References:

- [Context7 repository](https://github.com/upstash/context7)
- [MCP server source](https://raw.githubusercontent.com/upstash/context7/master/packages/mcp/src/index.ts)
- [MCP API proxy source](https://raw.githubusercontent.com/upstash/context7/master/packages/mcp/src/lib/api.ts)
- [Context7 MCP rules](https://raw.githubusercontent.com/upstash/context7/master/rules/context7-mcp.md)

### 2.2 Catalog and retrieval behavior

Upstash’s engineering article says the Context7 library catalog uses Redis Search with:

- exact and stemmed fields
- weighted title and description matching
- popularity and verification signals
- source-type filters
- trust scores
- version metadata
- more than 110,000 indexed libraries at publication time

The article documents the library catalog, not every detail of the document retrieval index. Context7’s API documentation separately confirms that document retrieval supports vector search, LLM reranking, and a fast mode that skips reranking.

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

Context7 indexes Markdown, MDX, reStructuredText, text, notebooks, OpenAPI specifications, websites, and `llms.txt` sources. It generally prefers documentation files over raw source code and falls back to source-derived documentation when little documentation exists.

`context7.json` supports:

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

It also supports manual refresh and GitHub-triggered refreshes. A local implementation should support both scheduled refreshes and event-driven refreshes.

### 2.5 Current two-call design versus one-call search

The traditional MCP flow is:

```text
resolve library → query documentation
```

Context7’s newer REST search endpoint can perform a unified lookup in one request. It can select up to four libraries, retrieve documentation in parallel, deduplicate, globally rerank, and report version status.

For a new tool, the one-request path should be the normal fast path. Explicit resolution should remain available for ambiguous libraries or workflows that need the user to choose a source.

---

## 3. Context7 pricing and cost model

Context7’s published plans state:

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

---

## 4. Normalized benchmark evidence

These studies measure different scenarios. The numbers below preserve each study’s baseline and task definition rather than pretending they are one common leaderboard.

| Study | Scenario | Within-study result | Evidence |
|---|---|---:|---|
| Context7 versus web search | 100 documentation questions using the same Claude Agent SDK | Context7 reported 36.81% fewer total tokens, 34.56% lower cost, 98.98% fewer fresh input tokens, and 22.77% fewer tool calls | Vendor benchmark |
| Context7 old versus new architecture | 80+ coding questions across evolving, niche, multi-library, popular, and unspecified libraries | Approximately 9.7k → 3.3k context tokens, 24s → 15s latency, and 3.95 → 2.96 tool calls | Vendor benchmark |
| GitHub CLI versus MCP | 75 matched runs using the same Claude model, prompts, and five read-only tasks | MCP used 4–32× more tokens; CLI succeeded in 25/25 runs, MCP in 18/25 due to connection timeouts | Vendor benchmark with open harness |
| Stripe CLI versus raw MCP versus code mode | 12 Stripe tasks; all arms passed all tasks | CLI: 711,555 tokens; raw MCP: 506,970; code mode: 294,924 | Vendor benchmark |
| Documentation URL discovery | 2,400 attempts across 20 documentation sites, two agents, four serving formats | Adding an `llms.txt` link reduced input tokens about 41–60% in the reported arms and reduced 404s close to zero | Reproducible vendor benchmark |
| API documentation RAG | 1,017 APIs across four less-common Python libraries | BM25 top-5 documentation raised average pass rates from roughly 18–41% to 57–76%; ideal target documents reached roughly 74–91% | Academic preprint |
| CloudAPIBench | 622 AWS/Azure API invocation tasks | GPT-4o low-frequency API validity rose 38.58% → 47.94% with documentation; naïve retrieval harmed common APIs by 39.02 percentage points | Academic research |
| MCP tool descriptions | 856 tools across 103 MCP servers | 97.1% contained at least one description smell; better descriptions improved success +5.85pp but increased steps +67.46% | ACM research |

### 4.1 Context7 versus web search

Context7’s five-category benchmark used 20 questions per category and the Claude Agent SDK. The reported averages were:

| Metric | Web search | Context7 | Reported change |
|---|---:|---:|---:|
| Total tokens | 136,562 | 84,278 | −36.81% |
| Fresh input tokens | 38,403 | 387 | −98.98% |
| Output tokens | 3,290 | 1,544 | −52.97% |
| Tool calls | 3.47 | 2.55 | −22.77% |
| Cost | $0.22 | $0.14 | −34.56% |

The most important result is not that Context7 “uses fewer tokens” in the abstract. It is that page-level web retrieval produced much more fresh context, while Context7 returned focused documentation slices.

Reference: [Context7 versus Claude Code web search benchmark](https://upstash.com/blog/context7-vs-web-search-benchmark)

### 4.2 GitHub CLI versus MCP

Scalekit compared a shell-based CLI, CLI plus an approximately 800-token skills file, and GitHub’s official MCP server using 43 tool schemas.

Reported median tokens per task:

| Task | CLI | MCP | MCP multiplier |
|---|---:|---:|---:|
| Repository language and license | 1,365 | 44,026 | 32× |
| PR details and review status | 1,648 | 32,279 | 20× |
| Repository metadata and install | 9,386 | 82,835 | 9× |
| Merged PRs by contributor | 5,010 | 33,712 | 7× |
| Latest release and dependencies | 8,750 | 37,402 | 4× |

The benchmark also reported 7 connection timeouts in 25 MCP runs. This reliability result is important, but it is not purely a protocol result: it includes a remote server and network path.

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

Mintlify’s benchmark used 2,400 valid attempts across 20 documentation sites, two agents, four serving arms, and three repetitions per cell.

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

The index link reduced navigation errors while maintaining approximately 94–99% accuracy. Inlining the entire index did not justify its additional token cost.

Reference: [Mintlify Docs URL Discovery Benchmark results](https://github.com/mintlify/docs-url-discovery-bench/blob/main/RESULTS.md)

---

## 5. Academic findings relevant to the design

### 5.1 API documentation materially improves unfamiliar-library coding

The paper [When LLMs Meet API Documentation](https://arxiv.org/html/2503.15231) studied Polars, Ibis, GeoPandas, and Ivy, with 1,017 eligible APIs.

The authors found:

- retrieval improved less-common API usage by approximately 83–220%
- code examples contributed more than descriptive prose or parameter lists
- BM25 was effective for the task
- ideal target-document retrieval substantially outperformed noisy top-5 retrieval
- documentation quality and example diversity mattered

This strongly supports a documentation index that treats examples as first-class retrieval objects.

### 5.2 Compact API specifications can outperform full pages

CloudAPIBench measured API hallucinations across 622 synthetic AWS/Azure tasks.

For GPT-4o:

- low-frequency API validity was only 38.58% without documentation
- documentation increased it to 47.94%
- naïve documentation could damage high-frequency API performance
- selective retrieval improved the overall score by 8.20 percentage points

The authors also compared context formats:

| Context | Average tokens | Valid API invocation |
|---|---:|---:|
| API name only | 36 | 52.73% |
| API description | 79 | 53.22% |
| API specification | 53 | 86.82% |
| Description + specification | 95 | 87.14% |
| Full documentation | 685 | 88.75% |

References:

- [CloudAPIBench paper](https://arxiv.org/html/2407.09726)
- [ICSE-SEIP DOI](https://doi.org/10.1109/ICSE-SEIP66354.2025.00027)

### 5.3 Code retrieval remains difficult

[CodeRAG-Bench](https://aclanthology.org/2025.findings-naacl.176/) evaluated retrieval-augmented code generation across eight task types, five document sources, ten retrievers, and ten language models.

Its central finding is that high-quality retrieved context helps, but:

- retrievers often fail to find useful documents
- generators do not always use retrieved context effectively
- retrieval quality and generation quality must be evaluated separately

### 5.4 Evaluate the retrieval process, not only the final patch

[ContextBench](https://euniai.github.io/ContextBench/) contains 1,136 issue-resolution tasks from 66 repositories and eight programming languages. It measures:

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
- execution steps increased by 67.46%
- 16.67% of cases regressed

The implication is not “write longer tool descriptions.” It is “write compact, discriminative descriptions and measure behavior.”

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

Start with one unified operation:

```text
docs.search(
  query,
  source?,
  version?,
  language?,
  limit?,
  max_tokens?,
  detail?
)
```

Return structured evidence:

```json
{
  "results": [
    {
      "ref": "docs://next.js/v16/proxy#rewrite",
      "title": "Proxy rewrite",
      "section": "Routing > Proxy",
      "snippet": "...",
      "examples": ["..."],
      "source_url": "https://...",
      "source_commit": "abc123",
      "requested_version": "16",
      "served_version": "16.2.9",
      "version_status": "verified",
      "content_hash": "...",
      "token_count": 142
    }
  ]
}
```

Never silently serve latest documentation when a version was requested. Return explicit fallback metadata.

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

Context7’s own `ctx7` CLI demonstrates this general pattern with `library`, `docs`, `--json`, setup, project-level configuration, and skills integration.

Reference: [Context7 CLI](https://raw.githubusercontent.com/upstash/context7/master/packages/cli/README.md)

### 7.3 MCP adapter

Expose only a small set of tools:

```text
search_docs
open_doc
verify_doc
```

Avoid exposing many overlapping operations such as separate library lookup, page search, section fetch, example fetch, signature fetch, and comparison tools unless evaluations show that the separation improves behavior.

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
  hits.slice(0, 3).map(hit => docs.open({ ref: hit.ref }))
)

console.log(docs.compact(pages, { maxTokens: 1200 }))
```

The sandbox should handle parallel retrieval, filtering, deduplication, version comparisons, and citation assembly. The main model should receive only the compact final result.

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
- [Mintlify benchmark](https://github.com/mintlify/docs-url-discovery-bench/blob/main/RESULTS.md)

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

Context7 documents layered malicious-content detection, source filtering, and policies. Community discussion also highlights that `llms.txt` and documentation can eventually become prompt-injection targets.

References:

- [Context7 data safety](https://context7.com/docs/security/data-safety.md)
- [Context7 data privacy](https://context7.com/docs/security/data-privacy.md)
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
- same task order
- same authentication conditions
- same timeout
- same output budget
- separate index-build cost from query cost
- report cached and uncached costs separately
- use at least 30 tasks per category
- repeat stochastic agents several times
- use paired confidence intervals

Do not combine token counts from unrelated studies. Compare within the same model, agent, task corpus, and accounting method.

Useful evaluation references:

- [ContextBench](https://euniai.github.io/ContextBench/)
- [CodeRAG-Bench](https://aclanthology.org/2025.findings-naacl.176/)
- [AgentDocsBench](https://pypi.org/project/agentdocsbench/)
- [Zvec agent benchmarks](https://zvec.org/en/docs/zvec-grep/benchmarks/)

---

## 12. Final recommendation

Build in phases:

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

The best Context7 alternative is not “another MCP server.” It is a high-quality, version-aware documentation index with CLI-first progressive disclosure and optional protocol adapters.

---

# References

## Context7 and documentation infrastructure

1. [Context7 GitHub repository](https://github.com/upstash/context7)
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
12. [Context7 CLI](https://raw.githubusercontent.com/upstash/context7/master/packages/cli/README.md)
13. [Context7 MCP rules](https://raw.githubusercontent.com/upstash/context7/master/rules/context7-mcp.md)
14. [How Redis Search powers Context7](https://upstash.com/blog/how-redis-search-powers-context7)
15. [Context7 without context bloat](https://upstash.com/blog/new-context7)
16. [Context7 versus Claude Code web search benchmark](https://upstash.com/blog/context7-vs-web-search-benchmark)

## MCP, CLI, and code mode

17. [MCP tools specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools)
18. [GitHub MCP Server](https://github.com/github/github-mcp-server)
19. [GitHub CLI manual](https://cli.github.com/manual/)
20. [Anthropic: Code execution with MCP](https://www.anthropic.com/research/code-execution-with-mcp)
21. [Anthropic: Advanced tool use](https://www.anthropic.com/research/advanced-tool-use)
22. [Anthropic: Effective context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
23. [Anthropic: Building effective agents](https://www.anthropic.com/research/building-effective-agents)
24. [Anthropic: Multi-agent research system](https://www.anthropic.com/engineering/multi-agent-research-system)
25. [Cloudflare: Code Mode](https://blog.cloudflare.com/code-mode/)
26. [Scalekit: MCP is up to 32× more expensive than CLI](https://www.scalekit.com/blog/mcp-vs-cli-use)
27. [Port of Context: CLI versus MCP versus Code Mode](https://www.portofcontext.com/blog/cli-vs-mcp-vs-code-mode)
28. [CircleCI: MCP versus CLI](https://circleci.com/blog/mcp-vs-cli/)
29. [Tyk: MCP versus CLI enterprise comparison](https://tyk.io/learning-center/mcp-vs-cli-for-ai-agents-enterprise-comparison-guide/)
30. [Apideck: CLI alternative to MCP context overhead](https://www.apideck.com/blog/mcp-server-eating-context-window-cli-alternative)
31. [mcp2cli](https://github.com/knowsuchagency/mcp2cli)

## Academic research

32. [CodeRAG-Bench: Can Retrieval Augment Code Generation?](https://aclanthology.org/2025.findings-naacl.176/)
33. [When LLMs Meet API Documentation](https://arxiv.org/html/2503.15231)
34. [On Mitigating Code LLM Hallucinations with API Documentation](https://arxiv.org/html/2407.09726)
35. [CloudAPIBench ICSE-SEIP DOI](https://doi.org/10.1109/ICSE-SEIP66354.2025.00027)
36. [ContextBench](https://euniai.github.io/ContextBench/)
37. [MCP Tool Descriptions Are Smelly](https://doi.org/10.1145/3858625)
38. [AgentDocsBench](https://pypi.org/project/agentdocsbench/)

## Documentation formats and benchmarks

39. [The `llms.txt` proposal](https://llmstxt.org/)
40. [Mintlify Docs URL Discovery Benchmark](https://github.com/mintlify/docs-url-discovery-bench)
41. [Mintlify benchmark results](https://github.com/mintlify/docs-url-discovery-bench/blob/main/RESULTS.md)
42. [Firecrawl Developer Index and DevDex](https://www.firecrawl.dev/blog/developer-index-launch.md)
43. [Zvec agent benchmarks](https://zvec.org/en/docs/zvec-grep/benchmarks/)

## Community evidence

44. [Hacker News: Context7 usage discussion](https://news.ycombinator.com/item?id=46490087)
45. [Hacker News: Bad MCP design costs more tokens](https://news.ycombinator.com/item?id=48407391)
46. [Hacker News: `llms.txt` discussion](https://news.ycombinator.com/item?id=48410589)
47. [MMNTM: The MCP Tax](https://www.mmntm.net/articles/mcp-context-tax)
