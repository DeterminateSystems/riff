mod run;
mod shell;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum Commands {
    Shell(shell::Shell),
    Run(run::Run),
}
