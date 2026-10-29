# Agent-Ready Repo Contract

For changes to AGENTS.md, hooks, or the instruction layer itself.

- `AGENTS.md` stays short (<200 lines): commands, layout, pointers. Instructions are how to decide; `docs/implementation/` is the decided design; neither duplicates the other or AGENTS.md.
- A new check lands in the justfile AND CI in the same change (`just ci` mirrors CI, so the two cannot drift).
- Commits follow conventional-commit format, enforced by the lefthook commit-msg hook: `type(scope): subject` — scope optional — with type one of feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert and subject ≤72 chars.
- Pre-commit runs `cargo fmt` (auto-staging fixes — expect your files reformatted mid-commit) and `clippy -D warnings`. A hook failure is fixed (`just lint-fix` for machine-applicable clippy fixes), never skipped.
- The change leaves the gate no weaker than it found it.

**Done means**: a fresh agent can clone the repo and get `just ci` green from AGENTS.md alone.
