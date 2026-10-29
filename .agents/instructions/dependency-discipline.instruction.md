# Dependency Discipline

The bar a new or upgraded dependency must clear.

- The build must stay green in all three feature states CI checks — `--no-default-features`, `+exact-tokens`, `+embeddings` (`just features`). Heavy dependencies (embedding models, tokenizers) go behind an opt-in feature, default-off. One crate owns the dependency; the binary's feature forwards to it (`devdocs/embeddings = ["devdocs-store/embeddings"]`). While a binary feature is an empty no-op (README deviations), the change that wires the dependency also forwards the flag — that is what makes `just features` exercise the real build.
- Features must be additive: a crate compiled with different feature sets by different members is a build hazard. Disable default features when the defaults exceed the need, and pin features minimally in `[workspace.dependencies]` — the table deliberately carries minimal feature lists (e.g. reqwest with only `gzip`).
- `deny.toml` denies wildcard versions and yanked crates, warns on duplicate versions, and enforces a license allowlist. Run `just deny` and `just machete` before pushing.

**Done means**: `just features`, `just deny`, and `just machete` all pass with the new dependency, and heavy deps are default-off.
