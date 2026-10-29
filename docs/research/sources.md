---
type: source registry
title: Source Registry — Dev-Docs Tooling Research
description: The eight raw research documents this knowledge bundle aggregates, with provenance keys, scope, and reliability notes. Raw files are read-only; this registry is the single map to them.
tags: [devdocs, mcp, cli, research, provenance]
timestamp: 2026-10-27T19:00:00Z
resource: docs/research
sources: [DS, DSV, GLMC, GLMP, GPT1, GPT2, KIMI, GEM]
---

# Source Registry

Eight independent deep-research runs (2026-10-26 → 2026-10-27) on one question: **how should a Context7-style developer-documentation tool for coding agents be built?** Every claim in this bundle carries a provenance key from the table below. Where sources disagree, the disagreement is stated in [verdict.md](verdict.md) and carried into the topic files — never silently resolved.

Raw documents live in [`raw/`](raw/) and are read-only inputs. This bundle is the working copy. The registry covers the eight registered inputs (seven markdown, one PDF); `raw/gemini-argon-4-high-research.pdf` is the archival original of the GEM markdown (user-added 2026-10-27 02:38; a normalized phrase-level diff confirmed the two carry identical content — only citation markers, math formatting, and whitespace differ), and `raw/glm-research.pdf` is both a registered input (GLMP) and that run's archival original.

## Provenance keys

| Key | File | Model/run | Scope emphasis | Evidence tier |
|---|---|---|---|---|
| **DS** | `raw/deepseek-research.md` | DeepSeek | Number normalization; the §4.1–4.8 argument-aware retrieval contract (three levels, hybrid retrieval, citations, budgets, fallback ladder); CLI exit-code contract; Bifrost code-mode scale test (§2); build plan with eval gates | Careful cross-source synthesis; flags vendor inconsistencies itself |
| **DSV** | `raw/deepseek-v4.1-1017-research.md` | DeepSeek v4.1 | Interface-evidence normalization table (Cloudflare/Anthropic/Speakeasy code-mode numbers); Zechner denominator caveats; the §5.1 workflow-cost benchmark protocol; eager-vs-lazy reframe; llms.txt as supply-side ingest | Careful synthesis; architecture- and measurement-heavy |
| **GLMC** | `raw/glm-5.3-flash-high-council-validated-research.md` | GLM 5.3 Flash | Source audits (Scalekit raw data, Context7 pinned commits); claims ledger C1–C8; licensing | Highest rigor: raw-data audit, 65 sources, external council review |
| **GLMP** | `raw/glm-research.pdf` | GLM (report format) | Context tax measurements; cost model ($/1k queries); scenario matrix; 90-day roadmap | Practitioner-report tier; some numbers secondhand (flagged in text). Keys `GLMP §n` / `GLMP Ch.n` both mean **chapter n of the PDF** (Ch.1–12); GLMP has no finer section numbering |
| **GPT1** | `raw/gpt-sol-6.1-xhigh-research-v1.md` | GPT-Sol 6.1 xHigh | Context7 internals from source; benchmark table; CLI/MCP/code-mode contracts | Careful synthesis; evidence labels per claim |
| **GPT2** | `raw/gpt-sol-6.1-xhigh-research-v2.md` | GPT-Sol 6.1 xHigh (v2) | Superset of GPT1 with evidence freeze (E1–E15 anchors + SHA-256), licensing gate, threat matrix, hardened benchmark protocol | Highest: every high-precision claim tied to a pinned anchor; supersedes GPT1 where they differ |
| **KIMI** | `raw/kimi-instant-research.md` | Kimi Instant | Strict non-comparability caveats; its own normalized benchmark table; ToolRet/RULER/ORD-QA grounding; third-party browser-automation CLI-vs-MCP benchmark; two-tool MCP recommendation; report-absolute-tokens benchmarking advice | Careful synthesis; strictest about non-comparability |
| **GEM** | `raw/gemini-argon-4-research.md` | Gemini Argon 4 | Effective-Tokens cost normalization (ET = m·(1.0·I + 0.1·C + 4.0·O)); Ref's session-aware retrieval + trajectory dedup; Zechner Haiku security-tax mechanism; agent-native CLI design principles (7-point contract); llms.txt v2 execution contract; SQLite + sqlite-vec + FTS5 local engine blueprint; `defer_loading` tool search; harness patterns (lockfile pre-resolution, file-spilling, research subagents) | Newest run (added 2026-10-27 02:32); rich secondary aggregation — many figures restated from other vendors' posts, treat as practitioner tier |

