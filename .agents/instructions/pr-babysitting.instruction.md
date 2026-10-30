# PR Babysitting

Run only when the user explicitly asks to review the comments on a PR or to babysit the PR.

Review bots (CodeRabbit, Codacy, and similar) add comments on recently pushed changes. Work the comments in order:

1. Read all bot comments, one by one.
2. Dispatch parallel research and validation subagents — one per claim — to judge each claim: legit, false positive, or does not apply here.
3. Implement a fix for every legit concern. One commit per concern; each commit stays green per AGENTS.md §Git commits.
4. Review the fixes with the `code-review` skill before you push.
5. Push the fixes.
6. Reply on each bot conversation. Start the reply with "This is a legit claim…", "This is a false positive…", or "This does not apply here…". Write the reply in ASD-STE100. Then mark the conversation as resolved.

**Done means**: every bot conversation carries a verdict reply and is resolved, and every legit claim has a pushed fix commit.
