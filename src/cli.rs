//! Command-line interface: `ponos run <script.luau>` with output flags.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::config::Registry;
use crate::render::{RenderOptions, Renderer};
use crate::script::{self, RunConfig};

#[derive(Parser, Debug)]
#[command(
    name = "ponos",
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

    /// Print the Luau type definitions for the ponos script API.
    Types,

    /// Hidden: MCP bridge server for typed results. Spawned per result
    /// session by the agent, as suggested in `session/new { mcpServers }`;
    /// not part of the user-facing surface.
    #[command(name = "__bridge", hide = true)]
    Bridge,
}

/// Luau type definitions for the `ponos` script API. Single source of
/// truth: `types/ponos.d.luau`, embedded at compile time so the emitted
/// definitions always match the installed binary.
const TYPE_DEFINITIONS: &str = include_str!("../types/ponos.d.luau");

/// What `Cli::try_parse_from` produced: dispatch early on subcommands that
/// need no runtime setup.
#[derive(Debug)]
enum Parsed {
    /// `ponos run` — full orchestration run.
    Run {
        script: PathBuf,
        render: RenderOptions,
        verbose: u8,
    },
    /// `ponos types` — print definitions, exit 0, touch nothing else.
    Types,
    /// `ponos __bridge` — typed-results MCP server over stdio.
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
        Command::Bridge => Parsed::Bridge,
    })
}

/// `ponos types`: print the definitions with a version header. Everything
/// after line 1 is byte-identical to `types/ponos.d.luau`. Requires no
/// script, registry, or agent configuration.
fn print_types() -> ExitCode {
    println!("-- ponos {} type definitions", crate::VERSION);
    print!("{TYPE_DEFINITIONS}");
    ExitCode::SUCCESS
}

/// Entry point: returns the process exit code.
pub fn main() -> ExitCode {
    let mut args: Vec<String> = vec!["ponos".to_string()];
    args.extend(std::env::args().skip(1));
    let (script, render_opts, verbose) = match parse(&args) {
        Ok(Parsed::Run {
            script,
            render,
            verbose,
        }) => (script, render, verbose),
        Ok(Parsed::Types) => return print_types(),
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
    let registry = match Registry::discover(&invocation_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };

    let renderer = std::sync::Arc::new(Renderer::new(render_opts));
    let config = RunConfig {
        script_path: script,
        invocation_dir,
        registry,
        renderer,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let outcome = rt.block_on(tokio::task::LocalSet::new().run_until(script::run(config)));

    if let Some(error) = &outcome.error {
        eprintln!("error: {error}");
    }
    for err in &outcome.undelivered_errors {
        eprintln!("error: unobserved task error: {err}");
    }

    ExitCode::from(u8::try_from(outcome.code).unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("ponos".to_string())
            .chain(list.iter().map(|s| s.to_string()))
            .collect()
    }

    #[test]
    fn types_subcommand_parses_without_run_arguments() {
        match parse(&args(&["types"])).unwrap() {
            Parsed::Types => {}
            Parsed::Run { .. } | Parsed::Bridge => panic!("expected Types"),
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
