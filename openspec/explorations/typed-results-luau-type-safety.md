# Exploration: Can typed results (`result` session option) be made type-safe — generics, TypeBox-style derivation, or runtime type→schema?

**Date:** 2026-08-22
**Status:** Explored — no change proposal yet
**Trigger question:** "The Luau type API has `any` for the result. Could we use Luau generics to define the type of the result? And since type and schema are defined separately, how do we keep them in sync?"

## TL;DR

Three questions, three answers, all verified empirically against `luau-lsp`
1.69.0 (the exact version the nix `ponos-analyze` check runs):

1. **Generics: yes, with one specific design.** A phantom-typed
   `Schema<T>` + generic `Session<T>`/`PromptResult<T>` works: the single
   `Schema<Review>` annotation on the schema table binds `T` through inference,
   and every `r.result` use site is then checked. Other formulations fail
   (details below). Zero runtime changes — pure `.ponos/ponos.d.luau` surface.
2. **TypeBox-style builder (derive type from schema value): impossible in
   Luau.** No `keyof`, no mapped types, builders can't compose types, and
   `typeof(sample)` is shape-only (singletons widened, no optionals). The
   design doc's "no schema-builder DSL" non-goal is provably correct. The
   Luau-native substitute is *codegen by the binary* (`ponos schemagen`,
   schema→type).
3. **Runtime type→schema (deriving schema from Luau types): no.** Types are
   erased at compile-to-bytecode; the mlua-embedded VM has nothing to
   introspect (architectural, not a missing feature). Rust-side source parsing
   (full-moon) could do it, but Luau types cannot express `minimum`/`maximum`/
   `pattern`/`minItems` — lossy on exactly the constraint clauses that make
   runtime validation worth having. Schema must stay the source of truth.

Recommended pickup order: ship the phantom-generic defs (small, separable,
could fold into `add-typed-agent-results` before archive or ride as a
follow-up); `ponos schemagen` and `ponos.validate` are independent follow-ups.

## Context

`add-typed-agent-results` (implemented, not yet archived) gives
`agent:session({ result = <JSON Schema table> })` a typed-result contract:
`session:prompt()` outcomes carry `result: any?` — the last schema-valid
submission, `nil` when absent. Everything type-checks, nothing about the
submission's *shape* is checked: scripts consume `r.result.verdict` as `any`.

Current definitions (`.ponos/ponos.d.luau`) type the seam as:

```luau
export type PromptResult = { …, result: any? }
export type SessionOptions = { …, result: { [string]: any }? }
```

Experiment artifacts: `.work/generics-exp/` (gitignored; decisive snippets
inlined below, reproduction commands at the end).

## Findings

### 1. Generics work — but only one formulation avoids the traps

Every candidate was tested with positive *and* negative probes (a candidate
only counts as "checked" if bad field access and `number`→`string` misuse are
actually caught):

| Pattern | Verdict | Evidence |
|---|---|---|
| `local s: Session<Review> = agent:session(…)` (generic factory) | ✅ works, catches bad fields | `Key 'nonexistent' not found in table 'Review'` |
| …but unannotated session | ❌ poisons | `r.result` is `'a?` (unbound generic) — *any* use errors: `Expected 'string', but got ''a?'` |
| Explicit generic call `agent:session<Review>(…)` or `agent.session<Review>(…)` | ❌ not Luau syntax | parses as chained `<` comparisons — `SyntaxError` |
| Non-generic factory returning `Session<any>` | ✅ works; annotation pins T honestly; plain sessions get `any?` (permissive, not poisoned) | negative probe still caught |
| Making `SessionOptions` itself generic | ❌ breaks `local opts: SessionOptions = …` | `Type parameter list is required` |
| **Phantom `Schema<T>` + intersection param** | ✅ **winning design** | inference binds T; bare `SessionOptions` untouched; inline schemas degrade gracefully |

The winning shape (call it **defsD**):

