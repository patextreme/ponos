## Context

The escape rule lives in two independent implementations that must stay in
lockstep (a spec-level invariant: check and pre-flight findings must match
runtime rejections):

- runtime navigator — `src/script/require.rs` (`ScriptRequirer::guard`,
  called from `reset`/`to_parent`/`to_child`; `escapes` helper)
- static walk — `src/check/lint.rs` (`resolve_edge`, with duplicated
  `normalize`/`escapes`/`resolve_file` helpers)

`jump_to_alias` rejects every non-relative require string (absolute paths,
bare names, aliases) with a "only `./` and `../` paths are allowed" error;
that behavior is retained unchanged.

## Goals / Non-Goals

**Goals:**

- Delete the script-tree boundary from both implementations in one change.
- Keep check/pre-flight findings and runtime errors agreeing at every step.

**Non-Goals:**

- Absolute-path requires, alias maps, portability warnings (rejected in the
  grilling session — see proposal "What Changes / Not doing").
- Symlink/realpath canonicalization. The guard was lexical; with no
  boundary there is nothing to escape through a symlink, and canonicalizing
  would add filesystem work to the hot require path for no gain.
- Bounding the require graph. The lint walk already terminates via its
  visited set (each file once); depth caps would be complexity without a
  failure mode to prevent.

## Decisions

- **Delete `guard`/`escapes` from both sites; change nothing else in the
  resolution rules.** The navigator keeps lexical `normalize` (still needed
  so `a/../b` resolves), the `..`-past-filesystem-root `NotFound` from
  `pop()`, and the module-not-found error in `to_child`. Pure widening:
  every previously valid script resolves identically.
- **No replacement concept.** We considered anchoring the boundary at the
  nearest `.ponos/` project root instead of deleting it; rejected because
  the boundary buys nothing security-wise (required modules share the
  sandboxed globals; scripts drive agents with full user authority) and
  luau-lsp never enforced it anyway. The "self-contained script tree" unit
  of distribution is retired; self-containment becomes author convention.
- **Docs state the trusted-code contract, agent-scoped wording only.** One
  sentence in README (near the require/sandbox description) and the skill
  doc: scripts are trusted, the sandbox limits the blast radius of bugs,
  not malice. Does not enumerate `require` as a vector (user decision).
- **New example mirrors the motivating layout** (`examples/workflow-1/`,
  `examples/workflow-2/`, `examples/shared/`), because the layout itself is
  the documentation; the existing examples-test harness pins it offline.
- **Obsolete scenarios are modified in place, not dropped.** OpenSpec
  deltas cannot delete a scenario from a surviving requirement (MODIFIED
  must carry every current scenario; REMOVED works only on whole
  requirements). So the two escape-flavored scenarios keep their names and
  flip their WHEN/THEN bodies to assert the new behavior (escape that
  resolves → no finding; residuals list → no require-tree entry). The
  names still describe their WHEN conditions honestly.

## Risks / Trade-offs

- [Runtime and lint drift if edited out of sync] → Both changes land in the
  same tasks; the new cross-tree tests exist for both runtime (require
  succeeds) and lint (no finding) so a one-sided edit fails CI.
- [Users lose the "zip the script dir" mental model] → Accepted
  deliberately; README drops the sentence rather than inventing a
  replacement rule.
- [`../` walks can reach unrelated trees (e.g. `~` via enough `..`)]
  → Accepted under the trusted-code contract; identical exposure existed
  for any script whose tree sat near sensitive files.

## Migration Plan

Pure widening — no migration. Rollback is reverting the commit; no state,
config, or on-disk format is involved.
