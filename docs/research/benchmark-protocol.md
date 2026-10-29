---
type: protocol
title: Benchmark Protocol — Settling the Interface Question
description: The step-by-step evaluation protocol every source says is missing from the public record. Each step ends on a completion criterion; the gates at the end are the adoption decision. No gate has been run as of 2026-10-27.
tags: [benchmark, evaluation, protocol, non-inferiority, eval-harness]
timestamp: 2026-10-27T19:00:00Z
resource: docs/research
sources: [KIMI, GPT2, GLMC, DS, DSV, GLMP, GPT1, GEM]
---

# Benchmark Protocol

**The missing evidence**: no published controlled benchmark runs CLI vs dynamic toolsets vs Code Mode vs a hosted incumbent on the same doc-lookup tasks with success rates. Every vendor benchmarks only itself (GLMC Finding 8, DSV §5, GPT2 §4.5). This protocol is the decision gate for every interface claim in [verdict.md](verdict.md) — a pre-registration without retained artifacts is not credible (the Scalekit audit demonstrates pre-registrations get abandoned in delivery; GLMC Finding 2).

Design rules distilled from the field's methodology critiques (GLMC Finding 8, GPT2 §11, DSV §5.1): within-study ratios only; never pool absolute token counts across models/prompts/catalogs/harnesses; report absolute tokens per query per arm alongside ratios, not percentages alone (KIMI §7, rec. 5); count cumulative billed tokens (re-billed per turn), not final context size; express cost per **verified success** including failures — the τ-bench **pass^k** metric exists precisely because single-attempt success overstates reliability (GLMC Finding 8); pin the harness (harness choice alone moves results up to 7 points — HAL; GLMC Finding 8. HAL's cross-benchmark rollout data — **21,730 rollouts, ~$40,000**, within-benchmark cost spread reaching three orders of magnitude depending on model, scaffold, and budget — is reported in GLMC Finding 5; cost claims without that context are noise).

Evaluation-design precedent (DS Phase 0): Redis Docs MCP's evaluation runs a **dual harness** — a *pure retrieval harness* (deterministic, cheap) and an *agentic harness* (stochastic, captures tool calls) — both against the same labelled dataset. Adopt the same split so retrieval quality and interface behavior are scored separately before being composed.

## Step 1 — Pilot: measure your own baseline

**Do**: harness probe measuring schema rent and per-query cost of the current docs workflow against the published bands in [interfaces.md](interfaces.md) (per-tool schema cost runs 550–1,400 tokens — GEM §4); ~20 questions, one library, to estimate question-curation effort and adjudication load before committing to the full set (GLMP Phase 0, DSV §5.1).
**Criterion**: baseline cost/query recorded; schema rent located within published bands (or flagged as a novel measurement); pilot effort estimated; per-arm denominator convention decided.

## Step 2 — Pre-register sizing and the decision rule

**Do**: pilot first, estimate the paired disagreement rate, declare confidence level (e.g. 95%) and power (e.g. 80%), then **calculate** the required held-out count for the declared margin. Set a maximum feasible count (cap) up front. If the calculation exceeds the cap, the study cannot resolve the margin — report *that*; do not widen the margin to fit (a different margin is a new study). Declare the aggregation rule, weighting, and multiplicity handling before data collection (DSV §5.1, GPT2 §11.3). GPT2's hardened defaults (§11.2–11.3), kept here as the large-study reference: **50 unique tasks per category × 5 runs each**, **≥10 independent library/site clusters per category**, and an **ICC 0.10 / 80%-power simulation** run *before* collection to verify the sizing can resolve the declared margins. This bundle's default sizing (~150–200 questions total, Step 3) is deliberately smaller than GPT2's hardened default of **50 tasks/category × 6 categories = 300 tasks** (§11 gives equal weight to its six categories); note the bundle's 8-category list in Step 3 is its own union across GPT1/GPT2/DSV/GLMC — GPT2's default covers six of them. If the powered calculation says the smaller set cannot resolve the margin, follow GPT2's sizing instead of widening the margin.
**Criterion**: written pre-registration exists with: margin, confidence/power, calculated n, cap, aggregation rule (equally-weighted vs workload-weighted strata), one-sided vs two-sided, and the multiplicity path (Bonferroni across candidates *if choosing among arms*; single unadjusted gate *if one committed candidate*).

