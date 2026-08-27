//! Command-line interface: `ptah run <script.luau>` with output flags.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::render::{RenderOptions, Renderer};
use crate::script::{self, RunConfig};
use ptah_core::ports::ConfigSource;

#[derive(Parser, Debug)]
#[command(
    name = "ptah",
    version,
    about = "Luau-scripted multi-agent orchestration over the Agent Client Protocol"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run an orchestration script.
    Run {
        /// Path to the entry Luau script.
        script: PathBuf,
        /// Suppress all streaming render and diagnostics.
        #[arg(long)]
        quiet: bool,
        /// Show runtime lifecycle diagnostics (-vv also passes agent stderr through).
        #[arg(long, short = 'v', action = clap::ArgAction::Count)]
        verbose: u8,
        /// Disable ANSI colors; keep text prefixes.
        #[arg(long)]
        no_color: bool,
    },

    /// Verify a script without executing it.
    Check {
        /// Path to the entry Luau script.
        script: PathBuf,
        /// Disable ANSI coloring of findings.
        #[arg(long)]
        no_color: bool,
    },

    /// Print the Luau type definitions for the ptah script API.
    Types,

    /// Hidden: MCP bridge server for typed results. Spawned per result
    /// session by the agent, as suggested in `session/new { mcpServers }`;
    /// not part of the user-facing surface.
    #[command(name = "__bridge", hide = true)]
    Bridge,
}

/// What `Cli::try_parse_from` produced: dispatch early on subcommands that
/// need no runtime setup.
#[derive(Debug)]
enum Parsed {
    /// `ptah run` — full orchestration run.
    Run {
        script: PathBuf,
        render: RenderOptions,
        verbose: u8,
    },
    /// `ptah types` — print definitions, exit 0, touch nothing else.
    Types,
    /// `ptah check` — verify a script without executing it.
    Check { script: PathBuf, no_color: bool },
    /// `ptah __bridge` — typed-results MCP server over stdio.
    Bridge,
}

/// Parse CLI arguments (unit-testable).
fn parse(args: &[String]) -> Result<Parsed, clap::Error> {
    let cli = Cli::try_parse_from(args)?;
    Ok(match cli.command {
        Command::Run {
            script,
            quiet,
            verbose,
            no_color,
        } => Parsed::Run {
            script,
            render: RenderOptions {
                quiet,
                no_color,
                verbose: verbose >= 1,
                agent_stderr: verbose >= 2,
            },
            verbose,
        },
        Command::Types => Parsed::Types,
        Command::Check { script, no_color } => Parsed::Check { script, no_color },
        Command::Bridge => Parsed::Bridge,
    })
}

/// `ptah types`: print the definitions with a version header. Everything
/// after line 1 is byte-identical to `.ptah/ptah.d.luau`. Requires no
/// script, registry, or agent configuration.
fn print_types() -> ExitCode {
    println!("-- ptah {} type definitions", crate::VERSION);
    print!("{}", crate::check::defs::TYPE_DEFINITIONS);
    ExitCode::SUCCESS
}

