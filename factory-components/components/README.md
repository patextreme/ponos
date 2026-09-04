# components

One directory per workflow component: `components/<name>/` holds the
facade module `component.luau` (exposing `new(config) -> instance`;
config is data-only) and the component's README with its declared
environment requirements. The module file is `component.luau` — not
`init.luau` — so ptah's require resolver and luau-lsp's resolve the
module's internal `../../std/…` requires identically. See
`../README.md` for the full contract.

- `openspec/` — groom, implement, verify an openspec change
- `pr-review-loop/` — review→fix→push convergence on a pull request
