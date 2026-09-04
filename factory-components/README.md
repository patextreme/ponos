# Factory Components

The shared workflow library: repo-agnostic helper modules (`std/`) and
composable workflow components (`components/`) that consumer repositories
mount as source and drive through thin shims. See
`docs/adr/0001-source-mounted-factory-components.md` for the distribution
decision and `CONTEXT.md` for the vocabulary.

## Layout

- `std/` — the stdlib: repo-agnostic machinery that knows nothing about
  any consumer repo.
  - `predicate.luau` — typed boolean judge (asks an agent whether a
    predicate holds for a payload; bounded retry, exhaustion is a script
    error).
  - `gh.luau` — GitHub CLI transport over `ptah.exec` with structured
    outcomes (never raises for a failed command) and POSIX-safe argument
    quoting.
  - `converge.luau` — the convergence loop: prompt → judge → fix, with
    first-class human escalation and an iteration cap.
  - `daemon.luau` — repo loop skeleton: apply a per-repo operation with
    per-repo error isolation, sequential or bounded-concurrency parallel.
- `components/<name>/` — one directory per component: `component.luau`
  is the facade module exposing `new(config) -> instance`, and the
  sibling `README.md` declares the component's environment
  requirements. (The module file is `component.luau` — not
  `init.luau` — so ptah's require resolver and luau-lsp's agree on the
  module's internal `../../std/…` requires; an `init.luau` module's
  relative requires resolve one directory off under luau-lsp.)
  - `openspec/` — groom, implement, and verify an openspec change.
  - `pr-review-loop/` — review→fix→push convergence against a pull
    request.

## The component contract

- A component is a facade of typed operations: `new(config)` returns an
  instance; instance methods are the operations; per-call data (a change
  name, a PR URL) is a method argument, not config.
- Config is data-only — strings, numbers, booleans. Functions are not
  configuration.
- Every module is `--!strict` and every component exports its `Config`
  type, so a consumer's `ptah check` validates their config against the
  component's config type (the compatibility gate).
- Library modules only require other modules inside this tree, never
  write files inside it, and never depend on a relative working
  directory — so the tree works mounted anywhere (including a read-only
  nix store path).

## Consuming

Mount the tree wherever you like (nix flake input + symlink, submodule,
vendored copy), then write a shim — the only workflow code you own:

```lua
--!strict
local openspec = require("./vendor/factory-components/components/openspec/component")

local ops = openspec.new({
	agent = "claude",
	judgeAgent = "claude",
	model = "claude-opus-4-5",
	judgeModel = "claude-haiku-4-5",
})

ops:groom("add-auth")
```

`require` paths are relative to the requiring file and may traverse
outside the shim's directory, so the mount point is your free choice.
Pin the mount with whatever mechanism mounted it (`flake.lock`, submodule
ref); `ptah check` on your shim is the compatibility gate when you bump.