```luau
-- Phantom-typed JSON Schema handle. `_phantom` is a type-level-only field:
-- never present at runtime, exists solely to carry T through unification.
export type Schema<T> = {
	[string]: any,
	_phantom: T?,
}

export type PromptResult<T> = {
	text: string,
	stop_reason: string,
	usage: Usage,
	result: T?,
}

-- SessionOptions stays NON-generic; the intersection adds the phantom only
-- where it flows through inference.
export type Agent = {
	session: <T>(self: Agent, opts: (SessionOptions & { result: Schema<T>? })?) -> Session<T>,
}

export type Session<T> = {
	prompt: (self: Session<T>, text: string, opts: PromptOptions?) -> PromptResult<T>,
	cancel: (self: Session<T>) -> (),
	label: (self: Session<T>) -> string,
	close: (self: Session<T>) -> (),
}
```

Why each piece is shaped the way it is:

- **Intersection param instead of generic `SessionOptions<T>`** — keeps the
  exported alias non-generic so existing `local opts: SessionOptions = …`
  annotations keep working (generic alias would require `SessionOptions<…>`
  everywhere: `Type parameter list is required`).
- **`Schema<T>` has `[string]: any`** — the actual schema table (any JSON
  Schema shape) still typechecks against it; the phantom `_phantom: T?` never
  conflicts with real fields.