/// `ptah check`: compile + static lints in-process, then the luau-lsp
/// typecheck pass with the embedded definitions. Exit `0` clean, `1`
/// findings, `2` the check could not run (missing script, registry
/// discovery failure, luau-lsp missing).
fn run_check(script: PathBuf, no_color: bool) -> ExitCode {
    if !script.is_file() {
        eprintln!("error: script not found: {}", script.display());
        return ExitCode::from(2);
    }
    let invocation_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let registry = match crate::config_fs::FsConfigSource.discover(&invocation_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let code = crate::check::check(&crate::check::CheckConfig {
        script_path: script,
        registry,
        color: !no_color,
    });
    ExitCode::from(code)
}

/// Entry point: returns the process exit code.
pub fn main() -> ExitCode {
    let mut args: Vec<String> = vec!["ptah".to_string()];
    args.extend(std::env::args().skip(1));
    let (script, render_opts, verbose) = match parse(&args) {
        Ok(Parsed::Run {
            script,
            render,
            verbose,
        }) => (script, render, verbose),
        Ok(Parsed::Types) => return print_types(),
        Ok(Parsed::Check { script, no_color }) => return run_check(script, no_color),
        Ok(Parsed::Bridge) => return crate::bridge::run(),
        Err(e) => {
            // --help / --version are "errors" that carry their own output
            // and exit code.
            e.print().ok();
            let kind = e.kind();
            if matches!(
                kind,
                clap::error::ErrorKind::DisplayVersion | clap::error::ErrorKind::DisplayHelp
            ) {
                return ExitCode::SUCCESS;
            }
            return ExitCode::from(2);
        }
    };

    if !script.is_file() {
        eprintln!("error: script not found: {}", script.display());
        return ExitCode::from(2);
    }

    // Tracing to stderr for library internals; the renderer owns stdout.
    let level = match verbose {
        0 => "warn",
        1 => "info",
        _ => "debug",
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .with_ansi(!render_opts.no_color)
        .with_writer(std::io::stderr)
        .try_init();

    let invocation_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let registry = match crate::config_fs::FsConfigSource.discover(&invocation_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    // Pre-flight: fail certainly-broken scripts (uncompilable entry or
    // reachable module, unresolvable literal require, unknown literal
    // agent name) before anything spawns. Computed forms are not
    // linted; strictness is not enforced here (`ptah check` does that).
    let preflight = crate::check::preflight(&script, &registry);
    if !preflight.is_empty() {
        for finding in &preflight {
            eprintln!("{}", finding.render(!render_opts.no_color));
        }
        eprintln!("{}", crate::check::summary_line(&preflight));
        return ExitCode::from(1);
    }

    let renderer = std::sync::Arc::new(Renderer::new(render_opts));
    // The composition line change ② moved out of the script crate: the
    // ACP stdio adapter is chosen here, at the composition root, and
    // injected through the `AgentTransport` port. The process runner is
    // the same decision one port over: ptah always injects it (there is
    // no gating flag — running a ptah script already implies arbitrary
    // shell through the headless allow-all agent posture).
    // Outer cancellation channel: SIGINT/SIGTERM forward into it below
    // (first signal — teardown rides inside `run`, exit code 130/143;
    // second signal — exit at once). Created before the runtime so the
    // config is complete here; the monitor task is spawned inside it.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(0);
    let config = RunConfig {
        script_path: script,
        invocation_dir,
        registry,
        transport: std::sync::Arc::new(ptah_acp::Transport),
        process_runner: Some(std::sync::Arc::new(crate::exec::TokioProcessRunner)),
        shutdown: Some(shutdown_rx),
        renderer,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    // Outer cancellation: SIGINT/SIGTERM race the script inside `run`.
    // The first signal rides the run's teardown path (kills in-flight
    // exec process groups, closes agent sessions) and the run reports
    // the shell-conventional 128+signal exit code; a second signal
    // during teardown exits at once. Signal disposition is a
    // world-touching composition decision, so it is installed here —
    // the run loop itself only sees the channel.
    let outcome = rt.block_on(async {
        let signal_monitor = install_signal_monitor(shutdown_tx);
        let outcome = tokio::task::LocalSet::new()
            .run_until(script::run(config))
            .await;
        if let Some(monitor) = signal_monitor {
            monitor.abort();
        }
        outcome
    });

    if let Some(error) = &outcome.error {
        eprintln!("error: {error}");
    }
    for err in &outcome.undelivered_errors {
        eprintln!("error: unobserved task error: {err}");
    }

    ExitCode::from(u8::try_from(outcome.code).unwrap_or(1))
}

/// Install the SIGINT/SIGTERM monitor forwarding into the run's
/// shutdown watch. The first signal sends the shell-conventional code
/// (130/143) — the run then tears itself down — and any later signal
/// exits immediately (the "press again to force" escape for a stuck
/// teardown). Returns the monitor task, aborted when the run ends on
/// its own so a late signal cannot kill a finishing process. Non-unix
/// platforms have no such signals: no monitor, and a dropped sender
/// the run reads as "nobody will ever cancel".
fn install_signal_monitor(
    shutdown_tx: tokio::sync::watch::Sender<i32>,
) -> Option<tokio::task::JoinHandle<()>> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let Ok(mut int) = signal(SignalKind::interrupt()) else {
            return None;
        };
        let Ok(mut term) = signal(SignalKind::terminate()) else {
            return None;
        };
        Some(tokio::spawn(async move {
            let code = tokio::select! {
                _ = int.recv() => 130,
                _ = term.recv() => 143,
            };
            let _ = shutdown_tx.send(code);
            // A second signal means the user wants out *now*: teardown
            // is still draining — exit hard with the interrupt code.
            tokio::select! {
                _ = int.recv() => {}
                _ = term.recv() => {}
            }
            std::process::exit(130);
        }))
    }
    #[cfg(not(unix))]
    {
        drop(shutdown_tx);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("ptah".to_string())
            .chain(list.iter().map(std::string::ToString::to_string))
            .collect()
    }

    #[test]
    fn types_subcommand_parses_without_run_arguments() {
        match parse(&args(&["types"])).unwrap() {
            Parsed::Types => {}
            Parsed::Run { .. } | Parsed::Check { .. } | Parsed::Bridge => panic!("expected Types"),
        }
    }

    #[test]
    fn missing_script_argument_is_a_usage_error() {
        let err = parse(&args(&["run"])).unwrap_err();
        assert!(!matches!(
            err.kind(),
            clap::error::ErrorKind::DisplayVersion | clap::error::ErrorKind::DisplayHelp
        ));
    }

    #[test]
    fn check_subcommand_parses() {
        match parse(&args(&["check", "s.luau"])).unwrap() {
            Parsed::Check { script, no_color } => {
                assert_eq!(script, PathBuf::from("s.luau"));
                assert!(!no_color);
            }
            _ => panic!("expected Check"),
        }
        match parse(&args(&["check", "s.luau", "--no-color"])).unwrap() {
            Parsed::Check { no_color: true, .. } => {}
            _ => panic!("expected Check"),
        }
    }

    #[test]
    fn check_without_script_is_a_usage_error() {
        let err = parse(&args(&["check"])).unwrap_err();
        assert!(!matches!(
            err.kind(),
            clap::error::ErrorKind::DisplayVersion | clap::error::ErrorKind::DisplayHelp
        ));
    }

    #[test]
    fn missing_subcommand_is_a_usage_error() {
        let err = parse(&args(&[])).unwrap_err();
        assert!(!matches!(
            err.kind(),
            clap::error::ErrorKind::DisplayVersion | clap::error::ErrorKind::DisplayHelp
        ));
    }

    #[test]
    fn flags_parse() {
        let Parsed::Run {
            script,
            render: opts,
            verbose: v,
        } = parse(&args(&["run", "s.luau"])).unwrap()
        else {
            panic!("expected Run")
        };
        assert_eq!(script, PathBuf::from("s.luau"));
        assert!(!opts.quiet && !opts.no_color && !opts.verbose && !opts.agent_stderr);
        assert_eq!(v, 0);

        let Parsed::Run {
            render: opts,
            verbose: v,
            ..
        } = parse(&args(&["run", "s.luau", "--quiet", "--no-color"])).unwrap()
        else {
            panic!("expected Run")
        };
        assert!(opts.quiet && opts.no_color);
        assert_eq!(v, 0);

        let Parsed::Run {
            render: opts,
            verbose: v,
            ..
        } = parse(&args(&["run", "s.luau", "-vv"])).unwrap()
        else {
            panic!("expected Run")
        };
        assert!(opts.verbose && opts.agent_stderr);
        assert_eq!(v, 2);
    }

    #[test]
    fn version_flag_reports_version() {
        // clap handles --version at parse time; a successful parse of
        // --version is a DisplayVersion "error".
        let parsed = Cli::try_parse_from(args(&["--version"]));
        assert!(parsed.is_err()); // clap exits with the version print
        let err = parsed.unwrap_err().to_string();
        assert!(
            err.contains(crate::VERSION) || err.contains("version"),
            "{err}"
        );
    }
}
