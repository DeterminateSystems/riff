mod cargo_metadata;
mod cmds;
mod dependency_registry;
mod dev_env;
mod flake_generator;
mod spinner;
mod telemetry;

use std::error::Error;
use std::io::Write;

use atty::Stream;
use clap::Parser;
use eyre::WrapErr;
use owo_colors::OwoColorize;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use cmds::Commands;
use telemetry::Telemetry;

const FSM_XDG_PREFIX: &str = "fsm";

#[derive(Debug, Parser)]
#[clap(name = "fsm")]
#[clap(about = "Automatically set up build environments using Nix", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
    /// Turn off user telemetry ping
    #[clap(long, global = true, env = "FSM_DISABLE_TELEMETRY")]
    disable_telemetry: bool,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::config::HookBuilder::default()
        .issue_url(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new"))
        .install()?;

    setup_tracing().await?;

    let maybe_args = Cli::try_parse();

    let args = match maybe_args {
        Ok(args) => args,
        Err(e) => {
            // Best effort detect the env var
            match std::env::var("FSM_DISABLE_TELEMETRY") {
                Ok(val) if val == "false" || val == "0" => {
                    Telemetry::new().await.send().await.ok();
                }
                Err(_) => {
                    Telemetry::new().await.send().await.ok();
                }
                _ => (),
            }
            e.exit() // Dead!
        }
    };
    match args.command {
        Commands::Shell(shell) => {
            let code = shell.cmd().await?;
            if let Some(code) = code {
                std::process::exit(code);
            }
        }
        Commands::Run(run) => {
            let code = run.cmd().await?;
            if let Some(code) = code {
                if code == 127 {
                    writeln!(
                        std::io::stderr(),
                        "The command you attempted to run was not found.
Try running it in a shell; for example:
\t{fsm_run_example}\n",
                        fsm_run_example =
                            format!("fsm run -- sh -c '{}'", run.command.join(" ")).cyan(),
                    )?;
                }

                std::process::exit(code);
            }
        }
    };
    Ok(())
}

#[tracing::instrument]
async fn setup_tracing() -> eyre::Result<()> {
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
