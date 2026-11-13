---
type: guide
title: Benchmark and Eval Harness
description: The golden-set format, the eval harness implementation, the gate metrics with thresholds, run artifacts, and the regression policy that wires the research benchmark protocol into devdocs.
tags: [devdocs, eval, benchmark, golden-set, gates]
timestamp: 2026-11-13T18:32:00Z
resource: docs
derives-from: [docs/research]
---

# Benchmark and Eval Harness

The research bundle states that no controlled docs-domain benchmark exists, and it prescribes the protocol that closes that gap (research: benchmark-protocol.md). This file wires that protocol into the repository as a runnable harness. The harness measures the retrieval core. It does not replace the unit tests in [04](04-testing-strategy.md).

## Components

- `crates/devdocs-eval` — the harness binary. Reads a suite, runs the retrieve crate, scores results, writes artifacts.
- `eval/golden/*.jsonl` — golden question files, one suite per file. A `<suite>.meta.json` sidecar next to a suite records the curation minutes.
- `eval/fixtures/` — the committed fixture corpus the suites grade against (§Fixture corpus).
- `eval/runs/<date>-<suite>/` — run artifacts. Gitignored.
- `eval/baseline/*.json` — the standing Phase 0 baseline records (schema rent, per-query cost of the current docs workflow) that each run report carries.
- `just eval-corpus` — rebuilds the gitignored corpus database `eval/runs/corpus.db` from the committed fixtures, fully offline (§Fixture corpus).
- `just eval` — runs the default pilot suite against the corpus database through the harness binary and prints the gate table. A gate that is not PASS exits 2, so the recipe reports the honest state.

## Golden-set format

One JSON object per line:

```json
{
  "id": "nextjs-proxy-rewrite-001",
  "library": "next.js",
  "version": "16.2.9",
  "task_type": "exact_api",
  "question": "How do I rewrite an incoming request path in Next.js middleware?",
  "expect_refs": ["docs://next.js/v16.2.9/proxy#rewrite"],
  "expect_symbols": ["middleware"],
  "grader": "ref_match",
  "notes": "accepts the Proxy section; the middleware page alone is a partial"
}
```

Field rules:

- `task_type` comes from the research 8-category union (research: benchmark-protocol.md Step 3): `exact_api`, `how_to`, `discovery`, `migration`, `multi_library`, `agent_task`, `unanswerable`, `poisoned`.
- `expect_refs` pins the acceptable `docs://` refs. `expect_symbols` pins required symbols for symbol-recall scoring. One of the two is required.
- `grader` is `ref_match` (default), `symbol_match`, or `manual`. `unanswerable` questions expect an empty result set. A hit is a failure.
- Version is exact. A question without a pinned version is invalid input for the harness.

## Pilot (Phase 0)

Build a 20-question suite across 3 libraries before writing retrieval code. This calibrates curation effort and produces the baseline the gates need (research: architecture.md Phase 0). Suggested start: Next.js, Zod, and one library the project's own lockfiles use. Include at least 2 `unanswerable` questions. Record curation minutes in the run report. The research protocol treats that cost as a measurement, not trivia.

Status (2026-11-13): the pilot suite is curated (`eval/golden/pilot.jsonl`, 48 questions, `pilot.meta.json`) and the fixture corpus behind it is committed (§Fixture corpus): `just eval-corpus` rebuilds the corpus database offline from the recorded fixtures, and `just eval` grades the suite against it with real numbers (the Phase 1 gate run, [12](12-roadmap-milestones.md)).

## Fixture corpus

`eval/fixtures/` holds the recorded fixtures the suites grade against: one
recorded-response manifest per source version (the `devdocs add
--record-to` shape), the serde git fixture, the keep list, and the trim
tool. `eval/fixtures/README.md` documents the layout and the trim rules.

The corpus comes from a real-network recording, and that step stays
manual: run the add against the live source with `--record-to`, write the
keep list (the index URLs plus every page a golden question pins), and
trim with `eval/fixtures/trim.py`. The trim keeps only keep-listed
responses and rewrites each kept llms.txt index body so it links only
kept pages — the replay then fetches only listed pages, and every link
resolves to a recorded response. The tool verifies both directions
before it writes, and re-trimming a trimmed manifest is the identity.
The committed `inventory.json` beside each manifest records the recorded
statuses, the original body hashes, and the trimmed hashes.