## Reading the tiers

- **GLMC and GPT2** are the audit-grade anchors. When they conflict with other sources, they win; the conflict is still recorded.
- **GPT2 supersedes GPT1** by design (same lineage, revised). Corrections GPT2 makes to GPT1 are treated as truth and noted where relevant (e.g., CloudAPIBench context-format ablation in [retrieval-design.md](retrieval-design.md)).
- **DS, DSV, KIMI, GEM** are independent syntheses. Convergence across them (plus the anchors) is the strongest signal in this bundle; divergence is always flagged.
- **GLMP** reads numbers from secondary sources in places; its measurements are used with the "±30% tokenizer noise" caveat it itself applies.

## Known defects to carry forward

Defects found *inside* the sources — errors in the raw docs that this bundle corrects or flags:

1. **Cloudflare 1.17M-token denominator**: the same article states both "1.17 million" (body) and "over 2 million" (meta description); KIMI §2/§5 carries a third variant, "~244k tokens" for endpoint-per-tool mode. Headline "99.9%" is approximate and source-inconsistent. (DSV §0 note; GLMP uses 1.17M.)
2. **Context7 self-reported −65% tokens**: displayed averages imply ≈66%; stated −30% tool calls implies ≈25%. Reported both ways, never reconciled. (Raw figures in KIMI §1; the implied-average analysis is DSV §0/§1(a).)
3. **Speakeasy "up to 160×"**: a maximum across benchmark cases; implied averages are ≈30× (simple) and ≈11× (complex). (Headline in KIMI; implied-average analysis in DSV.)
4. **Scalekit dollar extrapolation does not reproduce** from its own token data and pricing — off by 16–107× for the CLI figure and 18–46× for the MCP figure. Token medians reproduce exactly. (GLMC Finding 2; reproduction arithmetic in [interfaces.md](interfaces.md) §1.)
5. **Zechner CLI-vs-MCP aggregates conflict across sources**: DSV reports "MCP 23% faster, 2.5% cheaper" for the same experiment GLMP summarizes as "MCP most expensive of four tools" ($0.4804 vs CLI $0.3865). DSV states the 3×4×10 design but flags the per-arm denominator as unverified; GEM §4 states the full shape (3 tasks, 120 runs total ≈ 10 runs/arm/task across four arms, tmux $22.00 as the third arm; 30-run dollar totals $19.45 MCP vs $19.95 CLI), which reconciles GLMP's 10 runs/arm (Table 4, LLDB task) exactly. GLMP Table 4's per-arm detail (tmux $0.3729 / 1m28.7s; terminalcp CLI $0.3865 / 1m37.2s; MCP $0.4804 / 1m22.2s — fastest per-turn; screen $0.6003 / 1m46.2s; MCP ranked 1st on ease-of-use) lives only in the raw docs and [verdict.md](verdict.md)'s disagreement row. The aggregates still conflict; treat as transport-overhead background only. (DSV §0/§3; GLMP Table 4; GEM §4.)
6. **CloudAPIBench context-format table** is a StarCoder2-3B oracle-condition ablation, not the GPT-4o headline result — GPT1 blurs this, GPT2 corrects it. This bundle follows GPT2.
7. **Context7 Pro quota**: KIMI states "5,000 calls ($10 per extra 1,000)"; GLMC (pricing-page audit), GLMP, GPT1, GPT2, **and DS** all state **2,000 calls/seat + $5 per 1,000 overage**. This bundle follows the majority audited figure; KIMI is the flagged outlier.
8. **Context7 library count** drifts by claim date: ~104k (independent evaluation, KIMI/GLMP) vs 110k+ (vendor-cited, DS/GPT1/GPT2). Treated as "≈10⁵, vendor-claimed, drifts."

## Dating

All sources retrieved 2026-10-26 → 2026-10-27. Pricing, quotas, and tool surfaces are date-stamped observations, not permanent facts. GPT2 maintains a full evidence-freeze table (anchors E1–E15 with SHA-256 digests); consult it before re-deriving any high-precision number (see [benchmark-protocol.md](benchmark-protocol.md)).
