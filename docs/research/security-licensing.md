---
type: reference
title: Security & Licensing — Docs Are Untrusted Input
description: The threat model for a tool that inserts third-party text into agent context (prompt injection, corpus poisoning, SSRF, parser attacks), the mitigation set, the licensing/ToS ingestion gate, and the code-mode sandbox contract.
tags: [security, prompt-injection, licensing, sandbox, ssrf, untrusted-input]
timestamp: 2026-10-27T19:00:00Z
resource: docs/research
sources: [GLMC, GPT2, DS, KIMI, GEM]
---

# Security & Licensing

A docs tool inserts third-party text into an agent's context, adjacent to the agent's instructions. The security literature treats this channel seriously; **documentation content is an untrusted input channel, full stop** (GLMC Finding 7, unanimous across sources). Design for it from day one — not as hardening later.

## 1. Threat model

| Threat | Evidence |
|---|---|
| Indirect prompt injection via crawled page, example, or llms.txt | AgentDojo (NeurIPS 2024): 97 tasks, 629 security test cases measuring utility-under-attack (GLMC) |
| Corpus poisoning | PoisonedRAG (USENIX Sec 2025): **5 malicious texts in a corpus of millions → 90% attack success** for a target question; evaluated defenses insufficient. Docs analog: poisoned page, mirror repo, or malicious llms.txt entry steering the coding agent (GLMC) |
| Context poisoning via registry metadata | ContextCrush (Noma): even 2 read-only MCP tools are a viable poisoning vector via "custom rules" riding into context (KIMI §1 — single secondary write-up) |
| Malicious skill/extension supply chain | OpenClaw incident (GEM §5; GLMC Finding 5): 10,000+ public agentic-harness instances exposed, 12% of community skills carried malicious payloads or credential leaks, with session hijacking among the incident classes; Anthropic subsequently blocked subscription usage in response — the CLI-ecosystem analog of corpus poisoning |
| Query rewriting as an injection surface | HN commenter on Upstash's redesign: why is an LLM-rewritten query safer than the raw query? (GLMC) |
| Availability pressure → mirrors of unknown integrity | rate-limit-locked refresh bugs push users toward unvetted mirrors; local corpus removes the pressure (GLMC Finding 7) |
| SSRF / DNS rebinding / redirects in any crawler | GPT2 §10.1 threat matrix |
| Parser/dependency compromise (XML entities, YAML tags, OpenAPI `$ref`, notebook execution, git hooks/submodules/filters) | GPT2 §10.1 |
| Code-mode sandbox escape, exfiltration | GPT2 §10.2 |

The MCP spec (2025-06-18) assigns the duties itself: servers validate inputs, rate-limit, sanitize outputs; clients validate results before passing them to the model and log usage for audit; treat tool annotations as untrusted (GLMC Finding 7).

## 2. Mitigation set (the security MVP)

Tractable because the domain is narrow and read-only (GLMC Finding 7):

1. **Read-only scope** — a successful injection cannot pivot into state changes.
2. **Data, not instructions** — mark retrieved text as data; keep it separate from system/tool instructions; never let source text change tool policy.
3. **Sanitize at ingest** — strip non-content markup; quarantine instruction-like sections; strip or neutralize ANSI/control sequences for terminal sinks; never render active HTML.
4. **Provenance on every span** — source URL, version, commit, content hash on every result, so model and user can attribute claims; this is both a security and a trust feature ([retrieval-design.md](retrieval-design.md)).
5. **Never execute fetched code** — examples display as data; no auto-execution.
6. **No credentials from docs** — redact secrets before persistence and output; no credentials in snippets, logs, or caches; short-lived least-privilege tokens.
7. **Prefer official sources + signed/versioned snapshots**; source allowlists / trust tiers; mutable websites require an approved domain attestation or stay `source_untrusted` (GPT2 §7.1).
8. **Log provenance, not user queries** — redacted/keyed-fingerprint telemetry; sensitive queries stay local by default (DS §4.8, GPT2 §10.1).
9. **Local corpus is itself a control** — operator controls ingestion, so poisoning requires compromising the operator's sources rather than a multi-tenant crawl pipeline (GLMC Finding 7).
10. **Egress discipline** — central constrained fetch worker; HTTPS-only; reject non-approved schemes, userinfo, private/loopback/metadata IP forms, Unix sockets; canonicalize URLs; pin checked destination IP at connect time preserving TLS SNI validation; recheck every redirect; bound response bytes (compressed/decompressed), redirects, and work (GPT2 §10.1).

