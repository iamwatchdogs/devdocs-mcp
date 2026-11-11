# Worktree Lifecycle

How every change gets its branch, and how the branch and its worktree go away after merge.

- **Isolation.** Handle every change on its own worktree on its own branch (`wt switch --create <branch>`, AGENTS.md §Worktrees). Never commit a change on `main`, and never commit one change on another change's branch. The rule covers every kind of change — a ticket, a babysitting fix, a doc edit — and it applies before the first commit, not only when subagents run in parallel.

- **Post-merge cleanup.** When the change's PR has merged and its GitHub issue has closed, run the cleanup, in this order:
  1. Pull the merged changes into the parent branch — the branch the PR targeted, usually `main`. Run `git pull --ff-only` in the main checkout, or `git -C <main-checkout> pull --ff-only` from inside the worktree. `wt` never fetches, so the parent must carry the merge before the next step.
  2. Remove the worktree: `wt remove <branch>`. With the parent up to date, `wt` sees the branch content as integrated and deletes the local branch together with the worktree.
  3. Delete the remote branch: `git push origin --delete <branch>`. Skip this step when the repository already deleted the head branch at merge.
- The cleanup is part of the change. A merged change whose worktree, local branch, or remote branch still exists is not done.

**Done means**: every change lives on its own branch in its own worktree; after its PR merges and its issue closes, the worktree, the local branch, and the remote branch are gone, and the parent branch carries the merged changes.