`just eval-corpus` rebuilds `eval/runs/corpus.db` offline from the
committed fixtures: three `devdocs add` calls pinned to
`eval/corpus.toml`, each library's first add bootstrapping its license
gate at `approved_index`. `just eval` grades against that database.

## Harness flow

```text
devdocs-eval [--json] run <suite> [--db PATH] [--config PATH]
  1. load suite, validate every question (version pinned, refs well-formed)
  2. for each question: run the retrieve pipeline, record hits, ranks, timings
  3. score per question, aggregate per task_type and overall
  4. write eval/runs/<date>-<suite>/results.json + report.md
  5. print the gate table; exit 0 when all gates PASS, 2 on any FAIL
```

`--json` is a top-level flag and precedes the subcommand. The harness binary is `devdocs-eval`. The CLI subcommand wiring landed at M10 ([12](12-roadmap-milestones.md)): `devdocs eval run` proxies to it with the same flags (the global flag layer carries `--db` and `--config`). A set `--config` is forwarded into every spawned `devdocs search` call, so the run stays hermetic against the operator's user config; the spawned process carries no `DEVDOCS_*` environment variable, so the operator's environment cannot override the pin either. `just eval` passes `eval/corpus.toml`, whose `[search]` section pins the search knobs at the documented defaults. At the CLI edge, the harness input errors surface as the `USAGE` envelope (exit 1) and the operational failures as `UPSTREAM_ERROR` (exit 3), per docs/implementation/05 §Error model. A completed run with a blocked gate table prints it to stdout first, then the `EVAL_GATE` envelope (exit 2, docs/implementation/08 §Error codes). Exit codes: 0 every gate PASS; 1 invalid input (a broken suite, an unknown flag); 2 any gate FAIL or INCONCLUSIVE; 3 operational failure (a missing `devdocs` binary, I/O, a `devdocs search` call that exits 1, 3, or 4, or overshoots its 30-second deadline). Exit 2 is a valid timed miss (docs/implementation/08 §Error codes).

Timing note: the harness measures the in-process pipeline plus, in a second pass, the CLI subprocess end-to-end. The p95 gate is judged on the subprocess pass, cold-start excluded after one warmup call. Since M4 the timed call is the live search: an exit 0 (results served) or exit 2 (a valid query that served nothing, docs/implementation/08 §Error codes) contributes its wall time to the gate; any other exit, or a stall past the deadline, fails the run with exit 3 instead of contributing its wall time.

## Metrics

| Metric | Definition | Phase 1 gate |
|---|---|---|
| hit@5 | Fraction of questions with at least one expected ref in the top 5 results | ≥ 0.8 |
| symbol recall | Fraction of `expect_symbols` present in the symbol rung results for exact-lookup questions | ≥ 0.95 |
| p95 latency | 95th percentile of CLI subprocess wall time across the suite. Until M4, the CLI `search` path is a placeholder, so the row times process start and dispatch only | < 400 ms |
| cost per query | Model cost of the retrieval path (always $0 locally. Ingest embedding cost counted in Phase 3) | $0 |
| failure rate | Fraction of questions with a wrong or partial result under the binary grader | baseline for the −49% Phase 2 delta |
| version correctness | Fraction of questions where a result was served and the top result serves the answer's pinned version — the question's own pin, except `multi_library` where the matched expected ref's version decides. Unanswerable abstentions leave the denominator | ≥ 0.99 |

The binary grader: partial counts as wrong. The research protocol is explicit (research: benchmark-protocol.md Step 6).

Denominators: hit@5 counts only questions with non-empty `expect_refs`; symbol recall counts pinned symbols; failure rate counts every question. Version correctness counts the questions where a result was served. Unanswerable abstentions leave the denominator: an abstention has no served version to be wrong. The top result grades against the version the answer is pinned to: the question's own pin. A `multi_library` question grades against the version of the expected ref its top hit matches. When no expected ref matches the top hit, the question's own pin decides. The harness grades `ref_match` and `symbol_match`; a suite that carries the `manual` grader makes a run exit 1 until manual grading lands. Symbol recall grades from the rung results within the retrieval headroom window (4× the page limit, docs/implementation/08 §Rungs), so a suite pinning more symbols than the window holds cannot reach full recall.

