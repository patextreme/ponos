//! The bundled examples run green against the mock agent (CI parity for
//! task 8.1): each example is executed via the real binary with a generated
//! project registry mapping `demo` to the mock agent.

use std::path::PathBuf;
use std::process::Command;

fn ponos_bin() -> &'static str {
    env!("CARGO_BIN_EXE_ponos")
}

fn mock_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mock-agent")
}

fn project(example: &str, agent_env: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ponos-examples-{}-{example}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".ponos")).unwrap();
    let mut config = format!(
        "[agents.demo]\ncommand = \"{}\"\nargs = []\n\n[agents.demo.env]\n",
        mock_bin()
    );
    for (k, v) in agent_env {
        // TOML literal string: env values may carry double quotes (JSON).
        config.push_str(&format!("{k} = '{v}'\n"));
    }
    std::fs::write(dir.join(".ponos").join("config.toml"), config).unwrap();
    dir
}

fn run_example(example: &str, agent_env: &[(&str, &str)]) {
    let dir = project(example, agent_env);
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(example);
    let output = Command::new(ponos_bin())
        .arg("run")
        .arg(&script)
        .current_dir(&dir)
        .output()
        .expect("run ponos");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{example} failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn example_sequential_review() {
    run_example("sequential_review.luau", &[]);
}

#[test]
fn example_fanout() {
    run_example("fanout.luau", &[]);
}

#[test]
fn example_watchdog() {
    run_example("watchdog.luau", &[("MOCK_HANG", "1")]);
}

#[test]
fn example_typed_results() {
    run_example(
        "typed_results.luau",
        &[("MOCK_SUBMIT", r#"{"verdict":"approve","score":8}"#)],
    );
}

#[test]
fn example_workflow_1_shared_helper() {
    // Cross-tree require: the entry requires ../shared/helper from a
    // sibling directory of its own tree.
    run_example("workflow-1/main.luau", &[]);
}

#[test]
fn example_workflow_2_shared_helper() {
    run_example("workflow-2/main.luau", &[]);
}

#[test]
fn example_model_fanout() {
    // The mock advertises a `model` select option and echoes its current
    // value in each reply, so the two sessions provably run under the two
    // models the example sets.
    run_example(
        "model-fanout.luau",
        &[
            (
                "MOCK_CONFIG_OPTIONS",
                r#"[{"id":"model","name":"Model","type":"select","currentValue":"sonnet","options":[{"value":"sonnet","name":"Sonnet"},{"value":"opus","name":"Opus"},{"value":"haiku","name":"Haiku"}]}]"#,
            ),
            ("MOCK_CONFIG_ECHO", "model"),
        ],
    );
}
