mod generate;
mod print_dev_env;
mod run;
mod shell;

use clap::Subcommand;
use eyre::WrapErr;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum Commands {
    Shell(shell::Shell),
    Run(run::Run),
    PrintDevEnv(print_dev_env::PrintDevEnv),
    Generate(generate::Generate),
}

pub fn get_project_dir(project_dir: &Option<PathBuf>) -> color_eyre::Result<PathBuf> {
    let project_dir = match project_dir {
        Some(dir) => dir.clone(),
        None => std::env::current_dir().wrap_err("Current working directory was invalid")?,
    };
    tracing::debug!("Project directory is '{}'.", project_dir.display());
    Ok(project_dir)
}