Local single-user storage controls (GPT2 §6.5, §10.1): per-user no-follow ownership/mode checks on SQLite/WAL/temp/blob paths (0700/0600); OS-keystore/KMS-backed encryption for secrets/private docs; secure temp cleanup; disable core dumps; symlink-safe locking; refuse network filesystems unless locking semantics verified.

## 3. Licensing and ToS: a first-class design constraint

A redistributable corpus of official documentation is a copyright artifact (GLMC Finding 6; GPT2 §2.7):

- Fair-use analysis centers on **transformativeness** — a tool returning verbatim doc spans sits on the reproduction end of the axis (practitioner legal guide, not legal advice).
- Site ToS can prohibit AI ingestion even where robots.txt is silent; circumventing anti-bot measures risks statutory damages regardless of data being public.

**Ingestion registry** — every source enters a per-source state machine, **deny by default** (GPT2 §2.7):

```
pending_review → approved_fetch → approved_index → approved_embed
              → approved_cache → approved_snippet/redistribute
terminal/revocation: denied | expired | revoked
```

Required decisions per source: publisher identity + auth method; license (SPDX/URL or unknown) with indexing/snippet/embedding/redistribution flags; robots + ToS + crawl-delay + exception approval record; retention/output mode (transient fetch vs private cache vs shareable index) with attribution requirements; revocation/deletion contact + SLA + tombstone propagation. Each derivative artifact (normalized text, chunk, embedding, snippet) carries the license decision and deletion dependency. Runtime policy: attribution/notices emitted when required; mixed-license results partitioned and labeled; full-document output blocked when only snippets are approved; revoked/expired → `not_available`.

Practical consequences (GLMC Finding 6): per-library license gating at ingest; CC-BY-style attribution carried into every citation; redistribution-restricted doc sets excluded from any published corpus or served only from the user's own local fetch; **bring-your-own-corpus path so the tool never ships a corpus it has no right to redistribute**.

## 4. Code-mode sandbox contract (only if Code Mode ships)

Code mode is an execution environment, **not a trusted extension of the host** (GPT2 §10.2). Minimum contract — acceptance criteria, not optional hardening:

1. Documented isolation backend + OS matrix; feature-detect primitives at startup and **fail closed** — disable code mode rather than run unsandboxed. No "best effort" fallback.
2. Rootless unprivileged process/container: dropped capabilities, `no_new_privs`, seccomp, separate namespaces.
3. Pinned, provenance-checked runtime image; read-only base filesystem; ephemeral workspace.
4. No host sockets, metadata services, browser profiles, or ambient secrets.
5. Signed, one-time **brokered `docs` capability** bound to tenant/actor/scope/request/sandbox identity/generation/epoch/expiry/nonce; atomic consumption against durable nonce state; sandbox cannot call index or credential store directly.
6. Network disabled by default; if enabled, allowlisted proxy with connect-time destination checks + request/response budgets.
7. Hard limits: wall time, CPU, memory, child processes, file size, output tokens, concurrency.
8. Structured redacted telemetry (keyed query fingerprint, never raw query; never raw secrets).
9. Operator-visible kill switch; deterministic cleanup after cancellation.

Initial support limited to a declared Linux/runtime matrix (rootless user namespaces, seccomp, cgroups, OCI backend); macOS/Windows/Docker Desktop unsupported until a VM-backed implementation + escape-test suite is registered (GPT2 §10.2).

## 5. Multi-user / hosted deployments (if ever)

"MCP governance" is only a valid claim when these exist outside the protocol (GPT2 §10.3): identity-to-tenant mapping, authorization decision point, retention + deletion SLA, tamper-evident audit logs, quota policy, revocation process, backup/replica deletion, availability SLO, RTO/RPO, key-rotation schedule, incident runbooks. Proposed hosted-profile gate: 99.9% monthly availability, RTO ≤1h, RPO ≤15min, zero fail-open for auth/KMS/revocation/audit dependencies, source-revocation propagation ≤15min, warm local search p95 ≤2s. Owners assigned by role before collection (product/security/platform/privacy/evaluation). For multi-tenant invocation of the CLI surface (cloud agents), GEM §5's governance list applies: OAuth 2.1 with PKCE, per-tenant namespace isolation, structured tool-call audit logging. Its rationale: standard CLIs rely on ambient credentials (`~/.config` token files, global environment variables), lack protocol-level tenant isolation, and leave only unstructured shell histories rather than user-attributed audit logs — the OpenClaw incident (§1 above) shows the blast radius. Ambient CLI credentials remain acceptable only for the local single-user profile. For the docs MVP (local, read-only, single-user): ambient CLI credentials are fine; per-user OAuth/tenant isolation is out of scope until this profile is targeted (GLMC Finding 5).
