use std::error::Error;

use eyre::WrapErr;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

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
            EnvFilter::try_new(&format!("{}={}", env!("CARGO_PKG_NAME"), "debug"))?
        }
    };

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(ErrorLayer::default())
        .try_init()?;

    main_impl().await?;

    Ok(())
}

async fn main_impl() -> color_eyre::Result<()> {
    println!("Hello, world!");

    Ok(())
}
