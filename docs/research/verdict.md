---
type: decision record
title: Verdict — What to Build and How Confident We Are
description: The converged build decision from all eight research runs, the claims ledger with confidence tiers, and the unresolved conflicts. Read this first; topic files carry the evidence.
tags: [devdocs, decision, confidence, claims-ledger]
timestamp: 2026-10-27T19:00:00Z
resource: docs/research
sources: [DS, DSV, GLMC, GLMP, GPT1, GPT2, KIMI, GEM]
---

# Verdict

## The decision

All eight runs, independently conducted, converge on the same architecture. State it once, with its confidence:

> **Build a protocol-agnostic, local-first documentation retrieval core — version-pinned corpus, hybrid symbol-aware retrieval, provenance on every span — exposed as a CLI first for shell-capable agents, with a 2–4-tool MCP adapter for shell-less clients. Treat Code Mode as a measured, optional third adapter. Do not clone Context7's hosted service. The differentiation to earn is retrieval quality, version correctness, and provenance — not the interface or protocol.**

Every clause maps to a claims row below. The interface is "the easy part" (GLMC, DSV); **the corpus is the product** (GLMC Pattern 2, DSV, GLMP).

What this verdict is not: it is a **provisional implementation hypothesis**, not a post-benchmark conclusion (GPT2 §12 is explicit). No gate in [benchmark-protocol.md](benchmark-protocol.md) has been run. The evidence that exists comes from adjacent task families (GitHub operations, code generation, synthetic QA) — no published controlled benchmark compares interfaces on doc-lookup tasks with success rates (GLMC Finding 8, DSV §5, GPT2 §4.5). The protocol doc exists to close that gap.

## Claims ledger

Eight load-bearing claims, normalized from GLMC's audited ledger (C1–C8) and cross-checked against all other runs. Confidence: **High** = 3+ independent clusters, consistent; **Medium** = 2 clusters or single audited source with caveats. GEM (added 2026-10-27 02:32, after this ledger was frozen) independently corroborates claims 1–5 and 7 but was not re-audited into the confidence tiers.

| # | Claim | Confidence | Evidence | Topic file |
|---|---|---|---|---|
| 1 | Context7's repo is a thin client over a closed hosted index; "self-hosting" the server does not self-host the data; free tier 1,000 calls/mo | High | GLMC (pinned commit d956160), DS, DSV, GLMP, GPT1, GPT2, KIMI — unanimous | [context7-internals.md](context7-internals.md) |
| 2 | Token bloat is a **loading policy** problem, not a protocol property; MCP does not mandate full-catalog exposure | High | GLMC (spec + GitHub toolsets + Speakeasy), DS, DSV (eager-vs-lazy reframe, §3), GPT2 | [interfaces.md](interfaces.md) |
| 3 | Large static tool catalogs cost 4.3–32× more tokens than CLI on identical GitHub tasks (Scalekit, audited: mechanism solid, statistics flawed) | High (mechanism) / Medium (generalization) | GLMC (raw-data recomputation), GLMP, GPT1, GPT2, DS, DSV §3 (relayed); KIMI corroborates the mechanism only (schema tax, not the Scalekit benchmark) | [interfaces.md](interfaces.md) |
| 4 | Code-as-action helps large/compositional action spaces; **no evidence it helps a 2–4-tool docs domain**; one documented negative result (τ²-bench) | Medium | GLMC (CodeAct ICML, Anthropic product docs incl. negative result), DS, DSV, GPT1, GPT2, KIMI | [interfaces.md](interfaces.md) |
| 5 | Retrieval quality and version correctness — not interface choice — dominate docs-domain outcomes; small cited chunks beat context stuffing | High | GLMC (DocPrompting, Self-Route, degradation studies), DS, DSV, GPT1, GPT2, GLMP, KIMI | [retrieval-design.md](retrieval-design.md) |
| 6 | Hosted crawl-time indexes exhibit structural staleness; local version-pinned corpora remove the crawl-freshness failure mode for pinned versions | Medium | GLMC (issue tracker + 2011 JDK study; 2 clusters), GLMP (freshness table, long-tail complaints); DSV §1(b) corroborates the expiry mechanism only | [context7-internals.md](context7-internals.md) |
| 7 | Documentation content is an untrusted input channel (prompt injection, corpus poisoning) — design for it from day one | High | GLMC (AgentDojo, PoisonedRAG, MCP spec), GPT2 §10 (threat matrix), GPT1, KIMI §1 (ContextCrush), GEM §5 (OpenClaw) | [security-licensing.md](security-licensing.md) |
| 8 | No controlled dev-docs interface benchmark exists; a matched experiment is the missing evidence and the adoption gate | High (as of 2026-10-27) | Explicit in GLMC Finding 8, GPT2 §4.5–4.6, DSV §3 ("no common benchmark exists"); consistent with KIMI/DS/GPT1, which prescribe evaluation-first designs without asserting nonexistence | [benchmark-protocol.md](benchmark-protocol.md) |

## Where the sources disagree

