use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use eyre::WrapErr;
use tracing_error::ErrorLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use clap::{Args, Parser, Subcommand};

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
    let args = Cli::parse();

    match args.command {
        Commands::Shell(shell_args) => cmd_shell(shell_args).await,
    }
}

async fn cmd_shell(shell_args: Shell) -> color_eyre::Result<()> {
    let project_dir = get_project_dir(shell_args.project_dir);

    eprintln!("Project directory is '{}'.", project_dir.display());

    let mut dev_env = DevEnvironment::default();

    dev_env.detect(&project_dir)?;

    let flake_nix = dev_env.to_flake();

    eprint!("Generated 'flake.nix':\n{}", flake_nix);

    let flake_dir = std::env::temp_dir();

    let flake_nix_path = flake_dir.join("flake.nix");

    // FIXME: do async I/O?
    std::fs::write(&flake_nix_path, &flake_nix).expect("Unable to write flake.nix");

    Command::new("nix")
        .arg("develop")
        .arg("-L")
        .arg(format!(
            "path://{}",
            flake_dir.into_os_string().into_string().unwrap()
        ))
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Could not execute 'nix develop'."); // FIXME

    Ok(())
}

fn get_project_dir(project_dir: Option<PathBuf>) -> PathBuf {
    project_dir.unwrap_or_else(|| std::env::current_dir().unwrap())
}

#[derive(Default)]
struct DevEnvironment {
    build_inputs: Vec<String>,
}

impl DevEnvironment {
    fn to_flake(&self) -> String {
        // TODO: use rnix for generating Nix?
        format!(
            r#"{{
  outputs = {{ self, nixpkgs }}: {{
    devShells.x86_64-linux.default =
      with import nixpkgs {{ system = "x86_64-linux"; }};
      stdenv.mkDerivation {{
        name = "fsm-shell";
        buildInputs = [ {} ];
      }};
  }};
}}
"#,
            self.build_inputs.join(" ")
        )
    }

    fn detect(&mut self, project_dir: &Path) -> Result<(), std::io::Error> {
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

    fn add_deps_from_cargo(&mut self, project_dir: &Path) -> Result<(), std::io::Error> {
        eprintln!("Adding Cargo dependencies...");

        self.build_inputs.push("rustc".to_string());
        self.build_inputs.push("cargo".to_string());

        // FIXME: instead of calling 'cargo metadata', we could just
        // use the cargo crate here.

        // FIXME: chicken/egg problem: we need 'cargo' in $PATH first.
        let output = Command::new("cargo")
            .arg("metadata")
            .arg("--format-version")
            .arg("1")
            .arg("--manifest-path")
            .arg(project_dir.join("Cargo.toml"))
            .stderr(Stdio::inherit())
            .output()
            .expect("Could not execute 'cargo metadata'."); // FIXME

        if !output.status.success() {
            panic!("'cargo metadata' failed: {}", output.status);
        }

        let manifest: Manifest = serde_json::from_str(&String::from_utf8(output.stdout).unwrap())
            .expect("Could not parse 'cargo metadata' output.");

        let package_names: HashMap<_, _> = manifest
            .packages
            .into_iter()
            .map(|pkg| (pkg.name.clone(), pkg))
            .collect();

        if package_names.contains_key("expat-sys") {
            self.build_inputs.push("expat".to_string());
        }

        if package_names.contains_key("freetype-sys") {
            self.build_inputs.push("freetype".to_string());
        }

        if package_names.contains_key("servo-fontconfig-sys") {
            self.build_inputs.push("fontconfig".to_string());
        }

        if package_names.contains_key("libsqlite3-sys") {
            self.build_inputs.push("sqlite".to_string());
        }

        if package_names.contains_key("openssl-sys") {
            self.build_inputs.push("openssl".to_string());
        }

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Manifest {
    packages: Vec<Package>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Package {
    name: String,
    manifest_path: PathBuf,
}
