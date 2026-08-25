## 1. Workspace scaffolding

- [ ] 1.1 Convert root `Cargo.toml` to a workspace-only manifest (members `crates/*`, no root package; `[workspace.dependencies]` carrying today's version pins/features verbatim, `[workspace.lints]` baseline); verify `cargo metadata` resolves and no member exists yet besides the old package temporarily kept buildable
- [ ] 1.2 Create `crates/ponos-core` (name, edition, `[lints] workspace = true`, deps from design D1 incl. data-level mlua/agent-client-protocol/tokio::sync) and `git mv src/core/*` into it; convert `crate::` paths to `ponos_core::`; verify `cargo test -p ponos-core` green (inline unit tests move with their modules)

## 2. Adapter crates (moves in dependency order, each green before the next)

- [ ] 2.1 `crates/ponos-result` ← `src/result_wire.rs` (deps: core, tokio, serde, serde_json); re-path imports; verify its inline tests pass via `cargo test -p ponos-result`
- [ ] 2.2 `crates/ponos-render` ← `src/render/` (deps: core, jiff); verify `cargo test -p ponos-render` green
- [ ] 2.3 `crates/ponos-config` ← `src/config_fs.rs` (deps: core, toml); verify `cargo test -p ponos-config` green
- [ ] 2.4 `crates/ponos-acp` ← `src/acp/` (deps: core, ponos-result, agent-client-protocol, async-process, futures); `acp → result_wire` becomes the legal `acp → ponos-result` arrow; verify `cargo test -p ponos-acp` green
- [ ] 2.5 `crates/ponos-luau` ← `src/script/` (deps: core, mlua) with `default_transport()` **removed** and `RunConfig` gaining `transport: Arc<dyn AgentTransport>` (lazy `Default` impl); verify `cargo build -p ponos-luau` and that no `ponos-acp` dep appears in its manifest
- [ ] 2.6 `crates/ponos-check` ← `src/check.rs` + `src/check/` (deps: core, mlua, full-moon); verify `cargo test -p ponos-check` green

## 3. Composition root + facade

- [ ] 3.1 `crates/ponos-cli`: `[lib] name = "ponos"`, both `[[bin]]`s (`ponos`, `mock-agent`), `git mv` of `src/cli.rs`, `src/bridge.rs`, `src/main.rs`, `src/bin/mock-agent/`; facade `lib.rs` with flat `pub use ponos_acp as acp; …` + `pub use ponos_core::{config, task};` compat re-exports; transport composition line (`Arc::new(ponos_acp::Transport)`) lives here now; verify `cargo build -p ponos-cli --bins` produces both binaries
- [ ] 3.2 `git mv tests/` into `crates/ponos-cli/tests/` **unchanged except** adding the mechanical `transport:` line to the two `RunConfig` literals (`tests/script.rs`, `tests/e2e.rs`); verify `git diff -r` against the pre-move tests shows only those two lines and `env!("CARGO_BIN_EXE_mock-agent")` still resolves
- [ ] 3.3 Delete the old single-crate `src/` tree and stale manifest entries; `git grep -l 'src/config.rs\|crate::result_wire\|crate::acp\|crate::script'` returns nothing outside `crates/`; verify workspace `cargo build` from a clean `cargo clean`

## 4. Nix + gates

- [ ] 4.1 Update flake/crane source filtering for `crates/**` while keeping `examples/`, `skills/`, and test sources in the filtered source; verify `nix build` succeeds and `nix flake check` runs the full suite offline
- [ ] 4.2 Full gate: `cargo test` (all members), `cargo clippy --workspace -- -D warnings`, `nix flake check` green; `git diff examples/` empty; `cargo run -p ponos-cli --bin ponos -- --version` prints the same version as before the split
