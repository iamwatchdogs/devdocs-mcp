# Ticket Workflow

Run only when the user explicitly says to implement a ticket or a GitHub issue.

1. Read the requirements from the ticket or issue.
2. Dispatch subagents for leg work — research, exploration, verification — in parallel where the work splits, until you know the shape of the change.
3. Critical concerns or conflicts? Stop here and use the `grill-me` skill — or the closest conflict-resolution process the harness offers — to reach a common understanding with the user before you write code.
4. Create a new branch from `main`.
5. Implement the ticket requirements. Split the work into commits per AGENTS.md §Git commits.
6. Hand the change to the user for review.
7. When the user asks for a code review, run the `code-review` skill — or the closest review process the harness offers — to compare the branch with `main`. Reply in ASD-STE100 and end with a table of issues (issue, severity, probable cause) plus possible fixes.
8. Iterate on fixes and reviews with the user's input.
9. When the user confirms they are happy with the changes, do all four: push the branch, link it to the ticket or issue, apply the `in-progress` label to that ticket or issue, and raise the PR per `pr-creation.instruction.md`.

**Done means**: the branch is pushed, the ticket or issue carries the PR link and the in-progress state, and the PR follows the PR creation contract.
