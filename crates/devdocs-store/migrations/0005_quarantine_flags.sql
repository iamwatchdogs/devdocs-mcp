-- Migration 0005: the quarantine and review flags on chunks.
--
-- Source of truth: docs/implementation/06 §Schema, docs/implementation/07
-- §Injection filter. Applied migrations never change: a schema change is
-- a new migration file (docs/implementation/06 §Migrations).
--
-- Adds the quality-pass flags the injection filter writes and the serving
-- queries read. Non-destructive: existing chunks carry the defaults and
-- stay served. `quarantined` keeps a chunk out of every serving read;
-- `review_flagged` marks it for the operator list without blocking the
-- serve; `rule_names` records the filter rules that fired (comma-joined,
-- NULL when no rule fired); `code_caution` annotates content-derived
-- caution that survives a release.

ALTER TABLE chunks ADD COLUMN quarantined INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chunks ADD COLUMN review_flagged INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chunks ADD COLUMN rule_names TEXT;
ALTER TABLE chunks ADD COLUMN code_caution INTEGER NOT NULL DEFAULT 0;
