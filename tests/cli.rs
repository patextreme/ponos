//! CLI + renderer integration tests: run the real `ponos` binary against
//! the mock agent via a project registry and assert on captured output.

use std::path::PathBuf;
use std::process::Command;

fn ponos_bin() -> &'static str {
    env!("CARGO_BIN_EXE_ponos")
}

fn mock_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mock-agent")
}

struct Project {
    dir: PathBuf,
}

impl Project {
    fn new(name: &str, agent_env: &[(&str, &str)]) -> Self {
        let dir = std::env::temp_dir().join(format!("ponos-cli-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ponos")).unwrap();

        let mut config = format!("[agents.mock]\ncommand = \"{}\"\nargs = []\n", mock_bin());
        if !agent_env.is_empty() {
            config.push_str("\n[agents.mock.env]\n");
            for (k, v) in agent_env {
                config.push_str(&format!("{k} = \"{v}\"\n"));
            }
        }
        std::fs::write(dir.join(".ponos").join("config.toml"), config).unwrap();
        Self { dir }
    }

    fn script(&self, body: &str) -> PathBuf {
        let path = self.dir.join("main.luau");
        std::fs::write(&path, body).unwrap();
        path
    }

    fn run(&self, script: &PathBuf, flags: &[&str]) -> (i32, String, String) {
        let output = Command::new(ponos_bin())
            .arg("run")
            .arg(script)
            .args(flags)
            .current_dir(&self.dir)
            .env_remove("PONOS_TEST")
            .output()
            .expect("run ponos");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }
}

#[test]
fn version_flag() {
    let out = Command::new(ponos_bin()).arg("--version").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.trim().is_empty(), "{stdout}");
}

#[test]
fn types_prints_version_header_and_definitions() {
    // `ponos types` must emit a one-line version header followed by the
    // repo definitions byte-for-byte (spec: suitable for redirection).
    let out = Command::new(ponos_bin()).arg("types").output().unwrap();
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let (header, body) = stdout
        .split_once('\n')
        .unwrap_or_else(|| panic!("no header line: {stdout:?}"));
    assert_eq!(
        header,
        format!("-- ponos {} type definitions", env!("CARGO_PKG_VERSION"))
    );
    let repo_defs = std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("types/ponos.d.luau"),
    )
    .unwrap();
    assert_eq!(body, repo_defs, "emitted defs must be byte-identical");
}

