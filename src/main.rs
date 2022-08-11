use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use atty::Stream;
use clap::{Args, Parser, Subcommand};
use eyre::{eyre, WrapErr};
use itertools::Itertools;
use once_cell::sync::Lazy;
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

    if let Some(code) = nix_develop_exit.status.code() {
        // If the user returns, say, an EOF, we return the same code up
        std::process::exit(code);
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
        // Mapping of `$CRATE_NAME -> $NIXPKGS_NAME`
        static KNOWN_CRATE_TO_BUILD_INPUTS: Lazy<HashMap<&'static str, HashSet<&'static str>>> =
            Lazy::new(|| {
                let mut m = HashMap::new();
                // TODO(@hoverbear): Macro for this?
                macro_rules! crate_to_build_inputs {
                    ($collection:ident, $rust_package:expr, $nix_packages:expr) => {
                        $collection.insert($rust_package, $nix_packages.into_iter().collect())
                    };
                }
                crate_to_build_inputs!(m, "openssl-sys", ["openssl"]);
                crate_to_build_inputs!(m, "pkg-config", ["pkg-config"]);
                crate_to_build_inputs!(m, "expat-sys", ["expat"]);
                crate_to_build_inputs!(m, "freetype-sys", ["freetype"]);
                crate_to_build_inputs!(m, "servo-fontconfig-sys", ["fontconfig"]);
                crate_to_build_inputs!(m, "libsqlite3-sys", ["sqlite"]);
                crate_to_build_inputs!(m, "libusb1-sys", ["libusb"]);
                crate_to_build_inputs!(m, "hidapi", ["udev"]);
                crate_to_build_inputs!(m, "libgit2-sys", ["libgit2"]);
                crate_to_build_inputs!(m, "rdkafka-sys", ["rdkafka"]);
                m
            });

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

        let (package_set, resolve) = cargo::ops::resolve_ws(&workspace)
            .map_err(|e| eyre!(e))
            .wrap_err_with(|| {
                format!(
                    "Could not resolve workspace from `{}`",
                    project_dir.display()
                )
            })?;

        for package in package_set.get_many(resolve.iter()).unwrap() {
            let mut package_build_inputs = HashSet::new();

            if let Some(known_build_inputs) =
                KNOWN_CRATE_TO_BUILD_INPUTS.get(package.name().as_str())
            {
                let known_build_inputs = known_build_inputs
                    .iter()
                    .map(ToString::to_string)
                    .collect::<HashSet<_>>();
                tracing::debug!(package_name = %package.name(), inputs = %known_build_inputs.iter().join(", "), "Detected known build inputs");
                found_build_inputs = found_build_inputs
                    .union(&known_build_inputs)
                    .cloned()
                    .collect();
            }

            // TODO(@hoverbear): Add a `Deserializable` implementor we can get from this.
            let custom_metadata = match package.manifest().custom_metadata() {
                Some(custom_metadata) => custom_metadata,
                None => continue,
            };

            let metadata_table = match custom_metadata {
                toml_edit::easy::value::Value::Table(metadata_table) => metadata_table,
                _ => continue,
            };

            let fsm_table = match metadata_table.get("fsm") {
                Some(toml_edit::easy::value::Value::Table(metadata_table)) => metadata_table,
                Some(_) | None => continue,
            };

            let build_inputs_table = match fsm_table.get("build-inputs") {
                Some(toml_edit::easy::value::Value::Table(build_inputs_table)) => {
                    build_inputs_table
                }
                Some(_) | None => continue,
            };

            for (key, _value) in build_inputs_table.iter() {
                // TODO(@hoverbear): Add version checking
                package_build_inputs.insert(key.to_string());
            }
            tracing::debug!(package_name = %package.name(), inputs = %package_build_inputs.iter().join(", "), "Detected `package.fsm.build-inputs` in `Crate.toml`");
            found_build_inputs = found_build_inputs
                .union(&package_build_inputs)
                .cloned()
                .collect();
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
