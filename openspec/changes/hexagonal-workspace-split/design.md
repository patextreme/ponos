## Context

Change ① (`hexagonal-internal-restructure`) landed the module tree that this
change physicalizes: `core` (pure domain + ports), five adapter modules
(`acp`, `render`, `check`, `script`, `config_fs`) importing core only, plus
`bridge`/`result_wire` (typed-results I/O) and `cli` (composition). Its
recorded deviations leave exactly three surviving cross-module arrows:

1. `script/state.rs::default_transport()` → `crate::acp::Transport` — one
   composition line, reserved by ① for this change to move into `cli`.
2. `acp/driver.rs` → `result_wire` (`bind_result_socket`,
   `spawn_result_channel`): owns the per-session result channel.
3. `bridge.rs` → `result_wire` (`connect`, `submit_over_socket`): the client
   half of the same newline-JSON UDS protocol.

Constraints inherited from the design session and ①: pinned nightly via
nix dev shell (no system rustc); fully offline test suite; the four funded
ports are a closed set; no publishability ceremony for lib crates; 129
integration tests construct `SessionOptions`/`RunConfig` as struct literals
and call `run`/`setup_lua`/`start_session` with fixed arity.

Facts that corrected the exploration draft: `config_fs.rs` is 155 lines with
`cli.rs` as its only consumer; core carries data-level mlua (`task`),
`agent_client_protocol` schema types (`turn`, `session`, `ports`), and
`tokio::sync` — all settled by ①'s verbatim-move tasks; core is I/O-free
(TOML parse/fs discovery can never fold into it).

## Goals / Non-Goals

**Goals:**

- Every ① module boundary becomes a crate boundary; the dependency direction
  (adapter → core, cli → everything) is enforced by the compiler, with no
  documented exceptions.
- Integration tests and examples keep testing the same thing through the
  same API (`ponos::*` facade), unchanged except where a constructor
  literally cannot stay valid (the `RunConfig` transport field).
- Workspace-level dependency and lint inheritance so member crates don't
  each carry their own version pins.
- `nix flake check` stays green with per-crate crane source filtering,
  keeping `examples/` in the build source.

**Non-Goals:**

- No behavior, API-surface, output-byte, or exit-code changes beyond the
  `RunConfig` field addition.
- No dependency-direction *lints* or docs — that is change ③
  (`hexagonal-polish`).
- No publishing, semver discipline, or public API stability for member
  crates beyond the `ponos` facade.
- No re-shuffling of module *contents*: the crate map is ①'s module map;
  files move, code is not redesigned.

## Decisions

### D1 — Crate map (target)