## Step 3 — Build the golden set

**Do**: ~150–200 real doc questions total (development + held-out), stratified by task type × library, minimum 2 per cell — a ~70/30 split yields **≈45–60 held-out questions**, which covers the **24-question floor** — DSV §5.1's own stratification minimum (3 libraries × 4 task types × ≥2 questions per cell) — consistent in spirit with DS §7 Phase 0's golden-set sizing rule (20–50 real questions per core library), with room to grow the sparsest cells. Cells that can't meet the minimum are dropped and reported inconclusive — never folded into the headline rate. Each question pins the expected source and exact version up front. Include "does not exist" questions. Held-out split fixed until configurations are frozen; tune only on dev (DSV §5.1, DS §7 Phase 0, GPT2 §11.2).

Task categories (union of GPT1/GPT2 §11, DSV, GLMC Finding 8):

1. Known library + version, exact API lookup
2. Known library, unknown page, natural-language how-to
3. Unknown/niche library discovery
4. Version migration comparison
5. Multi-library comparison / cross-library composition
6. Agent coding task with a deterministic verifier
7. Unanswerable queries (abstention correctness)
8. Poisoned-doc cases (unsafe-content handling)

**Criterion**: stratification table filled (library × task type ≥2/cell); ground truth + acceptable citations pinned per question; dev/held-out split done; question set frozen before any arm runs.

## Step 4 — Freeze the environment

**Do**: one shared retrieval core wrapped three ways (CLI / 2–4-tool MCP / Code Mode) so the comparison isolates **interface, not retrieval quality** — otherwise a win is confounded with a better search function (DSV §5.1). Fix: model snapshot, task-prompt wording, harness, frozen source snapshot (URL + commit + hash + parser version), network state, retry policy, timeout, output budget. Cap interaction depth and record it in the manifest (GitHub's Copilot CI/CD agentic loops cap at 50 turns — GEM §4). Give each arm the minimum production-ready integration material it needs (MCP schemas; CLI skill/`--help`) — frozen before held-out runs — and count that metadata in the workflow-context-cost metric (DSV §5.1). Run two labeled comparisons: **capability-matched** (only the adapter changes) and **deployment-realistic** (each arm's normal production integration, deviations listed before the run) — never pool them (GPT2 §11.1).
**Criterion**: environment manifest published (corpus, task ledger, arm definitions, seeds, cache-eviction rules, PRNG schedule); a third party could reconstruct the denominator (GPT2 §11.2).

## Step 5 — Run arms with cache and failure accounting

**Do**: minimum comparison arms — CLI alone, CLI + thin MCP facade, one lazy MCP/dynamic-toolset baseline, plus (A) transport-isolation arms on the shared core and (B) optionally one real incumbent arm (Context7 MCP mode or `ctx7` CLI mode) reported separately as real-world context, never pooled into (A). **The incumbent baseline is the thing you would actually replace**: default to *your current documentation workflow* run on the same held-out questions against a matched corpus/version snapshot where feasible; a lean MCP baseline is a separate *interface* baseline for prototype selection only — if the real incumbent cannot be matched, a prototype-only gate is not evidence to replace it (DSV §5.1). GPT1's §11 arm list adds two retrieval-value references worth freezing where retrieval quality itself is on trial: a **no-retrieval baseline** (parametric knowledge only) and a **raw-page-fetch arm** (GPT1 §11). Per attempt record: cache state at each layer (model prompt cache, HTTP/CDN, retrieval index, OS), tool calls, hidden model calls, tokens by class (fresh/cache-read/cache-write/output), embedding/reranker calls, cost, outcome, timeout, crash, grader result. Compute cost per attempt under the Effective Tokens normalization so caching discounts and output asymmetry are priced in ([interfaces.md](interfaces.md) §1; GEM §4). **Failures stay in the primary denominator; retries never refill it** — report failure-adjusted results separately from conditional-on-valid-attempt results (GPT2 §4.4 catches Mintlify reporting valid-attempt-conditional accuracy; GPT2 §11.2). Completed-task definition travels with the results: GPT2's default counts a task complete only when **≥3 of 5 preregistered runs pass** (GPT2 §11.3). Randomize arm order (Latin square, recorded seed); fresh isolated session per attempt; prevent arms from warming each other's caches (GPT2 §11.1–11.2).
**Criterion**: attempt ledger complete — one row per task × arm × run, cache state logged, zero silent exclusions.

