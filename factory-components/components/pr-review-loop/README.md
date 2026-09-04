# pr-review-loop component

Run a review→fix→push convergence against a pull request: an agent
reviews the PR following a reviewer instruction — a repo-specific
document when one is configured, otherwise the component's built-in
default — a typed judge decides whether blocking issues remain, and
the loop escalates to a human, iterates through fixes and pushes, or
converges and posts the verdict as a PR comment. Extracted and
generalized from identus-ws's pr-review-loop.

## The instruction contract

The loop's convergence gate is a typed judge that asks whether the
review verdict contains **blocking issues**, and its fix prompts speak
the same vocabulary. That taxonomy is the contract between the loop
and its reviewer instruction:

- a configured instruction document (`reviewInstructionFile`) must
  define what counts as a **blocking** issue for this repository and
  instruct the reviewer to classify findings as blocking or
  non-blocking;
- the loop's review prompt asks for that classification in both
  modes — enforcement for instruction documents that under-specify
  the output format (redundant with a compliant document by design).

Verdicts that do not reduce to a blocking/non-blocking classification
— score gates, approve/request-changes votes, report-only reviews —
are a **different component**, not an instruction swap: loop shape is
component policy, and this loop's shape is the convergence gate
above. Swapping the instruction changes what "blocking" means for
the repo, not what the loop does with the classification.

## The built-in default

Omit `reviewInstructionFile` and reviews run against the component's
built-in default instruction — no instruction file to author, no
dependency on this repository's layout. The default
(`default-instruction.luau`, next to this README) is a full reviewer
persona that satisfies the contract: its Output section directs the
reviewer to classify every finding as BLOCKING (must be resolved
before the change is accepted) or NON-BLOCKING. It is the contract's
reference instance — copy it as the worked example when graduating to
a configured document. A configured document wins over the default.

The default is normal versioned behavior: tuning it changes
zero-config reviews on upgrade, and configuring a document is how a
repo pins its reviewer.

## Environment requirements (declared, not bundled)

- **Work agent able to act on the repository and the PR host** — read
  the repo, push commits to the PR branch, and comment on the PR
  (typically via the `gh` CLI on the agent's PATH, with credentials in
  the agent subprocess's environment).
- **The reviewer instruction document, when configured** — the
  repo-relative path given by `reviewInstructionFile` must exist and
  be readable by the agent. Without one the built-in default runs
  (no file required).
- **Judge agent** — any agent that can answer typed boolean prompts.

## Config (data plus declared agent handles)

```lua
local prReview = require("<mount>/factory-components/components/pr-review-loop/component")

local loop = prReview.new({
	agent = ptah.agent("claude"),        -- work agent handle
	judgeAgent = ptah.agent("claude"),   -- judge agent handle
	model = "opus",                      -- optional: work model id
	judgeModel = "haiku",                -- optional: judge model id
	-- optional (default: the built-in instruction): repo-relative
	-- reviewer document; must classify findings blocking/non-blocking
	-- (the instruction contract above); a configured file wins
	-- reviewInstructionFile = ".ptah/instructions/reviewer.md",
	dryRun = false,                      -- optional: never push (default false)
	maxIterations = 15,                  -- optional: cap (default 15)
})
```

## Operations

- `loop:review(prUrl)` — run the loop against one pull request; the PR
  URL is per-call data and the sole repository context. Returns the
  final accepted review verdict text.

With `dryRun = true` the commit-and-push step is skipped entirely: the
loop still reviews, judges, and fixes, but never pushes to the PR
branch — a gate for rehearsing instruction changes against a real
reviewer without pushing. The converged session still posts the
verdict comment: dry-run gates the branch, not the PR conversation.

The component ships facade-only (`:review`). A `run()` daemon
convenience (looping over open PRs) was deliberately deferred: it is
sugar over `std.daemon` + `:review` and can be added without breaking
the facade.
