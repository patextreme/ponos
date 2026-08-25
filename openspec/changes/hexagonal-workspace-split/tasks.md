## 1. Workspace scaffolding

- [ ] 1.1 Convert root `Cargo.toml` to a workspace-only manifest (members `crates/*`, no root package; `[workspace.dependencies]` carrying today's version pins/features verbatim, `[workspace.lints]` baseline); verify `cargo metadata` resolves with no member yet — the old `src/` tree belongs to no package from here until 3.1–3.3 lands and is inert, so mid-sequence verification is per-crate (`cargo test -p`/`build -p`) only
- [ ] 1.2 Create `crates/ponos-core` (name, edition, `[lints] workspace = true`, deps from design D1 incl. data-level mlua/agent-client-protocol/tokio::sync) and `git mv src/core/*` into it; convert `crate::` paths to `ponos_core::`; verify `cargo test -p ponos-core` green (inline unit tests move with their modules)

## 2. Adapter crates (moves in dependency order, each green before the next)

- [ ] 2.1 `crates/ponos-result` ← `src/result_wire.rs` (deps: core, tokio, serde, serde_json, tracing); re-path imports; verify its inline tests pass via `cargo test -p ponos-result`
- [ ] 2.2 `crates/ponos-render` ← `src/render/` (deps: core, jiff); verify `cargo test -p ponos-render` green
- [ ] 2.3 `crates/ponos-config` ← `src/config_fs.rs` (deps: core, toml); verify `cargo test -p ponos-config` green
- [ ] 2.4 `crates/ponos-acp` ← `src/acp/` (deps: core, ponos-result, agent-client-protocol, async-process, futures, tokio, tracing, libc); `acp → result_wire` becomes the legal `acp → ponos-result` arrow; verify `cargo test -p ponos-acp` green
- [ ] 2.5 `crates/ponos-luau` ← `src/script/` (deps: core, mlua, serde, serde_json, agent-client-protocol, tokio, futures) with `default_transport()` **removed** and `RunConfig` gaining a required `transport: Arc<dyn AgentTransport>` field (no `Default` impl — orphan-rule-blocked in `ponos-cli`, arrow-recreating in `ponos-luau`; the composition moves into `cli.rs` at 3.1); verify `cargo build -p ponos-luau` and that no `ponos-acp` dep appears in its manifest
- [ ] 2.6 `crates/ponos-check` ← `src/check.rs` + `src/check/` (deps: core, mlua, full-moon); two mechanical edits the move forces: `check/defs.rs`'s `include_str!("../../.ponos/ponos.d.luau")` gains one more `../` (the embedded defs stay at the repo root), and `summary_line`/`TYPE_DEFINITIONS` promote `pub(crate)` → `pub` (`cli.rs` consumes them across the new crate boundary); verify `cargo test -p ponos-check` green

## 3. Composition root + facade

- [ ] 3.1 `crates/ponos-cli`: `[lib] name = "ponos"`, both `[[bin]]`s (`ponos`, `mock-agent`), `git mv` of `src/cli.rs`, `src/bridge.rs`, `src/main.rs`, `src/bin/mock-agent/`; facade `lib.rs` with flat `pub use ponos_acp as acp; …` + `pub use ponos_core::{config, task};` compat re-exports; transport composition line (`Arc::new(ponos_acp::Transport)`) lives here now; verify `cargo build -p ponos-cli --bins` produces both binaries
- [ ] 3.2 `git mv tests/` into `crates/ponos-cli/tests/` **unchanged except** five mechanical edits: the `transport:` line added to the three `RunConfig` literals (`tests/script.rs`, `tests/e2e.rs`, `tests/acp.rs`), and the two `env!("CARGO_MANIFEST_DIR")` joins re-rooted to the workspace root (`tests/examples.rs`: `.join("../../examples")`, `tests/cli.rs`: `.join("../../.ponos/ponos.d.luau")`); verify `git diff -M <pre-move-rev> -- tests/ crates/ponos-cli/tests/` shows only those five lines and `env!("CARGO_BIN_EXE_mock-agent")` still resolves
- [ ] 3.3 Delete the old single-crate `src/` tree and stale manifest entries; `git grep -l 'crate::result_wire\|crate::acp\|crate::script' -- '*.rs' ':!crates/*'` returns nothing (Rust code only: the stale `src/config.rs` doc path in AGENTS.md and the path strings inside `examples/sequential_review.luau` are ③'s docs/straggler scope — `examples/` stays byte-identical per 4.2); verify workspace `cargo build` from a clean `cargo clean`

## 4. Nix + gates

- [ ] 4.1 Update flake/crane source filtering for `crates/**` while keeping `examples/`, `skills/`, and test sources in the filtered source; repoint the two `(fromTOML (readFile ../Cargo.toml)).package.version` reads (`nix/package.nix:14`, `nix/checks.nix:18`) to `crates/ponos-cli/Cargo.toml` (root manifest loses its `[package]` in 1.1); verify `nix build` succeeds and `nix flake check` runs the full suite offline
- [ ] 4.2 Full gate: `cargo test` (all members), `cargo clippy --workspace -- -D warnings`, `nix flake check` green; `git diff examples/` empty; `cargo run -p ponos-cli --bin ponos -- --version` prints the same version as before the split
