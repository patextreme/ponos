//! CLI + renderer integration tests: run the real `ponos` binary against
//! the mock agent via a project registry and assert on captured output.

use std::path::PathBuf;
use std::process::Command;

mod common;

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
fn relative_script_path_with_require_runs() {
    // A script invoked through a relative path (with a directory
    // component) must still be able to require sibling modules: the
    // require sandbox root must share the absolute namespace of chunk
    // names, or every require is rejected as escaping the tree.
    let project = Project::new("relative-require", &[]);
    std::fs::create_dir_all(project.dir.join("sub")).unwrap();
    std::fs::write(project.dir.join("sub/mod.luau"), "return 42").unwrap();
    std::fs::write(
        project.dir.join("sub/main.luau"),
        "local n = require('./mod')\nassert(n == 42)\n",
    )
    .unwrap();
    let (code, _, stderr) = project.run(&PathBuf::from("sub/main.luau"), &["--quiet"]);
    assert_eq!(code, 0, "stderr: {stderr}");
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
    let stdout = common::strip_timestamps(&stdout);
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
    let stdout = common::strip_timestamps(&stdout);
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
    let stdout = common::strip_timestamps(&stdout);
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
    let stdout = common::strip_timestamps(&stdout);
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
    let stripped = common::strip_timestamps(&stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stripped.contains("[ponos] mock/s1: config changed: model=sonnet"),
        "pushed change not rendered:\n{stdout}"
    );
    assert!(
        stripped.contains("[ponos] mock/s1: config changed: model=haiku"),
        "set change not rendered:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Tool-line rendering (agent-sessions spec: the tool-line contract)
// ---------------------------------------------------------------------------

/// Bodies of the rendered `tool: …` lines, timestamps stripped. Runs must
/// use `--no-color` so the `] tool: ` split is exact.
fn tool_bodies(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .map(common::strip_timestamp)
        .filter_map(|l| l.split_once("] tool: ").map(|(_, body)| body.to_string()))
        .collect()
}

/// The seconds value of a terminal line's `(status, X.Ys)` suffix.
fn terminal_duration(body: &str, status: &str) -> f64 {
    let rest = body
        .rsplit_once(&format!("({status}, "))
        .unwrap_or_else(|| panic!("no ({status}, …) suffix: {body:?}"))
        .1;
    let rest = rest
        .strip_suffix(')')
        .unwrap_or_else(|| panic!("no closing paren: {rest:?}"));
    let secs = rest
        .strip_suffix('s')
        .unwrap_or_else(|| panic!("no s unit: {rest:?}"));
    // `Mm SS.Ss` above a minute: take both parts.
    if let Some((mins, s)) = secs.split_once("m ") {
        mins.trim().parse::<f64>().unwrap() * 60.0 + s.parse::<f64>().unwrap()
    } else {
        secs.parse::<f64>().unwrap()
    }
}

fn run_prompt_script(project: &Project, flags: &[&str]) -> String {
    let script = project.script(
        r#"
local s = ponos.agent("mock"):session()
s:prompt("hi")
s:close()
"#,
    );
    let (code, stdout, _) = project.run(&script, flags);
    assert_eq!(code, 0, "{stdout}");
    stdout
}

#[test]
fn tool_line_renders_start_and_terminal_only() {
    // agent-sessions spec "Tool call renders start and terminal lines" +
    // "Repeated statuses do not flood the log": the full
    // pending → in_progress → in_progress → completed sequence renders
    // exactly two lines — the bare title at start, title + status +
    // duration at completion. The repeated in_progress renders nothing.
    let project = Project::new(
        "tool-flow",
        &[(
            "MOCK_TOOL_FLOW",
            "pending,in_progress,in_progress,completed",
        )],
    );
    let stdout = run_prompt_script(&project, &["--no-color"]);
    let lines = tool_bodies(&stdout);
    assert_eq!(lines.len(), 2, "exactly start + terminal:\n{stdout}");
    assert_eq!(lines[0], "Search files \"foo\"", "start line:\n{stdout}");
    assert!(
        lines[1].starts_with("Search files \"foo\" (completed, ") && lines[1].ends_with("s)"),
        "terminal line:\n{stdout}"
    );
    // Titles resolve through the id→title map: never the raw call id.
    assert!(
        !stdout.contains("tool-flow-1"),
        "raw call id leaked:\n{stdout}"
    );
}

#[test]
fn tool_line_re_sent_terminal_status_is_silent() {
    // agent-sessions spec "Repeated statuses do not flood the log": a
    // re-sent terminal status renders nothing beyond its first line.
    let project = Project::new(
        "tool-repeat",
        &[("MOCK_TOOL_FLOW", "pending,completed,completed")],
    );
    let stdout = run_prompt_script(&project, &["--no-color"]);
    let lines = tool_bodies(&stdout);
    assert_eq!(lines.len(), 1, "only the first terminal renders:\n{stdout}");
    assert!(
        lines[0].starts_with("Search files \"foo\" (completed, "),
        "terminal line:\n{stdout}"
    );
}

#[test]
fn tool_line_pending_is_silent() {
    // agent-sessions spec "Pending is silent": a pending announcement
    // renders no line at all.
    let project = Project::new("tool-pending", &[("MOCK_TOOL_FLOW", "pending")]);
    let stdout = run_prompt_script(&project, &["--no-color"]);
    assert!(
        tool_bodies(&stdout).is_empty(),
        "pending must not render:\n{stdout}"
    );
}

#[test]
fn tool_line_unannounced_update_falls_back_to_call_id() {
    // agent-sessions spec "Unannounced update falls back to the call id":
    // `!`-prefixed entries arrive without any `tool_call` announcement,
    // so the lines name the raw id.
    let project = Project::new(
        "tool-unannounced",
        &[("MOCK_TOOL_FLOW", "!in_progress,!completed")],
    );
    let stdout = run_prompt_script(&project, &["--no-color"]);
    let lines = tool_bodies(&stdout);
    assert_eq!(lines.len(), 2, "start + terminal still render:\n{stdout}");
    assert_eq!(lines[0], "tool-flow-1", "raw-id start line:\n{stdout}");
    assert!(
        lines[1].starts_with("tool-flow-1 (completed, ") && lines[1].ends_with("s)"),
        "raw-id terminal line:\n{stdout}"
    );
}

#[test]
fn tool_line_direct_completion_measures_from_first_observation() {
    // agent-sessions spec "Direct completion without progress": pending
    // → completed with no in_progress renders only the terminal line,
    // duration measured from first observation (the pending
    // announcement) — MOCK_DELAY_MS=120 guarantees a non-trivial span.
    let project = Project::new(
        "tool-direct",
        &[
            ("MOCK_TOOL_FLOW", "pending,completed"),
            ("MOCK_DELAY_MS", "120"),
        ],
    );
    let stdout = run_prompt_script(&project, &["--no-color"]);
    let lines = tool_bodies(&stdout);
    assert_eq!(lines.len(), 1, "terminal line only:\n{stdout}");
    assert!(
        lines[0].starts_with("Search files \"foo\" (completed, "),
        "terminal line:\n{stdout}"
    );
    assert!(
        terminal_duration(&lines[0], "completed") >= 0.1,
        "duration must span the announcement → completion gap:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Timestamps (cli spec: rendered lines are timestamped)
// ---------------------------------------------------------------------------

/// `true` for `HH:MM:SS` at the start of a (plain) line.
fn starts_with_hhmmss(line: &str) -> bool {
    let b = line.as_bytes();
    b.len() >= 8
        && b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && b[2] == b':'
        && b[3].is_ascii_digit()
        && b[4].is_ascii_digit()
        && b[5] == b':'
        && b[6].is_ascii_digit()
        && b[7].is_ascii_digit()
}

#[test]
fn rendered_lines_carry_plain_timestamps_under_no_color() {
    // cli spec "Rendered lines are timestamped" + "No-color keeps plain
    // timestamps": every renderer line (chunk, tool, plan, usage,
    // ponos.log) begins `HH:MM:SS [` with no ANSI anywhere; script print
    // output is byte-identical.
    let project = Project::new(
        "ts-plain",
        &[
            ("MOCK_CHUNKS", "hello"),
            ("MOCK_TOOL", "1"),
            ("MOCK_PLAN", "1"),
            ("MOCK_USAGE", "5,10,2,3"),
        ],
    );
    let script = project.script(
        r#"
print("print-line")
ponos.log("log-line")
local s = ponos.agent("mock"):session()
s:prompt("hi")
s:close()
"#,
    );
    let (code, stdout, _) = project.run(&script, &["--no-color"]);
    assert_eq!(code, 0, "{stdout}");
    let mut saw_print = false;
    let mut saw_rendered = false;
    for line in stdout.lines() {
        if line == "print-line" {
            saw_print = true; // "Script print output is untouched"
            continue;
        }
        saw_rendered = true;
        assert!(
            starts_with_hhmmss(line) && line[8..].starts_with(" ["),
            "line misses `HH:MM:SS [` prefix: {line:?}\n{stdout}"
        );
    }
    assert!(saw_print, "print line missing:\n{stdout}");
    assert!(saw_rendered, "no rendered lines found:\n{stdout}");
    assert!(!stdout.contains('\x1b'), "no ANSI expected:\n{stdout}");
}

#[test]
fn rendered_lines_carry_dimmed_timestamps_under_color() {
    // Same contract with color on: the timestamp is dimmed (`\x1b[2m` …
    // `\x1b[0m`) ahead of the `[label]` prefix.
    let project = Project::new("ts-color", &[("MOCK_CHUNKS", "hello")]);
    let stdout = run_prompt_script(&project, &[]);
    let chunk_line = stdout
        .lines()
        .find(|l| l.ends_with("hello\x1b[0m"))
        .unwrap_or_else(|| panic!("chunk line missing:\n{stdout}"));
    assert!(
        chunk_line.starts_with("\x1b[2m")
            && starts_with_hhmmss(&chunk_line[4..])
            && chunk_line[12..].starts_with("\x1b[0m ["),
        "timestamp not dimmed-leading: {chunk_line:?}\n{stdout}"
    );
}