## Gate table

The harness prints this table and writes it into `report.md`:

| Gate | Threshold | Phase |
|---|---|---|
| Symbol recall | ≥ 0.95 | 1 |
| hit@5 | ≥ 0.8 | 1 |
| p95 latency | < 400 ms | 1 |
| Cost per 1,000 queries | ≤ $0.20 frontier-class | 3 |
| Retrieval failures | −49% vs the M4 baseline | 2 |
| Version correctness | ≥ 0.99 | 1 |

A FAIL blocks the phase transition ([12](12-roadmap-milestones.md)). An INCONCLUSIVE (suite too small for the margin) is also a block. The protocol forbids widening the margin to fit the data (research: benchmark-protocol.md Step 2).

## Run artifacts

Each run writes:

- `results.json` — one row per question per run: id, rank list, timings, per-metric score, plus a `corpus` array: one row per indexed source (library, version, kind, origin_url) with its `commit_sha` (`null` for sources that record none, docs/implementation/06 §Schema).
- `report.md` — the gate table plus per-task-type breakdown.
- `manifest.json` — tool version, DB file hash, suite hash, host (os, arch, core count, CPU model when the platform reports one), a mirror of the `corpus` array, date. A third party can reconstruct the denominator from it without the database (research: benchmark-protocol.md Step 4 criterion).

The corpus identity matters: every result names the exact corpus it ran against. A gate claim without the manifest is not a claim.

Committed gate reports live under the tracked path `eval/reports/<date>-<suite>/` — the path carries the date, and the report carries the harness version in its reproducibility header (run date, `manifest.json` tool version, repo git SHA at run time, suite hash, database hash, corpus rows, question count, curation record). `eval/runs/` stays gitignored scratch. Every tracked report references the run dir it records, and the harness-rendered body sits intact below the header. The Phase 1 report is `eval/reports/2026-11-13-pilot/` with the CLI demo (`demo-cli.md`) and the MCP spot-check (`demo-mcp.md`) beside it.

## Regression gate

The implemented mechanics:

1. The eval gate job runs the full default suite in PR CI against the fixture-rebuilt database (`just eval-corpus` first, `just eval-ci` second — `eval-ci` runs `just eval`, permits the accepted exit 2, and fails only when a gate row that was PASS in the tracked report regresses), fully offline, and is a required check. A regression blocks merges to `main`, and therefore blocks any release PR, same as a failing test. Two limits apply: the block passes through the `gate` aggregate job, so it blocks as required only when branch protection requires `gate` — a repository settings step, not code; and a PR whose paths filter skips `eval/**` gets no eval check on that PR — the weekly canary covers those changes.
2. A weekly scheduled workflow (cron) re-runs the suite as a canary: it fails `main` and files an issue. Scheduled runs are canaries, not PR checks.
3. The fixture database is rebuilt from the committed recorded fixtures (`just eval-corpus`), fully offline, on every gate run.
4. Real-network re-recording stays manual and labeled: `eval/fixtures/README.md` documents the recording and trim steps.
5. An exit 2 with a documented triage in the tracked report is an accepted state, never a license to widen a threshold. The Phase 1 report carries the accepted INCONCLUSIVE on the Retrieval-failures row (its M4 baseline measurement is M11 work).

This reproduces Context7's benchmark loop locally: questions score the index, and bad content fails the gate before it serves (research: architecture.md §L3, context7-internals.md).

## Scaling up

The pilot and Phase 1 suites are small. Before any external adoption claim, grow to the hardened sizing from the research protocol: 50 tasks per category across 6 categories, 300 tasks, pre-registered margins, paired cluster bootstrap (research: benchmark-protocol.md Steps 2 and 7). Also add the interface comparison arms (CLI versus MCP versus a hosted incumbent) from Step 5, each against the shared retrieval core, with the incumbent run in its own labeled comparison.

## Standing re-measurement

Re-run the Phase 0 probe quarterly and on the reversal conditions in [01](01-tech-stack-decision.md). The quarterly run records: baseline cost per query of the current workflow, schema-rent bands, and any movement in the competitive boundary (research: verdict.md, reversal conditions).
