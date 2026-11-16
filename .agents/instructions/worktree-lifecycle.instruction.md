# Worktree Lifecycle

How every change gets its branch, and how the worktree and the branch go away.

- **Isolation.** Handle every change on its own worktree on its own branch (`wt switch --create <branch>`, AGENTS.md §Worktrees). Never commit a change on `main`, and never commit one change on another change's branch. The rule covers every kind of change — a ticket, a babysitting fix, a doc edit — and it applies before the first commit, not only when subagents run in parallel. Every change gets a fresh worktree; a worktree never serves a second change.

- **Post-push cleanup.** When the change's GitHub issue has been implemented and pushed — the branch carries every commit and the PR is up — remove the worktree: `wt remove <branch>`. The branch stays: the PR still points at it, and the merge still needs it. If `wt` keeps the branch because the merge is not in `main` yet, that is correct at this stage. A later fix for the same change runs in a fresh worktree on that branch.

- **Post-merge cleanup.** When the babysitting is done and the change's PR has merged, run the cleanup, in this order:
  1. Remove the worktree: `wt remove <branch>` (skip when the post-push cleanup already removed it).
  2. Delete the branch locally and on the remote: `git branch -D <branch>` and `git push origin --delete <branch>` — `wt remove` may have already deleted the local branch when it saw the content as integrated. Skip the remote delete when another PR still points at the branch on the remote, and skip either delete the repository already did at merge.
  3. Pull the merged changes onto the parent branch — the branch the PR targeted, usually `main`: run `git pull --ff-only` in the main checkout, or `git -C <main-checkout> pull --ff-only` from inside a worktree. Run this pull only after the cleanup steps above, and always run it after a PR merge: `wt` never fetches, so the pull is what puts the merge into the local `main`.
- The cleanup is part of the change. A merged change whose worktree, local branch, or remote branch still exists — with no other PR pointing at the branch — is not done.

**Done means**: every change lives on its own branch in its own worktree; once its issue is implemented and pushed, the worktree is gone; after its PR merges and its issue closes, the local branch and the remote branch are gone (unless another PR still points at the branch on the remote), and the parent branch carries the merged changes.
