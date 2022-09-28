mod cargo_metadata;
mod cmds;
mod dependency_registry;
mod dev_env;
mod flake_generator;
mod nix_dev_env;
mod spinner;
mod telemetry;

use std::error::Error;
use std::io::Write;
use std::process::ExitCode;

use atty::Stream;
use clap::Parser;
use eyre::WrapErr;
use owo_colors::OwoColorize;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use cmds::Commands;
use telemetry::Telemetry;

const RIFF_XDG_PREFIX: &str = "riff";

#[derive(Debug, Parser)]
#[clap(name = "riff")]
#[clap(version, about = "Automatically set up build environments using Nix", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
    /// Turn off user telemetry ping
    #[clap(long, global = true, env = "RIFF_DISABLE_TELEMETRY")]
    disable_telemetry: bool,
    /// Disable all network usage except `nix develop`
    // TODO(@hoverbear): Can we disable that, too?
    #[clap(long, global = true, env = "RIFF_OFFLINE")]
    offline: bool,
    /// Print out debug logging
    #[clap(long, global = true)]
    debug: bool,
}

#[tokio::main]
async fn main() -> color_eyre::Result<std::process::ExitCode> {
    color_eyre::config::HookBuilder::default()
        .issue_url(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new"))
        .install()?;

    setup_tracing().await?;

    let maybe_args = Cli::try_parse();

    let args = match maybe_args {
        Ok(args) => args,
        Err(e) => {
            let telemetry_ok_via_env = match std::env::var("RIFF_DISABLE_TELEMETRY")
                .or_else(|_| std::env::var("RIFF_OFFLINE"))
            {
                Ok(val) if val == "false" || val == "0" || val.is_empty() => true,
                Err(_) => true,
                _ => false,
            };
            let telemetry_ok_via_flag = !std::env::args()
                .take_while(|v| v != "--")
                .any(|v| v == *"--disable-telemetry" || v == *"--offline");
            if telemetry_ok_via_env && telemetry_ok_via_flag {
                Telemetry::new().await.send().await.ok();
            }
            e.exit() // Dead!
        }
    };
    match args.command {
        Commands::PrintDevEnv(print_dev_env) => {
            Ok(exit_status_to_exit_code(print_dev_env.cmd().await?))
        }
        Commands::Shell(shell) => Ok(exit_status_to_exit_code(shell.cmd().await?)),
        Commands::Run(run) => {
            let code = run.cmd().await?;
            if let Some(code) = code {
                if code == 127 {
                    writeln!(
                        std::io::stderr(),
                        "The command you attempted to run was not found.
Try running it in a shell; for example:
\t{riff_run_example}\n",
                        riff_run_example =
                            format!("riff run -- sh -c '{}'", run.command.join(" ")).cyan(),
                    )?;
                }
            }

            Ok(exit_status_to_exit_code(code))
        }
    }
}

fn exit_status_to_exit_code(status: Option<i32>) -> ExitCode {
    status
        .map(|x| (x as u8).into())
        .unwrap_or(ExitCode::SUCCESS)
}

#[tracing::instrument]
async fn setup_tracing() -> eyre::Result<()> {
    let debug = std::env::args()
        .take_while(|v| v != "--")
        .any(|v| v == "--debug");

    let filter_layer = match EnvFilter::try_from_default_env() {
        Ok(layer) => layer,
        Err(e) => {
            // Catch a parse error and report it, ignore a missing env.
            if let Some(source) = e.source() {
                match source.downcast_ref::<std::env::VarError>() {
                    Some(std::env::VarError::NotPresent) => (),
                    _ => return Err(e).wrap_err_with(|| "parsing RUST_LOG directives"),
                }
            }
            EnvFilter::try_new(&format!("{}={}", env!("CARGO_PKG_NAME"), "info"))?
        }
    };

    let filter_layer = if debug {
        let directive = format!("{}={}", env!("CARGO_PKG_NAME"), "debug").parse()?;
        filter_layer.add_directive(directive)
    } else {
        filter_layer
    };

    // Initialize tracing with tracing-error, and eyre
    let fmt_layer = tracing_subscriber::fmt::Layer::new()
        .with_ansi(atty::is(Stream::Stderr))
        .with_writer(std::io::stderr)
        .pretty();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .with(ErrorLayer::default())
        .try_init()?;

    Ok(())
}
