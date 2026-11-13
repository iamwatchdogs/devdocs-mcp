# Phase 1 gate report — pilot — 2026-11-13

Reproducibility header (the tracked record of the official M10 gate run. The
run artifacts in `eval/runs/` are gitignored scratch, this file is the
permanent copy):

- Run date: 2026-11-13 (`eval/runs/2026-11-13-pilot-9`, produced by
  `just eval-corpus && just eval`. `just eval` exited 2 by design, see
  Triage).
- Harness version: `devdocs-eval 0.1.0` (`manifest.json` field `tool`).
- Repo git SHA at run time: `5291fdc` (branch `m10-phase1-gate`).
- Suite hash: `f60b00cd38a5c5630c8d2cae109e3430d4bc2626a8ee6cbe912bbe10d31a1ed6`
- Database hash: `efc9c912176e5ca48e902138daf570f88363e18176bc33f7b84fa8084fd3facf`
  (the fixture-rebuilt `eval/runs/corpus.db`, offline).
- Corpus (from `manifest.json`):

  | Library | Version | Kind | Commit |
  |---|---|---|---|
  | next.js | 16.2.9 | llms_txt | — (recorded via llms.txt) |
  | serde | 1.0.229 | git | `7fc3b4c30c94f73a96ebd1553f2b090d928fc3a8` |
  | zod | 4.6.7 | llms_txt | — (recorded via llms.txt) |

- Host: macOS, aarch64, 8 cores, `Mac14,7`.
- Question count: 48 across all eight task-type categories.
- Curation record: 116 minutes (`eval/golden/pilot.meta.json`,
  `curation_minutes`).

This report IS the Phase 1 baseline. Later gate reports carry their deltas
against the numbers on this page (the per-task table plus the
reproducibility header above implement that reporting style).

## Gate table

| Gate | Measured | Threshold | Phase | Status |
|---|---|---|---|---|
| Symbol recall | 1.000 (5/5) | ≥ 0.95 | 1 | PASS |
| hit@5 | 1.000 (39/39) | ≥ 0.8 | 1 | PASS |
| p95 latency | 46.6 ms (subprocess, after one warmup; placeholder CLI before M4) | < 400 ms | 1 | PASS |
| Cost per 1,000 queries | $0.00 | ≤ $0.20 frontier-class | 3 | PASS |
| Retrieval failures | 0.000 (M4 baseline pending) | −49% vs the M4 baseline | 2 | INCONCLUSIVE |
| Version correctness | 1.000 (44/44) | ≥ 0.99 | 1 | PASS |

## Per-task-type breakdown

| task_type | n | pass | fail |
|---|---|---|---|
| exact_api | 18 | 18 | 0 |
| how_to | 12 | 12 | 0 |
| discovery | 5 | 5 | 0 |
| migration | 3 | 3 | 0 |
| multi_library | 3 | 3 | 0 |
| agent_task | 2 | 2 | 0 |
| unanswerable | 4 | 4 | 0 |
| poisoned | 1 | 1 | 0 |

## Per-question table

Graded rows from `results.json` of the recorded run. The top ref is the
rank-1 in-process hit. `—` marks an abstention.