#[test]
fn types_needs_no_registry_or_agents() {
    // No script, no registry (empty HOME and cwd), no agent spawned: still
    // succeeds. A spawned agent would need a registry entry to come from.
    let dir = std::env::temp_dir().join(format!("ponos-cli-types-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let out = Command::new(ponos_bin())
        .arg("types")
        .current_dir(&dir)
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.is_empty(), "expected no diagnostics: {stderr}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("declare ponos"), "{stdout}");
}

#[test]
fn missing_script_argument_is_usage_error() {
    let out = Command::new(ponos_bin()).arg("run").output().unwrap();
    assert_ne!(out.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.to_lowercase().contains("usage"), "{stderr}");
}

#[test]
fn nonexistent_script_names_path() {
    let out = Command::new(ponos_bin())
        .arg("run")
        .arg("/definitely/not/here.luau")
        .output()
        .unwrap();
    assert_ne!(out.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("/definitely/not/here.luau"), "{stderr}");
}

#[test]
fn attributed_colored_output_for_two_sessions() {
    let project = Project::new("color", &[("MOCK_CHUNKS", "hello world")]);
    let script = project.script(
        r#"
local agent = ponos.agent("mock")
local s1 = agent:session()
local s2 = agent:session()
local r1 = ponos.spawn(function() return s1:prompt("one") end)
local r2 = ponos.spawn(function() return s2:prompt("two") end)
r1:await(); r2:await()
"#,
    );
    let (code, stdout, _) = project.run(&script, &[]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("[mock/s1]"),
        "missing s1 attribution:\n{stdout}"
    );
    assert!(
        stdout.contains("[mock/s2]"),
        "missing s2 attribution:\n{stdout}"
    );
    assert!(
        stdout.contains("\x1b["),
        "expected ANSI color codes:\n{stdout}"
    );
    assert!(stdout.contains("hello world"));
}

#[test]
fn no_color_keeps_prefixes_without_ansi() {
    let project = Project::new("nocolor", &[("MOCK_CHUNKS", "plain text")]);
    let script = project.script(
        r#"
local s = ponos.agent("mock"):session()
s:prompt("hi")
"#,
    );
    let (code, stdout, _) = project.run(&script, &["--no-color"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("[mock/s1]"), "{stdout}");
    assert!(!stdout.contains('\x1b'), "no ANSI expected:\n{stdout}");
}

#[test]
fn quiet_suppresses_render_but_not_print() {
    let project = Project::new("quiet", &[("MOCK_CHUNKS", "streamed")]);
    let script = project.script(
        r#"
print("script-said-this")
ponos.log("log-line")
local s = ponos.agent("mock"):session()
s:prompt("hi")
"#,
    );
    let (code, stdout, _) = project.run(&script, &["--quiet"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("script-said-this"), "{stdout}");
    assert!(stdout.contains("[ponos] log-line"), "{stdout}");
    assert!(!stdout.contains("[mock/"), "{stdout}");
}

#[test]
fn double_verbose_passes_agent_stderr_through() {
    let project = Project::new("stderr", &[("MOCK_STDERR", "agent-chatter")]);
    let script = project.script(
        r#"
local s = ponos.agent("mock"):session()
s:prompt("hi")
"#,
    );
    let (code, stdout, _) = project.run(&script, &["-vv"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("agent-chatter"),
        "agent stderr not passed through:\n{stdout}"
    );

    // Without -vv the chatter stays hidden.
    let (code, stdout, _) = project.run(&script, &[]);
    assert_eq!(code, 0);
    assert!(!stdout.contains("agent-chatter"), "{stdout}");
}

#[test]
fn usage_update_renders_context_line() {
    // agent-sessions spec: `usage_update` carries context-window used/size
    // and is rendered for display (token counts come from the prompt
    // response and are asserted in the acp tests).
    let project = Project::new("usage", &[("MOCK_USAGE", "5,10,2,3")]);
    let script = project.script(
        r#"
local s = ponos.agent("mock"):session()
s:prompt("hi")
"#,
    );
    let (code, stdout, _) = project.run(&script, &[]);
    assert_eq!(code, 0, "{stdout}");
    assert!(
        stdout.contains("context: 5/10 tokens"),
        "usage_update not rendered:\n{stdout}"
    );
}

#[test]
fn exit_code_propagates() {
    let project = Project::new("exit", &[]);
    let script = project.script("ponos.exit(3)");
    let (code, _, _) = project.run(&script, &[]);
    assert_eq!(code, 3);

    let script = project.script("error('boom', 0)");
    let (code, _, stderr) = project.run(&script, &[]);
    assert_eq!(code, 1);
    assert!(stderr.contains("boom"), "{stderr}");
}

#[test]
fn config_change_lifecycle_lines_render() {
    // session-config-options spec "Config changes are rendered": a
    // successful setConfig and an agent-pushed config_option_update each
    // render one session-attributed lifecycle line naming the changed
    // option id and its new value (--verbose).
    let dir = std::env::temp_dir().join(format!("ponos-cli-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".ponos")).unwrap();
    // Env values carry JSON: TOML literal strings keep the quotes intact.
    let config = format!(
        "[agents.mock]\ncommand = \"{}\"\nargs = []\n\n[agents.mock.env]\nMOCK_CONFIG_OPTIONS = '[{}]'\nMOCK_CONFIG_UPDATE = '[{}]'\n",
        mock_bin(),
        r#"{"id":"model","name":"Model","type":"select","currentValue":"opus","options":[{"value":"opus","name":"Opus"},{"value":"haiku","name":"Haiku"}]}"#,
        r#"{"id":"model","name":"Model","type":"select","currentValue":"sonnet","options":[{"value":"opus","name":"Opus"},{"value":"haiku","name":"Haiku"},{"value":"sonnet","name":"Sonnet"}]}"#,
    );
    std::fs::write(dir.join(".ponos").join("config.toml"), config).unwrap();
    let script = dir.join("main.luau");
    std::fs::write(
        &script,
        r#"
local s = ponos.agent("mock"):session()
s:prompt("first")             -- the agent pushes model=sonnet after this turn
s:setConfig("model", "haiku") -- and the set reports model=haiku
s:prompt("second")
s:close()
"#,
    )
    .unwrap();
    let output = Command::new(ponos_bin())
        .arg("run")
        .arg(&script)
        .arg("--verbose")
        .current_dir(&dir)
        .output()
        .expect("run ponos");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("[ponos] mock/s1: config changed: model=sonnet"),
        "pushed change not rendered:\n{stdout}"
    );
    assert!(
        stdout.contains("[ponos] mock/s1: config changed: model=haiku"),
        "set change not rendered:\n{stdout}"
    );
}
