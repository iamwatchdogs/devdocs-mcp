# Pull Request Creation

How every PR gets raised: one structure, one link, one assignee.

- Body: follow `.github/pull_request_template.md`. The template is the single source of truth for the PR body — do not restate or reorder its sections.
- Issue link: when an issue applies, bind the PR to it — a closing keyword (`Closes #N`) in the body and the issue in the template's "Linked issues" section. When no issue applies, say so in that section.
- Assignee: assign the PR to its author. Take the account id from the gh CLI (`gh api user --jq .login`, or pass `--assignee @me` to `gh pr create`).

**Done means**: the PR body matches the template, carries the issue link when one applies, and lists the author as assignee.
