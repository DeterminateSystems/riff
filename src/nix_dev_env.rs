use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Stdio;

use eyre::WrapErr;
use owo_colors::OwoColorize;
use serde::Deserialize;
use tokio::process::Command;

pub async fn get_nix_dev_env(flake_dir: &Path) -> color_eyre::Result<NixDevEnv> {
    let output = get_raw_nix_dev_env(flake_dir).await?;

    serde_json::from_str(&output).wrap_err(
        "Unable to parse output produced by `nix print-dev-env` into our desired structure",
    )
}

/// The output schema of `nix print-dev-env --json`.
#[derive(Debug, Clone, Deserialize)]
pub struct NixDevEnv {
    variables: HashMap<String, Variable>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Variable {
    #[serde(rename = "exported")]
    Exported(String),
    #[serde(rename = "var")]
    Var(String),
    #[serde(rename = "array")]
    Array(Vec<String>),
    #[serde(rename = "associative")]
    Associative(HashMap<String, String>),
}

pub async fn get_raw_nix_dev_env(flake_dir: &Path) -> color_eyre::Result<String> {
    let mut nix_command = Command::new("nix");
    nix_command
        .arg("print-dev-env")
        .arg("--json")
        .args(&["--extra-experimental-features", "flakes nix-command"])
        .arg("-L")
        .arg(format!("path://{}", flake_dir.to_str().unwrap()))
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    tracing::trace!(command = ?nix_command.as_std(), "Running");

    // TODO(@hoverbear): Try to enable this somehow. Right now since we don't keep the lock
    // in a consistent place, we can't reliably pick up a lock generated in online mode.
    //
    // If we stored the generated flake/lock in a consistent place this could be enabled.
    //
    // if self.offline {
    //     nix_develop_command.arg("--offline");
    // }

    let nix_command_exit = match nix_command
        .spawn()
        .wrap_err("Failed to spawn `nix develop`")? // This could throw a `EWOULDBLOCK`
        .wait_with_output()
        .await
    {
        Ok(nix_command_exit) => nix_command_exit,
        err @ Err(_) => {
            let wrapped_err = err
                .wrap_err_with(|| {
                    format!(
                        "\
                        Could not execute `{nix_print_dev_env}`. Is `{nix}` installed?\n\n\
                        Get instructions for installing Nix: {nix_install_url}\n\
                        Underlying error\
                        ",
                        nix_print_dev_env = "nix print-dev-env".cyan(),
                        nix = "nix".cyan(),
                        nix_install_url = "https://nixos.org/download.html".blue().underline(),
                    )
                })
                .unwrap_err();
            eprintln!("{wrapped_err:#}");
            std::process::exit(1);
        }
    };

    String::from_utf8(nix_command_exit.stdout)
        .wrap_err("Output produced by `nix print-dev-env` was not valid UTF8")
}

pub async fn run_in_dev_env(
    dev_env: &NixDevEnv,
    command_name: &str,
) -> color_eyre::Result<Command> {
    let mut command = Command::new(&command_name);

    // TODO(@edolstra): Copied from develop.cc, would be nice to
    // keep these in sync somehow (e.g. `nix print-dev-env --json`
    // could output them).
    let prepended_vars = HashSet::from(["PATH".to_owned(), "XDG_DATA_DIRS".to_owned()]);

    let ignored_vars = HashSet::from(
        [
            "BASHOPTS",
            "HOME",
            "NIX_BUILD_TOP",
            "NIX_ENFORCE_PURITY",
            "NIX_LOG_FD",
            "NIX_REMOTE",
            "PPID",
            "SHELLOPTS",
            "SSL_CERT_FILE",
            "TEMP",
            "TEMPDIR",
            "TERM",
            "TMP",
            "TMPDIR",
            "TZ",
            "UID",
        ]
        .map(str::to_owned),
    );

    for (name, value) in &dev_env.variables {
        if let Variable::Exported(value) = value {
            if ignored_vars.contains(name) {
                continue;
            }
            let mut value = value.clone();
            if prepended_vars.contains(name) {
                if let Ok(old_value) = std::env::var(&name) {
                    value = format!("{}:{}", value, old_value);
                }
            }
            command.env(name, value);
        }
    }

    // Increment $IN_RIFF.
    command.env(
        "IN_RIFF",
        (std::env::var("IN_RIFF")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
            + 1)
        .to_string(),
    );

    Ok(command)
}

pub async fn get_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .unwrap_or_else(|| "bash".to_owned())
}
