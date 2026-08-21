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
