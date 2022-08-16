use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

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

#[derive(serde::Deserialize)]
struct CargoMetadata {
    packages: Vec<Package>,
}

// TODO: further specify the type of the serde_json::Value?
#[derive(serde::Deserialize)]
struct Package {
    name: String,
    metadata: HashMap<String, HashMap<String, serde_json::Value>>,
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

    dev_env.detect(&project_dir).await?;

    let flake_nix = dev_env.to_flake();

    tracing::trace!("Generated 'flake.nix':\n{}", flake_nix);

    let flake_dir = TempDir::new()?;

    let flake_nix_path = flake_dir.path().join("flake.nix");

    tokio::fs::write(&flake_nix_path, &flake_nix)
        .await
        .wrap_err("Unable to write flake.nix")?;

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
        .await
        .wrap_err("Could not execute `nix flake lock`")?;

    if !nix_lock_exit.status.success() {
        return Err(eyre!(
            "`nix flake lock` exited with code {}:\n{}",
            nix_lock_exit
                .status
                .code()
                .map(|x| x.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            std::str::from_utf8(&nix_lock_exit.stderr)?,
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
        .spawn()
        .wrap_err("Failed to spawn `nix develop`")?
        .wait_with_output()
        .await
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
    environment_variables: HashMap<String, String>,
    ld_library_path: HashSet<String>,
}

impl DevEnvironment {
    fn to_flake(&self) -> String {
        // TODO: use rnix for generating Nix?
        format!(
            include_str!("flake-template.inc"),
            build_inputs = self.build_inputs.iter().join(" "),
            environment_variables = self
                .environment_variables
                .iter()
                .map(|(name, value)| format!("\"{}\" = \"{}\";", name, value))
                .join("\n"),
            ld_library_path = if !self.ld_library_path.is_empty() {
                format!(
                    "export LD_LIBRARY_PATH=\"{}\"",
                    self.ld_library_path
                        .iter()
                        .map(|v| format!("${{lib.getLib {v}}}/lib"))
                        .join(":")
                )
            } else {
                "".to_string()
            }
        )
    }

    async fn detect(&mut self, project_dir: &Path) -> color_eyre::Result<()> {
        let mut any_found = false;

        if project_dir.join("Cargo.toml").exists() {
            self.add_deps_from_cargo(project_dir).await?;
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
    async fn add_deps_from_cargo(&mut self, project_dir: &Path) -> color_eyre::Result<()> {
        // We do this because of `clippy::type-complexity`
        struct KnownCrateRegistryValue {
            build_inputs: HashSet<&'static str>,
            environment_variables: HashMap<&'static str, &'static str>,
            ld_library_path_inputs: HashSet<&'static str>,
        }
        static KNOWN_CRATE_REGISTRY: Lazy<HashMap<&'static str, KnownCrateRegistryValue>> =
            Lazy::new(|| {
                let mut m = HashMap::new();
                macro_rules! crate_to_build_inputs {
                    ($collection:ident, $rust_package:expr, $build_inputs:expr) => {
                        crate_to_build_inputs!(
                            $collection,
                            $rust_package,
                            $build_inputs,
                            env = [],
                            ld = []
                        );
                    };
                    ($collection:ident, $rust_package:expr, $build_inputs:expr, env = $environment_variables:expr) => {
                        crate_to_build_inputs!(
                            $collection,
                            $rust_package,
                            $build_inputs,
                            env = $environment_variables,
                            ld = []
                        );
                    };
                    ($collection:ident, $rust_package:expr, $build_inputs:expr, ld = $ld_library_path_inputs:expr) => {
                        crate_to_build_inputs!(
                            $collection,
                            $rust_package,
                            $build_inputs,
                            env = [],
                            ld = $ld_library_path_inputs
                        );
                    };
                    ($collection:ident, $rust_package:expr, $build_inputs:expr, env = $environment_variables:expr, ld = $ld_library_path_inputs:expr) => {
                        $collection.insert(
                            $rust_package,
                            KnownCrateRegistryValue {
                                build_inputs: $build_inputs.into_iter().collect(),
                                environment_variables: $environment_variables.into_iter().collect(),
                                ld_library_path_inputs: $ld_library_path_inputs
                                    .into_iter()
                                    .collect(),
                            },
                        )
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
                crate_to_build_inputs!(
                    m,
                    "clang-sys",
                    ["llvmPackages.libclang", "llvm"],
                    env = [("LIBCLANG_PATH", "${llvmPackages.libclang.lib}/lib"),]
                );
                crate_to_build_inputs!(
                    m,
                    "winit",
                    ["xorg.libX11"],
                    ld = [
                        "xorg.libX11",
                        "xorg.libXcursor",
                        "xorg.libXrandr",
                        "xorg.libXi",
                        "libGL",
                        "glxinfo"
                    ]
                );
                m
            });

        tracing::debug!("Adding Cargo dependencies...");

        let mut found_build_inputs = HashSet::new();
        found_build_inputs.insert("rustc".to_string());
        found_build_inputs.insert("cargo".to_string());

        let mut cmd = Command::new("cargo");
        cmd.args(&["metadata", "--format-version", "1"]);
        cmd.arg("--manifest-path");
        cmd.arg(project_dir.join("Cargo.toml"));

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(eyre!("`cargo metadata` failed to execute"));
        }

        let stdout = std::str::from_utf8(&output.stdout)?;
        let metadata: CargoMetadata = serde_json::from_str(stdout)?;

        let mut found_envs = HashMap::new();
        let mut found_build_inputs = HashSet::new();
        let mut found_ld_inputs = HashSet::new();
        found_build_inputs.insert("rustc".to_string());
        found_build_inputs.insert("cargo".to_string());
        found_build_inputs.insert("rustfmt".to_string());

        for package in metadata.packages {
            let name = package.name;

            if let Some(KnownCrateRegistryValue {
                build_inputs: known_build_inputs,
                environment_variables: known_envs,
                ld_library_path_inputs: known_ld_inputs,
            }) = KNOWN_CRATE_REGISTRY.get(&*name)
            {
                let known_build_inputs = known_build_inputs
                    .iter()
                    .map(ToString::to_string)
                    .collect::<HashSet<_>>();

                let known_ld_inputs = known_ld_inputs
                    .iter()
                    .map(ToString::to_string)
                    .collect::<HashSet<_>>();
                tracing::debug!(
                    package_name = %name,
                    buildInputs = %known_build_inputs.iter().join(", "),
                    environment_variables = %known_envs.iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                    "Detected known crate information"
                );
                found_build_inputs = found_build_inputs
                    .union(&known_build_inputs)
                    .cloned()
                    .collect();

                for (known_key, known_value) in known_envs {
                    found_envs.insert(known_key.to_string(), known_value.to_string());
                }

                found_ld_inputs = found_ld_inputs
                    .union(&known_ld_inputs)
                    .map(|v| v.to_string())
                    .collect();
            }

            // Attempt to detect `package.fsm.build-inputs` in `Crate.toml`

            // TODO(@hoverbear): Add a `Deserializable` implementor we can get from this.
            let fsm_object = match package.metadata.get("fsm") {
                Some(fsm_object) => fsm_object,
                None => continue,
            };

            let package_build_inputs = match fsm_object.get("build-inputs") {
                Some(serde_json::Value::Object(build_inputs_table)) => {
                    let mut package_build_inputs = HashSet::new();
                    for (key, _value) in build_inputs_table.iter() {
                        // TODO(@hoverbear): Add version checking
                        package_build_inputs.insert(key.to_string());
                    }
                    package_build_inputs
                }
                Some(_) | None => Default::default(),
            };

            let package_envs = match fsm_object.get("environment-variables") {
                Some(serde_json::Value::Object(envs_table)) => {
                    let mut package_envs = HashMap::new();
                    for (key, value) in envs_table.iter() {
                        package_envs.insert(
                            key.to_string(),
                            value.as_str().ok_or(eyre!("`package.metadata.fsm.environment-variables` entries must have string values"))?.to_string()
                        );
                    }
                    package_envs
                }
                Some(_) | None => Default::default(),
            };

            let package_ld_inputs = match fsm_object.get("LD_LIBRARY_PATH-inputs") {
                Some(serde_json::Value::Object(ld_table)) => {
                    let mut package_ld_inputs = HashSet::new();
                    for (key, _value) in ld_table.iter() {
                        // TODO(@hoverbear): Add version checking
                        package_ld_inputs.insert(key.to_string());
                    }
                    package_ld_inputs
                }
                Some(_) | None => Default::default(),
            };

            tracing::debug!(
                package_name = %name,
                buildInputs = %package_build_inputs.iter().join(", "),
                environment_variables = %package_envs.iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                ld_library_path_inputs = %package_ld_inputs.iter().join(", "),
                "Detected `package.fsm` in `Crate.toml`"
            );
            found_build_inputs = found_build_inputs
                .union(&package_build_inputs)
                .cloned()
                .collect();
            for (package_env_key, package_env_value) in package_envs {
                found_envs.insert(package_env_key, package_env_value);
            }
        }

        eprintln!(
            "{check} {lang}: {colored_inputs}{maybe_colored_envs}",
            check = "✓".green(),
            lang = "🦀 rust".bold().red(),
            colored_inputs = {
                let mut sorted_build_inputs = found_build_inputs
                    .union(&found_ld_inputs)
                    .collect::<Vec<_>>();
                sorted_build_inputs.sort();
                sorted_build_inputs.iter().map(|v| v.cyan()).join(", ")
            },
            maybe_colored_envs = {
                if !found_envs.is_empty() {
                    let mut sorted_build_inputs =
                        found_envs.iter().map(|(k, _)| k).collect::<Vec<_>>();
                    sorted_build_inputs.sort();
                    format!(
                        " ({})",
                        sorted_build_inputs.iter().map(|v| v.green()).join(", ")
                    )
                } else {
                    "".to_string()
                }
            }
        );

        self.build_inputs = found_build_inputs;
        self.environment_variables = found_envs;
        self.ld_library_path = found_ld_inputs;

        Ok(())
    }
}
