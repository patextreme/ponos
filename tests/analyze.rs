//! Real-analyzer contract tests for the type definitions — the static
//! counterpart to the runtime probe in `tests/types.rs`. The rest of the
//! suite stubs `luau-lsp` (see `tests/check.rs`) to stay hermetic; these
//! tests instead drive `ponos check` with the *real* analyzer, so the
//! binary's embedded definitions are what's under test, and assert the
//! type-definitions capability's scenarios from both sides: strict
//! scripts on the current surface analyze clean, and each documented
//! misuse reports a diagnostic naming the promised type.
//!
//! Gated on `luau-lsp` being on PATH — always true in the nix dev shell
//! and in the sandbox, where `checks.ponos-tests` injects the binary and
//! sets `PONOS_REQUIRE_REAL_LSP=1` so a silent skip cannot pass CI.
//! Plain `cargo test` elsewhere skips with a notice. Fully offline.

use std::path::{Path, PathBuf};
use std::process::Command;

fn ponos_bin() -> &'static str {
    env!("CARGO_BIN_EXE_ponos")
}

fn mock_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mock-agent")
}

/// A temp project with a `.ponos/config.toml` defining one agent, `mock`.
/// HOME is pinned into the project dir so a developer's user registry
/// cannot leak into the checks.
struct Project {
    dir: PathBuf,
}

impl Project {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ponos-analyze-{}-{name}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ponos")).unwrap();
        std::fs::write(
            dir.join(".ponos").join("config.toml"),
            format!("[agents.mock]\ncommand = \"{}\"\nargs = []\n", mock_bin()),
        )
        .unwrap();
        Self { dir }
    }

    fn write(&self, body: &str) -> PathBuf {
        let path = self.dir.join("main.luau");
        std::fs::write(&path, body).unwrap();
        path
    }

    /// Run `ponos check <script>` with `path` as the child's entire PATH
    /// (the real luau-lsp's directory — ponos discovers the analyzer by
    /// PATH, exactly like the stub tests in check.rs).
    fn check(&self, script: &Path, path: &Path) -> (i32, String, String) {
        let output = Command::new(ponos_bin())
            .arg("check")
            .arg(script)
            .current_dir(&self.dir)
            .env("PATH", path)
            .env("HOME", &self.dir)
            .env_remove("XDG_CONFIG_HOME")
            .output()
            .expect("run ponos check");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }
}

/// which-style scan for an executable `luau-lsp` on PATH (the same rule
/// ponos itself uses to find the analyzer).
#[cfg(unix)]
fn luau_lsp_on_path() -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join("luau-lsp");
        std::fs::metadata(&candidate)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
            .then_some(candidate)
    })
}

#[test]
#[cfg(unix)]
fn real_luau_lsp_definitions_contract() {
    let Some(lsp) = luau_lsp_on_path() else {
        if std::env::var_os("PONOS_REQUIRE_REAL_LSP").is_some() {
            panic!("PONOS_REQUIRE_REAL_LSP is set but luau-lsp is not on PATH");
        }
        eprintln!("skipping: luau-lsp not on PATH (run inside `nix develop`)");
        return;
    };
    let lsp_dir = lsp.parent().expect("luau-lsp path has a parent");

    // type-definitions "Constructor config type-checks" (rejecting
    // side): a non-string-or-boolean config entry value reports a type
    // error naming the option-table type.
    let p = Project::new("bad-config");
    let script = p.write(
        "--!strict\n\
         local agent = ponos.agent(\"mock\")\n\
         local s = agent:session({ config = { model = 42 } })\n",
    );
    let (code, stdout, stderr) = p.check(&script, lsp_dir);
    assert_eq!(code, 1, "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stdout.is_empty(), "stdout: {stdout}");
    assert!(
        stderr.contains("SessionOptions"),
        "diagnostic must name the option-table type, stderr:\n{stderr}"
    );

    // type-definitions "Wrong setConfig value type".
    let p = Project::new("bad-setconfig");
    let script = p.write(
        "--!strict\n\
         local agent = ponos.agent(\"mock\")\n\
         local s = agent:session()\n\
         s:setConfig(\"model\", 42)\n",
    );
    let (code, _stdout, stderr) = p.check(&script, lsp_dir);
    assert_eq!(code, 1, "stderr:\n{stderr}");
    assert!(
        stderr.contains("boolean | string"),
        "diagnostic must name the accepted value types, stderr:\n{stderr}"
    );

    // type-definitions "Typo in result field" / "Typed-result surface
    // type-checks" (rejecting side): an invented prompt-outcome field
    // reports a type error naming the result table type.
    let p = Project::new("bad-field");
    let script = p.write(
        "--!strict\n\
         local agent = ponos.agent(\"mock\")\n\
         local s = agent:session()\n\
         local r = s:prompt(\"hi\")\n\
         print(r.txt)\n",
    );
    let (code, _stdout, stderr) = p.check(&script, lsp_dir);
    assert_eq!(code, 1, "stderr:\n{stderr}");
    assert!(
        stderr.contains("Key 'txt' not found in table 'PromptResult'"),
        "diagnostic must name the result table type, stderr:\n{stderr}"
    );

    // Accepting sides of "Constructor config type-checks" and
    // "Typed-result surface type-checks", plus "Outcome narrowing": one
    // strict script using constructor `config` + `resultSchema` +
    // `r.result` and branching on a locally-bound parallel outcome
    // analyzes with zero type errors. (The local binding is
    // load-bearing: narrowing does not apply through repeated index
    // expressions like `outcomes[1].ok` — the spec words the scenario
    // as binding the result to a local for exactly this reason.)
    let p = Project::new("happy");
    let script = p.write(
        "--!strict\n\
         local agent = ponos.agent(\"mock\")\n\
         local s = agent:session({\n\
         \tresultSchema = { type = \"object\" },\n\
         \tconfig = { model = \"opus\" },\n\
         })\n\
         local r = s:prompt(\"hi\")\n\
         print(r.result)\n\
         local outcomes = ponos.parallel({ 1, 2 }, function(item)\n\
         \treturn item * 2\n\
         end)\n\
         local entry = outcomes[1]\n\
         if entry.ok then\n\
         \tprint(entry.value)\n\
         else\n\
         \tprint(entry.error)\n\
         end\n\
         s:close()\n",
    );
    let (code, stdout, stderr) = p.check(&script, lsp_dir);
    assert_eq!(code, 0, "stdout:\n{stdout}\nstderr:\n{stderr}");
    assert!(stdout.is_empty(), "stdout: {stdout}");
    assert!(
        !stderr.contains("TypeError"),
        "happy-path script must analyze clean, stderr:\n{stderr}"
    );
}