## Step 6 — Grade with blinded rubric

**Do**: versioned rubric per question (full/partial/wrong with "partial" defined up front; **binary aggregation — partial counts as not-correct** for the primary gate). Answer correctness and version correctness scored **separately**. Fix the **per-question aggregation rule in advance** (e.g. *majority of 3 runs*, or *any-run-success*) so each question yields one binary outcome per arm (DSV §5.1). Two blinded graders, arm names/timings/token counts stripped, κ ≥0.80 and raw agreement ≥0.90 or recalibrate before analysis; ambiguous answers → two-reviewer + tie-break. Missing/empty output = failed response. For executable tasks, the deterministic verifier decides. Manually adjudicate a sample of judge decisions to bound judge error (GPT2 §11.2–11.3, DSV §5.1) — the Scalekit audit's "no judge verdicts in the public pack" is the failure mode this step prevents (GLMC Finding 2).
**Criterion**: every question has one binary outcome per arm; grader calibration thresholds met; adjudication log retained.

## Step 7 — Statistics and gates

**Do**: paired per-question comparison against the named incumbent; bootstrap (or exact) CI resampling **question-level pairs with repeats kept together** — GPT2's default is a **10,000-resample paired cluster bootstrap** (GPT2 §11.3); one-sided lower bound at the pre-registered level. Apply the pre-registered decision rule — do not select the "strongest" arm post hoc and gate it uncorrected. Cost accounting follows GPT2 §6.2's TCO shape: `cost_attempt = C_attempts / attempts` and the workload gate on `C_workload / completed unique tasks`, computed against **frozen price cards** declared in the manifest so pricing drift cannot flatter a late-run arm.

**Adoption gates (defaults; replace only by new pre-registration)** — from GPT2 §11.3 and DSV §5.1:

- **Quality non-inferiority**: candidate no worse than **−5 points** vs incumbent on answer and version correctness; floors: task success ≥85%, requested-version correctness ≥99% (with available pinned source), citation precision ≥95%, citation coverage ≥90%. Failures and silent version fallbacks count as **wrong**.
- **Efficiency superiority** (both required): point estimate ≥**20% lower** full-workflow cost per completed task AND one-sided upper bound <1.0× baseline; p95 latency ratio ≤1.25× or meets declared SLO. Full workflow context cost = shell/skill/discovery/returned-doc tokens, not just init tokens.
- **Safety**: zero confirmed secret disclosures, zero cross-tenant disclosures; adversarial-corpus unsafe-content rate within pre-declared threshold ([security-licensing.md](security-licensing.md)). GPT2's §11.3 safety-opportunity ledger sets the zero-event bar arithmetically: **at least 299 clean opportunities for a 1% upper bound, 598 for 0.5%** (Clopper–Pearson) — a small adversarial set cannot certify a low rate.
- **Interactive-episode gate**: ~12–20 multi-turn coding episodes (fixed repo + doc versions, completion rubric), held-out like questions, ≥3 per task type; adoption requires episode-level non-inferiority too (pre-declared margin in percentage points, converted to episodes only at analysis time). If the episode set can't reach required count, mark episode results exploratory and **restrict the adoption claim to lookup performance** (DSV §5.1).
**Criterion**: PASS / FAIL / INCONCLUSIVE reported against every threshold — with raw counts, valid and failed denominators, exclusions, cache state, versions, per-category results, uncertainty intervals, and the reproduction command. A single aggregate token number is insufficient (GPT2 §11.2).

## Standing re-measurement

Re-run the Phase 0 probe quarterly and on the reversal conditions in [verdict.md](verdict.md) (platform tool search, llms.txt supply, parametric-knowledge drift). The architecture in [architecture.md](architecture.md) is designed so any single reversal changes adapters and emphasis — not the core.

## Useful public benchmarks (reference, not gates)

ContextBench (retrieval-process metrics), CodeRAG-Bench (oracle-gap framing), AgentDocsBench, Mintlify docs-url-discovery-bench, Zvec agent benchmarks (GPT1/GPT2 reference sections).
