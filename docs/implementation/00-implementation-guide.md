---
type: index
title: Implementation Guide — Index
description: The entry file for the devdocs implementation guide. It maps the guide files, fixes the reading order, and states the writing standard. The research bundle in docs/research supplies the evidence. This guide turns that evidence into a build plan.
tags: [devdocs, implementation, index, asd-ste100]
timestamp: 2026-10-28T00:00:00Z
resource: docs
derives-from: [docs/research]
---

# Implementation Guide — Index

This guide gives the complete build plan for the `devdocs` tool. The tool is a local-first documentation retrieval core for coding agents. The research bundle in [docs/research](research/index.md) supplies the evidence. This guide turns that evidence into decisions, file trees, schemas, and commands.

Every external version in this guide was verified against live sources on 2026-10-28. The verification notes name the source for each fact. Cargo.lock pins the exact dependency versions after the first build.

## File map

| File | Topic |
|---|---|
| [01-tech-stack-decision.md](01-tech-stack-decision.md) | The technology decision, the rejected alternatives, and the reversal conditions |
| [02-project-scaffolding.md](02-project-scaffolding.md) | Scaffold commands, file tree, Cargo workspace, config files |
| [03-code-quality-toolchain.md](03-code-quality-toolchain.md) | Rust toolchain pin, formatter, lints, hooks, MSRV policy |
| [04-testing-strategy.md](04-testing-strategy.md) | Test frameworks, test pyramid, fixtures, coverage, mutation tests |
| [05-architecture.md](05-architecture.md) | Layer design, crate graph, data flow, error model, configuration |
| [06-data-model.md](06-data-model.md) | SQLite schema, FTS5 setup, migrations, hashing policy |
| [07-ingest-pipeline.md](07-ingest-pipeline.md) | Source adapters, fetch guard, license gate, segmentation, dedup |
| [08-retrieval-interfaces.md](08-retrieval-interfaces.md) | Hybrid search, result shape, CLI contract, MCP adapter |
| [09-security-implementation.md](09-security-implementation.md) | Threat controls, SSRF rules, injection filter, storage controls |
| [10-dependency-audit.md](10-dependency-audit.md) | Audit tools, allow lists, update cadence, new-dependency checklist |
| [11-ci-cd-release.md](11-ci-cd-release.md) | CI workflows, release builds, crates.io publish, coverage upload |
| [12-roadmap-milestones.md](12-roadmap-milestones.md) | Phases, milestones, acceptance criteria, gates |
| [13-implementation-plan.md](13-implementation-plan.md) | Step-by-step build order with code sketches and a risk register |
| [14-benchmark-eval.md](14-benchmark-eval.md) | Golden set, eval harness commands, gate metrics, regression policy |

## Reading order

1. Read [01](01-tech-stack-decision.md) for the technology decision.
2. Read [05](05-architecture.md) for the shape of the system.
3. Read [02](02-project-scaffolding.md) and [03](03-code-quality-toolchain.md) to set up the repository.
4. Read [12](12-roadmap-milestones.md) and [13](13-implementation-plan.md) to plan the work.
5. Read the layer files [06](06-data-model.md) to [09](09-security-implementation.md) before you build each layer.
6. Read [04](04-testing-strategy.md), [10](10-dependency-audit.md), [11](11-ci-cd-release.md), and [14](14-benchmark-eval.md) to wire in quality controls.

## Writing standard

All guide files follow ASD-STE100 Simplified Technical English, Issue 9 (January 2025). The applied rules:

- Procedural sentences have 20 words or fewer. One instruction per sentence.
- Descriptive sentences have 25 words or fewer. Paragraphs have 6 sentences or fewer.
- Active voice. Simple present, simple past, simple future, or imperative tense.
- "must" shows obligation. "can" shows ability. "shall" and "should" are not used.
- No semicolons. No contractions. No Latin abbreviations. Write "for example", not "e.g.".
- "-ing" words appear only inside technical names, for example "embedding model".
- Numbers are digits. A number with a unit counts as one word.

Exemptions. Code blocks, file paths, commands, identifiers, schema names, and table values for technical values stay in their normal form. Each identifier counts as one word per STE rule 8.6.

## Conventions

- The binary name is `devdocs`. The repository name is `devdocs-mcp`.
- The storage root is `$XDG_DATA_HOME/devdocs` on Linux, `~/Library/Application Support/devdocs` on macOS. The environment variable `DEVDOCS_HOME` overrides both.
- Provenance references use the form `(research: file.md §section)`. The referenced file lives in [docs/research](research/index.md).
- Conflicts between research sources are recorded, not averaged. The conflict list lives in [research/verdict.md](research/verdict.md).

## Change procedure

1. Change one guide file per pull request when the change is large. Related files can change together when the change is small.
2. Update the `timestamp` field in the file header.
3. Keep the version tables current. Re-verify versions when a file names a version and the date is older than 90 days.
4. Record architecture changes as a new section in [01](01-tech-stack-decision.md) with the date and the reason.