Recorded disagreements, with the resolution this bundle uses. Each is a trap an agent could fall into by quoting a single source.

| Disagreement | Sources | Resolution |
|---|---|---|
| Is CLI-vs-MCP decided? | GLMP/GLMP-derived: "CLI-first" reads as near-settled. GLMC/KIMI/GPT2: a hypothesis; the one controlled benchmark has flaws; no docs-domain benchmark exists | **Hypothesis, gated.** CLI is the default *candidate* for shell-capable harnesses; adoption claim limited to lookup performance until [benchmark-protocol.md](benchmark-protocol.md) gates pass |
| Zechner's MCP-vs-CLI result | DSV: "wash — MCP 23% faster, 2.5% cheaper". GLMP: "MCP most expensive of four" ($0.4804 vs $0.3865). GEM §4: $19.45 vs $19.95 across 30 runs, 51 vs 66 min | Both describe the same experiment with different aggregates; denominators are stated inconsistently across sources (DSV flags them unverified; GLMP: 10 runs/arm/task; GEM §4: 3 tasks, 120 runs total ≈ 30 per arm across four arms, tmux $22.00 as the third arm — arithmetically consistent). GLMP Table 4's per-arm detail, carried here because the disagreement row is where a reader checks it: tmux $0.3729 / 1m28.7s; terminalcp CLI $0.3865 / 1m37.2s; terminalcp MCP $0.4804 / 1m22.2s (fastest per-turn, most expensive per task, ease-of-use rank 1); screen $0.6003 / 1m46.2s; DSV adds that **tmux beats the bespoke CLI by 10–22% on cost on simple tasks, while the bespoke tool is 39% cheaper than tmux on complex tasks** — task complexity flips the winner. Use as **transport-overhead background only**; never cite either aggregate |
| Response `max_tokens` default | DS §4.6: "sane default 2,000–4,000". GEM §7 reference implementation: 1,000 (vs Context7's 5,000). GLMP: 1,000–2,000 ceiling enforced server-side | **Not reconciled by any source.** The bundle carries the spread ([retrieval-design.md](retrieval-design.md) §3.6) and picks per deployment profile at build time; server-side budget precedents (Microsoft Learn `maxTokenBudget`, Cloudflare ~6,000-token response cap) bound the range |
| Code Mode for docs | GLMP: profile for sandbox-equipped harnesses with long multi-step tasks. GLMC/DSV: wrong default for 2–4 tools, near-zero headroom below Anthropic's ~10-tool threshold | **Optional, gated adapter.** Adopt only on measured win in multi-hop workflows (see [interfaces.md](interfaces.md)) |
| Context7 Pro quota | KIMI: 5,000 calls + $10/1k. GLMC/GLMP/GPT1/GPT2/DS: 2,000 calls/seat + $5/1k | Follow the majority audited figure (GLMC audited the pricing page); KIMI flagged in [sources.md](sources.md) |
| Cloudflare native-MCP denominator | Body: 1.17M tokens. Meta: "over 2 million" | Headline 99.9% is **approximate, source-inconsistent**; quote the mechanism, not the precision |
| Naive retrieval harm (CloudAPIBench) | GPT1: "naïve retrieval harmed common APIs by 39.02pp". GPT2: that figure is the *suboptimal/unselective retriever* high-frequency drop (−39.02pp); selective retrieval *gained* 8.20pp overall | Follow GPT2: **selective** retrieval helps, unselective retrieval can hurt — the distinction is the finding |

## Open questions (unresolved by any source)

1. **Does retrieval still beat parametric memory for popular libraries post-training-cutoff?** DocPrompting's gains are 2022-era (GLMC Finding 6 flags this as unmeasured). Failure mode argument: agents cannot know which recalled API details are stale; provenance-carrying retrieval is the only checkable channel.
2. **Does docs-visible-to-main-model beat docs-behind-subagent for interactive coding?** DSV marks this an untested heuristic (§4(f)).
3. **Skill/instruction-file size cost for a novel CLI**: Scalekit's CLI+Skills arm showed mixed results — the ~800-token skill file **raised** cost on the simple task (1,365 → 4,724 tokens) and **lowered** it on the most complex (8,750 → 6,860) — no factorial study exists (GLMC Finding 2).
4. **Cold-start ingestion economics** (first touch of an un-indexed library): unquantified (GLMC Gap 6; GPT2 §6.4 supplies formulas, not numbers).

## What would change the verdict

Reversal conditions from GLMP Ch.12, adopted as standing watch items:

- **Platform tool search becomes free and universal** → CLI's schema advantage narrows; decision rests entirely on retrieval quality and data ownership. Adapter priorities invert; core survives unchanged.
- **llms.txt publishing stalls** → ingest degrades to repo-crawling; local-first maintenance cost rises; hosted federation strengthens.
- **Parametric knowledge catches up** (continuously-refreshed training corpora) → category compresses to long-tail coverage; favors hosted breadth over local depth.

None hold as of 2026-10-27. Re-run the Phase 0 measurement quarterly ([benchmark-protocol.md](benchmark-protocol.md)).
