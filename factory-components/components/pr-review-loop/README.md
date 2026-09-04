# pr-review-loop component

Run a review→fix→push convergence against a pull request: an agent
reviews the PR following a repo-specific instruction document, a typed
judge decides whether blocking issues remain, and the loop escalates to
a human, iterates through fixes and pushes, or converges and posts the
verdict as a PR comment. Extracted and generalized from identus-ws's
pr-review-loop.

## Environment requirements (declared, not bundled)

- **Work agent able to act on the repository and the PR host** — read
  the repo, push commits to the PR branch, and comment on the PR
  (typically via the `gh` CLI on the agent's PATH, with credentials in
  the agent subprocess's environment).
- **The reviewer instruction document** — the repo-relative path given
  by `reviewInstructionFile` must exist and be readable by the agent.
- **Judge agent** — any agent that can answer typed boolean prompts.

## Config (data-only)

```lua
local prReview = require("<mount>/factory-components/components/pr-review-loop/component")

local loop = prReview.new({
	agent = "claude",                    -- work agent (registry name)
	judgeAgent = "claude",               -- judge agent (registry name)
	model = "opus",                      -- optional: work model id
	judgeModel = "haiku",                -- optional: judge model id
	repo = "owner/name",                 -- target repository
	reviewInstructionFile = ".ptah/instructions/review-instruction.md",
	dryRun = false,                      -- optional: never push (default false)
	maxIterations = 15,                  -- optional: cap (default 15)
})
```

## Operations

- `loop:review(prUrl)` — run the loop against one pull request; the PR
  URL is per-call data, not config. Returns the final accepted review
  verdict text.

With `dryRun = true` the commit-and-push step is skipped entirely: the
loop still reviews, judges, and fixes, but never pushes to the PR
branch — a gate for rehearsing instruction changes against a real
reviewer without touching the PR.

The component ships facade-only (`:review`). A `run()` daemon
convenience (looping over open PRs) was deliberately deferred: it is
sugar over `std.daemon` + `:review` and can be added without breaking
the facade.