- **When `result` is omitted or an unannotated inline table** — T stays
  unsolved and degrades to a *permissive* free type: `r.result.anything`
  analyzes clean (same ergonomics as today's `any?`), never the poisoned
  `'a?`. Verified with a dedicated probe.

Usage — the annotation lives once, in the schema module, next to the schema:

```luau
-- typed_results_schema.luau
export type Review = { verdict: "approve" | "block", score: number }
local review: Schema<Review> = {
	type = "object",
	properties = {
		verdict = { type = "string", enum = { "approve", "block" } },
		score = { type = "integer", minimum = 0, maximum = 10 },
	},
	required = { "verdict", "score" },
}
return { review = review }

-- script — no annotation needed here; T inferred from schema.review
local s = agent:session({ id = "reviewer", result = schema.review })
local r = s:prompt("review the diff")
if r.result ~= nil then
	ponos.log(("verdict=%s score=%d"):format(r.result.verdict, r.result.score))
end
```

Verified properties of defsD:

- ✅ inference binds `T = Review` for real (not permissively): `local bad: string = r.result.score` → `Expected 'string', but got 'number'`
- ✅ unknown fields caught: `Key 'nonexistent' not found in table 'Review'`
- ✅ enum-literal precision works: `{ verdict: "approve" | "block", … }` narrows `r.result.verdict` to the union
- ✅ bare `SessionOptions` annotations unaffected
- ✅ unannotated inline schema (`result = { type = "object" }`) stays permissive
- ✅ **all existing strict-mode `examples/*.luau` + `tests/fixtures/*.luau` analyze clean under defsD** (backward compatible)
- ✅ escape hatch for inline schemas: `local s: Session<Review> = agent:session({ result = { … } })` — annotation is honest, catches bad fields

Runtime impact: **none**. The d.luau is an editor/`ponos-analyze` contract
only; `src/script/` returns plain tables regardless (phantom field never
exists). Adoption means: update `.ponos/ponos.d.luau`, extend
`tests/fixtures/types_probe.luau` (a result session end-to-end), extend the
example, update README's type docs. All already in the change's task list
shapes.

### 2. Keeping type and schema in sync (the two-artifact problem)

Generics check the *seam* (session → prompt → use sites). They cannot check
schema-contents ↔ type-shape equivalence: Luau has no dependent types; the
schema is a runtime *value*; nothing ever compares the two. Layers, weakest →
strongest:

1. **Co-location** — the `Schema<Review>` claim sits on the schema table
   itself; drift must happen within a few lines of its counterparty. Cheap,
   unenforced.
2. **Typed witness + one runtime check** — keep a sample in the schema module,
   validate once at startup:

   ```luau
   local sample: Review = { verdict = "approve", score = 7 } -- compiler ↔ Review
   assert(ponos.validate(sample, review))                    -- jsonschema ↔ schema
   ```

   The chain `Review ←(compiler) sample (jsonschema)→ schema` catches the
   common drifts (renamed/retyped fields, requiredness) at script start.
   Honest limits: smoke test, not proof — optional fields, unexercised enum
   members, schema-extra properties can still drift. Needs a small new
   `ponos.validate(value, schema) -> boolean | nil, string` global
   (`jsonschema` + Luau↔JSON conversion already in-tree; ~20 lines).
3. **Codegen** (the full TypeBox guarantee) — see finding 4.

### 3. TypeBox-style builder DSL: impossible in this Luau

TypeBox's core is `Static<typeof T>` — deriving a type from a value via TS
type-level computation. Probed each capability:

| Capability | Result |
|---|---|
| `keyof T` | `Unknown type 'keyof'` |
| Mapped type `{ [K in keyof T]: U }` | `SyntaxError` |
| Builder composition `object({ verdict = Schema<A>, score = Schema<B> })` | T unbound → `{ [string]: 'a }` poison; heterogeneous fields can't reconstruct `{ verdict: A, score: B }` |
| `typeof(sample)` value→type | shape-only (next finding) |

And `typeof(sample)` — Luau's one derivation trick — is unusable for
contracts, verified:

```luau
local sample = { verdict = "approve", score = 7 }
export type Review = typeof(sample)
local x: Review = { verdict = "block", score = 1 } -- ACCEPTED: singleton "approve" widened to string
local sample2 = { score = nil }
local y: typeof(sample2) = { score = 7 } -- TypeError: 'nil' vs 'number' — no optional fields
```

No singletons, no optionals — exactly the two things that make a schema a
*contract* rather than a shape. (Also rules out "derive the type from an
example submission" as a substitute.) Conclusion: a userland builder module
can validate/construct the runtime schema but can never type its result —
`add-typed-agent-results` design's "no builder DSL" non-goal is correct, and
provably so.

### 4. The Luau-native substitute: generate types with the binary, not the type system

Since the type system can't compute, let ponos do it — house precedent exists
(`ponos types` emits definitions from an embedded file; sync is guarded by
tests). Proposed subcommand:

```
ponos schemagen examples/typed_results_schema.luau > gen/review_types.luau
```

evaluates the schema module, walks each exported schema, emits:

```luau
-- generated by ponos schemagen — do not edit
export type Review = {
	verdict: "approve" | "block",
	score: number,
}
```

Mapping is mechanical over the subset `jsonschema` validation supports:
`enum`→literal unions, absent-from-`required`→`?`, `anyOf`→union,
`array/items`→`{T}`, `integer`/`number`→`number`. The schema module closes the
loop: `local review: Schema<t.Review> = { … }` with `local t =
require("./gen/review_types")`.

Drift protection becomes mechanical — CI gates: (a) regen + diff fails if
`gen/` is stale after a schema edit; (b) `ponos-analyze` fails at every
`r.result` use site if a regenerated type invalidates consumer code. That's
the full TypeBox guarantee — single source of truth, both artifacts, no manual
sync — via codegen.

### 5. Runtime type→schema (reverse direction): architecturally closed

Asked as "can the embedded runtime generate a schema from a Luau type?"

- **Types are erased.** Annotations are dropped when source compiles to
  bytecode — to the VM they're comments. The `lua_State` mlua binds exposes
  only runtime kind checks (`value.type_name()` → `nil|boolean|number|string|
  table|function|…`), same seven kinds as Luau's `typeof()`. A `Review` table
  and a `Usage` table are indistinguishable at runtime. Not an mlua gap; it's
  Luau's gradual-typing design.
- **"Embedding the typechecker"** would mean FFI to Luau's C++ Frontend/
  Analysis library — that is what luau-lsp *is*. Unstable API surface,
  LSP-grade C++ FFI inside an orchestrator binary: a non-starter for ponos.
- **Rust-side parsing is feasible but wrong-direction.** ponos owns source
  text before mlua sees it (`src/script/mod.rs` and `require.rs` both
  `read_to_string` first), and `full-moon 2.2.0` (pure Rust, lossless, what
  StyLua uses) parses Luau type annotations — so `ponos schemagen
  --from-types` *could* emit schemas from declared types. But Luau types
  cannot express `minimum`/`maximum`/`pattern`/`minLength`/`minItems`/
  `multipleOf`/`format` — the derivation is lossy on exactly the constraint
  clauses that make runtime validation worth having. Recovering them needs a
  side-table of dropped constraints, which reintroduces dual declaration.

Direction comparison:

| Route | Feasible? | Lossless? |
|---|---|---|
| Runtime introspection (mlua/VM) | ❌ types erased — architectural | — |
| Embed Luau C++ typechecker | ⚠️ in principle (luau-lsp is that) | ✅ but not practical |
| Rust-side parse via full-moon (type→schema) | ✅ | ❌ drops `minimum`/`maximum`/`pattern`/… |
| Schema→type walker (finding 4) | ✅ deps already in-tree | ✅ for everything Luau can express |

**The schema is the strictly richer contract and must stay the source of
truth; schema→type is the only lossless direction.** (full-moon remains worth
remembering for other tool-time uses — `ponos fmt`, script linting.)

## The gap

`r.result: any?` ships with `add-typed-agent-results`; everything above is an
upgrade path sitting ready: the defsD shape is verified backward-compatible,
so the type-safety improvement can land whenever without touching runtime
code. The sync problem has a cheap layer (witness sample) and a complete one
(`schemagen`), but both are unstarted.

## Open questions (for a future change)

1. **Fold or follow-up?** Fold the phantom-generic defs into
   `add-typed-agent-results` before archive (it touches the same files:
   d.luau, probe fixture, example, README), or ship as its own small change
   (`typed-results-generics`)? Inclination: fold — it's the same surface and
   the change isn't archived yet.
2. **`ponos.validate` surface** — global `(value: any, schema: Schema<any>) -> boolean | nil, string`?
   Does the witness-sample pattern deserve README documentation as *the*
   recommended pattern, making `validate` near-mandatory?
3. **`schemagen` mechanics** — which schema subset for v1 (enum/required/
   properties/type/anyOf/array probably suffice)? Output layout (`gen/`
   sibling to the schema module)? Regenerate check as a cargo test (like the
   `ponos types` sync guard) or nix check? Should `Schema<T>` values carry
   their module path so `schemagen` can find *all* schemas without flags?
4. **Probe coverage** — `tests/fixtures/types_probe.luau` + the nix
   `ponos-analyze` check would need negative cases (a deliberate type error
   asserting analysis *fails*)? Today's gate only asserts clean passes; the
   defsD evidence depends on negative probes, which the gate can't express as
   written. Maybe a fixture pair in `tests/` shelling out to luau-lsp.
5. **Phantom name stability** — `_phantom` must never collide with a real
   JSON Schema keyword; current keywords don't start with `_` (JSON Schema
   reserves `$`-prefixed). Document the reservation in the d.luau comment.

## Pointers (verified at time of writing)

- `.ponos/ponos.d.luau` — current `result: any?` / `{ [string]: any }?` seam (the thing to upgrade)
- `openspec/changes/add-typed-agent-results/` — proposal/design/tasks; design.md non-goals ("no schema-builder DSL") now empirically justified
- `nix/checks.nix:73-95` — `ponos-analyze`: `luau-lsp analyze --platform=standard --definitions=.ponos/ponos.d.luau examples/*.luau tests/fixtures/*.luau` (nixpkgs luau-lsp 1.69.0; same binary at `~/.nix-profile/bin/luau-lsp`)
- `tests/types.rs` + `tests/fixtures/types_probe.luau` — runtime probe pattern a defs update must extend
- `src/cli.rs:53` — `include_str!`d definitions, `ponos types` sync precedent for codegen-artifact guarding
- `src/script/mod.rs:574`, `src/script/require.rs:155` — source-text ownership boundary (where Rust-side parsing would hook)
- Cargo.toml:22 — `mlua 0.12` (`luau`, `async`, `serialize`); runtime introspection surface
- crates.io — `full-moon 2.2.0` (pure-Rust lossless Luau parser; not currently a dependency)
- `.work/generics-exp/` — experiment scripts (gitignored, ephemeral): `defsA`–`defsD` (candidate definition files), `a1`–`a7` (generic formulations), `b1`–`b6` (phantom/inference), `c1`–`c4` (TypeBox capabilities), `d_probe` (defsD decisive probe). Reproduce with:
  `luau-lsp analyze --platform=standard --definitions=defsD.luau <file>.luau`
