use std::path::Path;

use clap::Parser;
use semver::Version;
use serde::Serialize;
use tokio::{fs::OpenOptions, io::{AsyncReadExt, AsyncWriteExt}, process::Command};
use uuid::Uuid;
use eyre::eyre;

use crate::{FSM_XDG_PREFIX, Cli};

static TELEMETRY_DISTINCT_ID_PATH: &str = "distinct_id";

#[derive(Debug, Serialize)]
pub(crate) struct Telemetry {
    /// Stored in `$XDG_DATA_HOME/fsm/distinct_id` as a UUIDv4
    distinct_id: Uuid,
    system_os: String,
    system_arch: String,
    /// The `CARGO_PGX_VERSION` from an `fsm` build
    fsm_version: String,
    /// The version output of `nix --version`
    nix_version: Option<Version>,
    /// If the exit code of `test -t 0` is 0, then this is true, otherwise false
    is_tty: bool,
    /// The command given to fsm (eg "shell")
    subcommand: Option<String>,
}

impl Telemetry {
    /// Create a new `Telemetry` without any pre-existing information
    /// 
    /// This is not very performant and may do things like re-invoke `nix` or reparse the `$ARG`s.
    pub(crate) async fn new() -> eyre::Result<Self> {
        let distinct_id = distinct_id().await?;

        let system_os = std::env::consts::OS.to_string();
        let system_arch = std::env::consts::ARCH.to_string();
        let fsm_version = env!("CARGO_PKG_VERSION").to_string();
        let nix_version = nix_version().await?;

        let is_tty = atty::is(atty::Stream::Stdout);

        let subcommand = {
            let cli = Cli::parse();
            // Only capture the command used, not the entire invocation of the command from bash, etc
            match cli.command {
                crate::cmds::Commands::Shell(_) => Some("shell".to_string()),
            }
        };

        Ok(Self {
            distinct_id,
            system_os,
            system_arch,
            fsm_version,
            nix_version,
            is_tty,
            subcommand,
        })
    }
}

async fn distinct_id() -> eyre::Result<Uuid> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix(FSM_XDG_PREFIX)?;
    let distinct_id_path = xdg_dirs.place_config_file(Path::new(TELEMETRY_DISTINCT_ID_PATH))?;
    
    let mut distinct_id_file = OpenOptions::new()
        .read(true)
        .write(true)
        .truncate(false)
        .create(true) // We do this proactively to avoid the user seeing a non-fatal error later when we freshen the cache.
        .open(distinct_id_path.clone())
        .await?;
    let mut distinct_id = Default::default();
    distinct_id_file
        .read_to_string(&mut distinct_id)
        .await?;

    let distinct_id = if distinct_id.is_empty() {
        let distinct_id = Uuid::new_v4();
        tracing::debug!(%distinct_id, "Writing new distinct ID");
        distinct_id_file.write_all(distinct_id.as_bytes()).await?;
        distinct_id
    } else {
        Uuid::from_slice(distinct_id.as_bytes())?
    };

    Ok(distinct_id)
}

async fn nix_version() -> eyre::Result<Option<Version>> {
    let mut command = Command::new("nix");
    command.arg("--version");
    let output = command.output().await;
    match output {
        Ok(output) => {
            if output.status.success() {
                let stdout = output.stdout;
                let mut stdout_string = std::str::from_utf8(&stdout)?.to_string();
                let last_space = stdout_string.rfind(" ").ok_or(eyre!("Unexpected `nix --version` string"))?;
                stdout_string = stdout_string.split_off(last_space);
                let version = Version::parse(stdout_string.trim())?;
                Ok(Some(version))
            } else {
                Err(eyre!("`nix --version` failed to run for telemetry"))
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            tracing::trace!("Could not run `nix --version` due to `EPERM`, this is likely because `nix` is not installed");
            Ok(None)
        },
        Err(err) => Err(err.into()),
    }

}