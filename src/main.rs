mod cargo_metadata;
mod cmds;
mod dev_env;

use std::error::Error;

use atty::Stream;
use clap::Parser;
use eyre::WrapErr;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use cmds::Commands;

#[derive(Debug, Parser)]
#[clap(name = "fsm")]
#[clap(about = "Automatically set up build environments using Nix", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::config::HookBuilder::default().install()?;

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

    main_impl().await?;

    Ok(())
}

async fn main_impl() -> color_eyre::Result<()> {
    let args = Cli::parse();

    match args.command {
        Commands::Shell(shell) => shell.cmd().await,
    }
}
