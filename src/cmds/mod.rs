mod run;
mod shell;
mod print_dev_env;

use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum Commands {
    Shell(shell::Shell),
    Run(run::Run),
    PrintDevEnv(print_dev_env::PrintDevEnv),
}
