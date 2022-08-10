use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use atty::Stream;
use clap::{Args, Parser, Subcommand};
use eyre::{eyre, WrapErr};
use itertools::Itertools;
use owo_colors::OwoColorize;
use tempfile::TempDir;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Debug, Parser)]
#[clap(name = "fsm")]
#[clap(about = "Automatically set up build environments using Nix", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Shell(Shell),
}

/// Start a development shell
#[derive(Debug, Args)]
struct Shell {
    /// The root directory of the project
    #[clap(long, value_parser)]
    project_dir: Option<PathBuf>,
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
        Commands::Shell(shell_args) => cmd_shell(shell_args).await,
    }
}

async fn cmd_shell(shell_args: Shell) -> color_eyre::Result<()> {
    let project_dir = get_project_dir(shell_args.project_dir);

    tracing::debug!("Project directory is '{}'.", project_dir.display());

    let mut dev_env = DevEnvironment::default();

    dev_env.detect(&project_dir)?;

    let flake_nix = dev_env.to_flake();

    tracing::trace!("Generated 'flake.nix':\n{}", flake_nix);

    let flake_dir = TempDir::new()?;

    let flake_nix_path = flake_dir.path().join("flake.nix");

    // FIXME: do async I/O?
    std::fs::write(&flake_nix_path, &flake_nix).expect("Unable to write flake.nix");

    let mut nix_lock_command = Command::new("nix");
    nix_lock_command
        .arg("flake")
        .arg("lock")
        .args(&["--extra-experimental-features", "flakes nix-command"])
        .arg("-L")
        .arg(format!("path://{}", flake_dir.path().to_str().unwrap()));
    tracing::trace!(command = ?nix_lock_command, "Running");
    let nix_lock_exit = nix_lock_command
        .output()
        .wrap_err("Could not execute `nix flake lock`")?;

    if !nix_lock_exit.status.success() {
        return Err(eyre!(
            "`nix flake lock` exited with code {}:\n{}",
            nix_lock_exit
                .status
                .code()
                .map(|x| x.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            std::str::from_utf8(&nix_lock_exit.stdout)?,
        ));
    }

    let mut nix_develop_command = Command::new("nix");
    nix_develop_command
        .arg("develop")
        .args(&["--extra-experimental-features", "flakes nix-command"])
        .arg("-L")
        .arg(format!("path://{}", flake_dir.path().to_str().unwrap()))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    tracing::trace!(command = ?nix_develop_command, "Running");
    let nix_develop_exit = nix_develop_command
        .output()
        .wrap_err("Could not execute `nix develop`")?;

    // At this point we have handed off to the user shell. The next lines run after the user CTRL+D's out.

    if !nix_develop_exit.status.success() {
        return Err(eyre!(
            "`nix develop` exited with code {}:\n{}",
            nix_develop_exit
                .status
                .code()
                .map(|x| x.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            std::str::from_utf8(&nix_develop_exit.stdout)?,
        ));
    }

    Ok(())
}

fn get_project_dir(project_dir: Option<PathBuf>) -> PathBuf {
    project_dir.unwrap_or_else(|| std::env::current_dir().unwrap())
}

#[derive(Default)]
struct DevEnvironment {
    build_inputs: HashSet<String>,
    extra_attrs: HashMap<String, String>,
}

impl DevEnvironment {
    fn to_flake(&self) -> String {
        // TODO: use rnix for generating Nix?
        format!(
            include_str!("flake-template.inc"),
            self.build_inputs.iter().join(" "),
            self.extra_attrs
                .iter()
                .map(|(name, value)| format!("\"{}\" = \"{}\";", name, value))
                .join("\n"),
        )
    }

    fn detect(&mut self, project_dir: &Path) -> color_eyre::Result<()> {
        let mut any_found = false;

        if project_dir.join("Cargo.toml").exists() {
            self.add_deps_from_cargo(project_dir)?;
            any_found = true;
        }

        if !any_found {
            eprintln!(
                "'{}' does not contain a project recognized by FSM.",
                project_dir.display()
            );
        }

        Ok(())
    }

    #[tracing::instrument(skip_all, fields(project_dir = %project_dir.display()))]
    fn add_deps_from_cargo(&mut self, project_dir: &Path) -> color_eyre::Result<()> {
        tracing::debug!("Adding Cargo dependencies...");

        let mut found_build_inputs = HashSet::new();
        found_build_inputs.insert("rustc".to_string());
        found_build_inputs.insert("cargo".to_string());

        let mut cfg = cargo::util::config::Config::default()
            .map_err(|e| eyre!(e))
            .wrap_err("Could not get default `cargo` instance")?;

        // TODO(@hoverbear): Add verbosity option
        cfg.configure(
            0,     // verbose
            true,  // quiet
            None,  // color
            false, // frozen
            false, // locked
            false, // offline
            &None, // target_dir
            &[],   // unstable_flags
            &[],   // cli_config
        )
        .map_err(|e| eyre!(e))
        .wrap_err("Could not configure `cargo`")?;

        let workspace = cargo::core::Workspace::new(&project_dir.join("Cargo.toml"), &cfg)
            .map_err(|e| eyre!(e))
            .wrap_err_with(|| {
                format!(
                    "Could not create workspace from `{}`",
                    project_dir.display()
                )
            })?;

        let (_package_set, resolve) = cargo::ops::resolve_ws(&workspace)
            .map_err(|e| eyre!(e))
            .wrap_err_with(|| {
                format!(
                    "Could not resolve workspace from `{}`",
                    project_dir.display()
                )
            })?;

        let package_names: HashMap<_, _> = resolve
            .iter()
            .map(|pkg_id| (pkg_id.name(), pkg_id))
            .collect();

        if package_names.contains_key("pkg-config") {
            self.build_inputs.insert("pkg-config".to_string());
        }

        if package_names.contains_key("expat-sys") {
            found_build_inputs.insert("expat".to_string());
        }

        if package_names.contains_key("freetype-sys") {
            found_build_inputs.insert("freetype".to_string());
        }

        if package_names.contains_key("servo-fontconfig-sys") {
            found_build_inputs.insert("fontconfig".to_string());
        }

        if package_names.contains_key("libsqlite3-sys") {
            found_build_inputs.insert("sqlite".to_string());
        }

        if package_names.contains_key("openssl-sys") {
            found_build_inputs.insert("openssl".to_string());
        }

        if package_names.contains_key("prost-build") {
            found_build_inputs.insert("protobuf".to_string());
        }

        if package_names.contains_key("rdkafka-sys") {
            found_build_inputs.insert("rdkafka".to_string());
            found_build_inputs.insert("pkg-config".to_string());
            // FIXME: ugly. Unless the 'dynamic-linking' feature is
            // set, rdkafka-sys will try to build its own
            // statically-linked rdkafka from source.
            self.extra_attrs
                .insert("CARGO_FEATURE_DYNAMIC_LINKING".to_owned(), "1".to_owned());
        }

        eprintln!(
            "{check} {lang}: {colored_inputs}",
            check = "✓".green(),
            lang = "🦀 rust".bold().red(),
            colored_inputs = {
                let mut sorted_build_inputs = found_build_inputs.iter().collect::<Vec<_>>();
                sorted_build_inputs.sort();
                sorted_build_inputs.iter().map(|v| v.cyan()).join(", ")
            }
        );

        self.build_inputs = found_build_inputs;

        Ok(())
    }
}