| Question | Task type | Grader | Pass | Top ref |
|---|---|---|---|---|
| nextjs-middleware-to-proxy-001 | migration | ref_match | pass | docs://next.js/v16.2.9/Proxy/Convention |
| nextjs-proxy-matcher-001 | exact_api | ref_match | pass | docs://next.js/v16.2.9/Exports/Matcher#config |
| nextjs-proxy-runtime-001 | how_to | ref_match | pass | docs://next.js/v16.2.9/Runtime |
| nextjs-response-headers-001 | discovery | ref_match | pass | docs://next.js/v16.2.9/Cache-Control |
| nextjs-csp-header-001 | how_to | ref_match | pass | docs://next.js/v16.2.9/Options/Content-Security-Policy |
| nextjs-serveractions-origins-001 | exact_api | ref_match | pass | docs://next.js/v16.2.9/Examples/CORS#allowedOrigins |
| nextjs-image-remote-patterns-001 | exact_api | ref_match | pass | docs://next.js/v16.2.9/Reference/Configuration options/remotePatterns |
| nextjs-image-svg-001 | how_to | ref_match | pass | docs://next.js/v16.2.9/Reference/Configuration options/dangerouslyAllowSVG |
| nextjs-image-devicesizes-001 | exact_api | ref_match | pass | docs://next.js/v16.2.9/Reference/Configuration options/deviceSizes |
| nextjs-script-beforeinteractive-001 | exact_api | ref_match | pass | docs://next.js/v16.2.9/Optional Props/beforeInteractive#RootLayout |
| nextjs-metadata-title-001 | how_to | ref_match | pass | docs://next.js/v16.2.9/Reference/Metadata Fields/title/template#metadata |
| nextjs-cli-commands-001 | discovery | ref_match | pass | docs://next.js/v16.2.9/Reference/Commands |
| nextjs-codemods-cra-001 | migration | ref_match | pass | docs://next.js/v16.2.9/Codemods/11/Migrate from CRA/cra-to-next |
| nextjs-codemods-react-import-001 | migration | ref_match | pass | docs://next.js/v16.2.9/Codemods/10/Add React imports/add-missing-react-import#Home |
| nextjs-router-push-001 | exact_api | ref_match | pass | docs://next.js/v16.2.9/router object/router.push/Resetting state after navigation#Page |
| nextjs-error-digest-001 | exact_api | ref_match | pass | docs://next.js/v16.2.9/Reference/Props/error.digest |
| nextjs-mdx-frontmatter-001 | how_to | ref_match | pass | docs://next.js/v16.2.9/Frontmatter#Page |
| nextjs-docker-deploy-001 | agent_task | ref_match | pass | docs://next.js/v16.2.9/Docker/Templates |
| nextjs-caching-usecache-interleaving-001 | how_to | ref_match | pass | docs://next.js/v16.2.9/Examples/Interleaving#Page |
| nextjs-route-cors-001 | how_to | ref_match | pass | docs://next.js/v16.2.9/Examples/CORS#allowedOrigins |
| nextjs-auth-authorization-001 | discovery | ref_match | pass | docs://next.js/v16.2.9/Authorization |
| nextjs-revalidate-tag-001 | exact_api | ref_match | pass | docs://next.js/v16.2.9/On-demand revalidation/revalidateTag#updateUser |
| nextjs-poisoned-tsconfig-001 | poisoned | ref_match | pass | docs://next.js/v16.2.9/ignoreBuildErrors |
| nextjs-request-rewrites-001 | exact_api | symbol_match | pass | docs://next.js/v16.2.9/Rewriting to an external URL/Rewrites with basePath support#rewrites |
| nextjs-generate-static-params-001 | exact_api | symbol_match | pass | docs://next.js/v16.2.9/What can make transitions slow?/Dynamic segments without generateStaticParams#generateStaticParams |
| nextjs-force-dynamic-001 | exact_api | symbol_match | pass | docs://next.js/v16.2.9/Caching fetch requests/Route segment config/dynamic#dynamic |
| nextjs-zod-metadatabase-001 | multi_library | ref_match | pass | docs://next.js/v16.2.9/Reference/Other fields/metadataBase |
| nextjs-zod-codec-001 | multi_library | ref_match | pass | docs://zod/v4.6.7/Defining schemas/Codecs |
| nextjs-zod-issues-001 | multi_library | ref_match | pass | docs://zod/v4.6.7/Zod Core/Issues |
| nextjs-no-db-client-001 | unanswerable | ref_match | pass | — |
| zod-uuid-validator-001 | exact_api | ref_match | pass | docs://zod/v4.6.7/Defining schemas/String formats/UUIDs |
| zod-email-validator-001 | exact_api | ref_match | pass | docs://zod/v4.6.7/Defining schemas/String formats/Emails |
| zod-codecs-001 | exact_api | ref_match | pass | docs://zod/v4.6.7/Defining schemas/Codecs |
| zod-coercion-001 | how_to | ref_match | pass | docs://zod/v4.6.7/Defining schemas/Primitives/Coercion |
| zod-enums-001 | exact_api | ref_match | pass | docs://zod/v4.6.7/Defining schemas/Enums |
| zod-catch-fallback-001 | how_to | ref_match | pass | docs://zod/v4.6.7/Defining schemas/Catch |
| zod-record-001 | exact_api | ref_match | pass | docs://zod/v4.6.7/Defining schemas/Records/z.record |
| zod-jsonschema-unrepresentable-001 | how_to | ref_match | pass | docs://zod/v4.6.7/JSON Schema/z.toJSONSchema()/unrepresentable |
| zod-refinements-001 | how_to | ref_match | pass | docs://zod/v4.6.7/Defining schemas/Refinements |
| zod-ecosystem-001 | discovery | ref_match | pass | docs://zod/v4.6.7/Intro/Ecosystem |
| zod-i18n-001 | discovery | ref_match | pass | docs://zod/v4.6.7/Migration guide/Internationalization |
| zod-stringbool-001 | exact_api | ref_match | pass | docs://zod/v4.6.7/Migration guide/Stringbool |
| zod-error-issues-001 | how_to | ref_match | pass | docs://zod/v4.6.7/Zod Core/Issues |
| zod-no-socket-schema-001 | unanswerable | ref_match | pass | — |
| serde-point-example-001 | exact_api | symbol_match | pass | docs://serde/v1.0.229/Serde   Build Status Latest Version serde msrv serde_derive msrv/Serde in action#Point |
| serde-main-001 | agent_task | symbol_match | pass | docs://serde/v1.0.229/Serde   Build Status Latest Version serde msrv serde_derive msrv/Serde in action#main |
| serde-no-orm-001 | unanswerable | ref_match | pass | — |
| react-hooks-001 | unanswerable | ref_match | pass | — |

