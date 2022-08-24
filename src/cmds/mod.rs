mod shell;

use clap::Subcommand;

#[derive(Debug, Subcommand, Clone)]
pub enum Commands {
    Shell(shell::Shell),
}
