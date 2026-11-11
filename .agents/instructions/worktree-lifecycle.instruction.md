# Worktree Lifecycle

How every change gets its branch, and how the branch and its worktree go away after merge.

- **Isolation.** Handle every change on its own worktree on its own branch (`wt switch --create <branch>`, AGENTS.md §Worktrees). Never commit a change on `main`, and never commit one change on another change's branch. The rule covers every kind of change — a ticket, a babysitting fix, a doc edit — and it applies before the first commit, not only when subagents run in parallel.

**Done means**: every change lives on its own branch in its own worktree, and `main` carries no task's changes directly.