## Triage

The Retrieval-failures row reads INCONCLUSIVE. The gate is a −49% delta
against the M4 baseline measurement, and that baseline does not exist yet:
measuring it is M11 work (docs/implementation/13 §M11, step 4). The row is
the accepted Phase 1 state, documented here. No threshold moved — the
protocol forbids widening a margin to fit the data, and no gate on this
page changed for this run. Until the M4 baseline lands, `just eval` exits 2
on the finished run by design: the recipe reports the honest state of the
gate table instead of a green exit.

## Phase 0 baseline (pre-devdocs docs workflow)

- recorded: 2026-11-02
- workflow: the current (pre-devdocs) docs workflow: hosted docs MCP plus web search over vendor docs pages
- schema rent: 550–1400 tokens per tool — research: interfaces.md §1 (GEM §4 single-tool band)
  - raw multi-tool MCP dumps run 17,600–80,000 tokens (GLMP §1); a 1–2-tool docs surface costs 600–1,200 tokens, close to an 800-token CLI SKILL.md
- per-query cost: hosted API $1.66 / raw MCP $3.66 per 1,000 queries — research: interfaces.md §1 (GLMP Ch.9 cost model)
  - frontier list pricing ($3/M in, $15/M out); hosted API = 1,500-token schema + 4,000-token response + 300 output; raw MCP = 26,000-token schema rent amortized over a 10-query session (2,600 input/query) + 500 in / 1,500 out; modeled estimates, not invoices (GLMP Ch.9 Table 6)

Re-run the probe quarterly and on the reversal conditions (docs/implementation/14 §Standing re-measurement).

## Corpus

- suite: pilot — sha256 f60b00cd38a5c5630c8d2cae109e3430d4bc2626a8ee6cbe912bbe10d31a1ed6
- database: sha256 efc9c912176e5ca48e902138daf570f88363e18176bc33f7b84fa8084fd3facf
- pipeline: Phase 0 stub — one FTS5 rung through devdocs-store; the hybrid pipeline replaces it at M4 (docs/implementation/13)
- CLI search path: placeholder until M4. The p95 row times process start and dispatch only.
