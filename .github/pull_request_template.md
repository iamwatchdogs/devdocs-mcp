<!--
PR body template — the single source of truth for PR structure
(.agents/instructions/pr-creation.instruction.md points here).
Delete the "Linked issues" section only when no issue applies.
-->

## Summary

<!-- What changed and why. One to three sentences. -->

## Linked issues

<!-- e.g. `Closes #12`. When no issue applies, write `None`. -->

## Changes

<!-- One bullet per logical commit. -->

## Testing

<!-- How you proved the change: `just` commands run, tests added or updated. -->

## Checklist

- [ ] The PR follows `.agents/instructions/pr-creation.instruction.md` (issue link, author assignee).
- [ ] `just ci` is green.