| Crate | Contents (from ①'s tree) | Depends on |
|---|---|---|
| `ponos-core` | `core/`: config model, contract, events, ports, task, text, turn, session, error | serde, serde_json, jsonschema; **data-level**: mlua (`task`), agent-client-protocol (`turn`/`session`/`ports`), tokio::sync |
| `ponos-acp` | `acp/`: process, proto, driver | core, result, agent-client-protocol, async-process, futures, tokio, tracing, libc |
| `ponos-luau` | `script/`: sandbox, bindings, state, run, require | core, mlua, serde, serde_json, agent-client-protocol (schema types, data-level), tokio, futures |
| `ponos-check` | `check.rs`, `check/`: defs, lint; `TYPE_DEFINITIONS` | core, mlua, full-moon |
| `ponos-config` | `config_fs.rs` — TOML parse + discovery, the only `ConfigSource` impl | core, toml |
| `ponos-render` | `render/` | core (jiff joins here from cli's dep set) |
| `ponos-result` | `result_wire.rs` — UDS channel + submit/verdict protocol (both halves) | core, tokio, serde, serde_json, tracing |
| `ponos-cli` | `cli.rs`, `bridge.rs`, `main.rs`, `bin/mock-agent/`, `lib.rs` facade, `tests/` | everything; clap, rmcp, libc, tracing, tracing-subscriber, jiff |

Rationale for the two deviations from the exploration draft:

- **`ponos-result` is its own crate** (Q2a): filing it under `ponos-cli`
  creates `acp → cli → acp` cycle (the driver needs the server half);
  folding it into `ponos-acp` makes the ACP adapter own a non-ACP protocol
  and drags `ponos-acp` into `bridge.rs`'s dep set; splitting the module
  duplicates the wire types across a boundary. One coherent protocol, one
  small crate, both consumers import it — the only cycle-free mechanical
  option.
- **`ponos-config` stays a crate** (Q3b): 155 lines today, but it preserves
  the one-adapter-one-crate symmetry with `render`/`check` and is the
  natural home if config discovery grows (XDG, layering). Folding it into
  `ponos-cli` is the accepted fallback, recorded here so ③ can revisit
  without a new design session.

Alternatives considered: single crate with `mod` visibility enforcement
(status quo — arrows stay grep-audited, rejected); cargo feature-gated
optional `ponos-luau → ponos-acp` dep (declared arrow + cfg wart, rejected).

### D2 — Transport injection kills the `script → acp` arrow

`RunConfig` gains `transport: Arc<dyn AgentTransport>` as a required
field — no `Default` impl: it would be orphan-rule-blocked in `ponos-cli`
and arrow-recreating in `ponos-luau`. The `default_transport()`
composition moves from `script/state.rs` into `ponos-cli`; `cli.rs`'s own
`RunConfig` literal is the fourth and last construction site. **Gate
amendment** (Q1a): ②'s original "tests byte-identical" acceptance is
amended to "byte-identical except five mechanical lines: the `transport:`
line at the three `RunConfig` literal sites (`tests/script.rs`,
`tests/e2e.rs`, `tests/acp.rs`), plus the two `env!("CARGO_MANIFEST_DIR")`
joins that re-root to the workspace root — `tests/examples.rs`
(`examples/`) and `tests/cli.rs` (`.ponos/ponos.d.luau`) both need
`../../` because the manifest dir moves to `crates/ponos-cli` while
their targets stay at the repo root (`tests/types.rs`'s target moves
with the tests and is unaffected); zero expectation changes; `examples/`
untouched". The gate
was a proxy for no-behavior-change; constructor-arity and path-re-root
mechanical edits to make an arrow die do not breach it.

Alternatives: keep the arrow as a documented crate-level exception (defeats
the change's purpose; ③ is lints+docs only and cannot absorb it); optional
dependency (exception with extra steps).

### D3 — `ponos-cli` is the composition root *and* permanent facade

`ponos-cli` gets `[lib] name = "ponos"` re-exporting member crates
(`pub use ponos_acp as acp; …`) plus ①'s core compat re-exports
(`pub use ponos_core::config;`, `pub use ponos_core::task;`). This is not
a transitional shim to delete in ③: the facade **is** the binary package's
public surface, and the integration tests exercise the system through it
(`ponos::acp::start_session`, `ponos::script::RunConfig`, `ponos::render`,
`ponos::config::AgentSpec`, `ponos::task::spawn`). ③'s straggler hunt must
not touch it (Q4).

Both `[[bin]]`s stay in `ponos-cli` so `env!("CARGO_BIN_EXE_mock-agent")`
resolves from `ponos-cli`'s own tests; "mock-agent is not part of the CLI
surface" remains a behavioral contract (AGENTS.md), not a packaging one.

### D4 — Workspace mechanics

- Root `Cargo.toml`: `[workspace]` members under `crates/*`, no root
  package; `[workspace.dependencies]` carries every version pin once;
  `[workspace.lints]` with member `[lints] workspace = true` (baseline
  only — policy tightening belongs to ③, which has pre-committed the
  grep-test floor + workspace lints as its enforcement floor).
- Nix/crane: source filtering extended to `crates/**`; `examples/`,
  `skills/`, and test sources stay in the filtered source so
  `tests/examples.rs` and the offline suite pass in the sandbox. The two
  `package.version` reads in nix (`package.nix`, `checks.nix`) repoint to
  `crates/ponos-cli/Cargo.toml` — the root manifest becomes workspace-only
  and loses `[package]`. Cargo
  lockfile regenerated as part of the move; toolchain pin untouched.
- Module moves are `git mv` where possible so history follows; intra-crate
  `crate::` paths become `ponos_core::`-style extern paths mechanically.

## Risks / Trade-offs

- [Crane per-crate filtering drops a source dir a crate needs → sandbox
  check fails late] → keep filtering close to today's proven allowlist,
  add `crates/` roots wholesale rather than per-file exclusion; acceptance
  gate is `nix flake check`, run before declaring done.
- [`RunConfig` field addition breaks a test literal not yet found] →
  verified: exactly three sites (`tests/script.rs`, `tests/e2e.rs`,
  `tests/acp.rs` — the only `RunConfig {` literals outside `src/cli.rs`);
  also verified: the only other move-sensitive test lines are the two
  `env!("CARGO_MANIFEST_DIR")` joins (`tests/examples.rs`,
  `tests/cli.rs`; `tests/types.rs`'s target moves with the tests);
  gate wording pins all five exceptions so review can check it.
- [Workspace split changes lockfile/features (e.g. feature unification
  shifts mlua/rmcp builds) → build breaks offline] → pin feature sets
  per-member exactly as today's single-crate `Cargo.toml` lists them;
  dev shell is the only toolchain, `nix flake check` is the arbiter.
- [Facade drift: `ponos::*` re-exports and member crates diverge] →
  facade is a flat `pub use` list in one `lib.rs`; ③'s docs task covers
  it; no glob re-exports.
- [One more crate than the draft promised (`ponos-result`)] → accepted
  explicitly (Q2a); private member, zero ceremony.

## Migration Plan

Single mechanical sequence, one PR, no rollout: workspace conversion +
moves + facade, then the two-line test edit, then nix update; `cargo test`,
`cargo clippy -- -D warnings`, `nix flake check` green at the end. Rollback
is `git revert` of the move commit; nothing persists outside the repo.

## Open Questions

None blocking. (Whether `ponos-render` should own `jiff` vs. stay
time-format-free is decided by the move itself: `render/mod.rs` is the
consumer; it joins `ponos-render`'s deps.)
