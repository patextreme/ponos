## 0. Reconciliation gate (do first)

- [x] 0.1 Read ②'s archived tasks.md implementation-notes/deviations and the landed workspace layout (`ls crates/`); if anything diverged from ②'s design D1–D4 (crate names, file placement, facade shape), update this change's design.md and the paths/allowlist below via the update-change workflow before touching code; verify `openspec validate hexagonal-polish` passes after any edit

## 1. Dependency-direction guard test

- [x] 1.1 Add `crates/ponos-core/tests/deps_guard.rs` implementing design D1: self-scan of `CARGO_MANIFEST_DIR/src/**.rs` with the forbidden-import list and the pinned allowlist (`tokio::sync`, `tokio::task::spawn_local` in task.rs only, mlua in task.rs only, `agent_client_protocol` schema types in turn/session/ports/events only, `std::env` reads, `std::sync`); verify it **passes** on the landed core and **fails** on a planted `tokio::fs` import (plant, run, revert)
- [x] 1.2 Verify the guard runs inside the offline sandbox: `nix flake check` executes it via the workspace test run (no network, no new toolchain deps in the flake)

## 2. Workspace lint policy

- [x] 2.1 Inventory current clippy output on the pinned nightly (`cargo clippy --workspace -- -D warnings` baseline), pick the mechanical pass set, promote them in `[workspace.lints]`; verify `cargo clippy --workspace -- -D warnings` green with zero new `#[allow]` attributes (or recorded non-adoptions in design D2)
- [x] 2.2 One-shot trial of `unused_crate_dependencies` on the pin; if clean and offline-safe, promote to `[workspace.lints]`; otherwise record non-adoption reason in design D2 and drop it; verify `cargo clippy --workspace -- -D warnings` still green either way

## 3. Docs

- [x] 3.1 Rewrite AGENTS.md's `## Architecture` section per design D3: workspace crate map (composition root + facade, six adapters, core), the four funded ports with the closed-set rule (new ports = their own change), TUI-readiness rationale; also refresh stale paths elsewhere in AGENTS.md (Testing section's `src/bin/mock-agent/` → `crates/ponos-cli/src/bin/mock-agent/`); verify every referenced path exists on disk (`test -e` for each path mentioned)
- [x] 3.2 Refresh stale paths in README.md and `skills/ponos/SKILL.md` (`src/bin/mock-agent/` and any other root-relative `src/…` references → `crates/…`), leaving illustrative output byte-identical (the README "Output format" sample block is test-pinned by `crates/ponos-cli/tests/cli.rs`; SKILL.md code-sample payload strings are fictional); verify `git grep -nE '(^|[^/])src/(task\.rs|config\.rs|acp|script|render|check|cli\.rs|bin/mock-agent)' -- '*.md' ':!openspec'` returns only that README sample-log line

## 4. Straggler hygiene + gate

- [x] 4.1 Sweep for ①/② leftovers: `git grep -n 'crate::result_wire\|crate::acp\|crate::script\|crate::render\|crate::check\|crate::config_fs' -- '*.rs' ':!crates/ponos-cli'` (should be empty; the same facade paths inside `crates/ponos-cli` are live composition-root usage per design D4), dead modules/files under `crates/` (cargo warnings clean), and confirm the `ponos` facade re-exports in `crates/ponos-cli/src/lib.rs` are untouched per the Q4 carve-out; delete only true stragglers
- [x] 4.2 Full gate: `cargo test` (all members), `cargo clippy --workspace -- -D warnings`, `nix flake check` green; `git diff examples/` empty (payload strings carved out in design D4); `git diff --stat` touches only `crates/ponos-core/tests/deps_guard.rs`, manifests (lint attrs), AGENTS.md, README.md, skills/ponos/SKILL.md, and deleted stragglers
