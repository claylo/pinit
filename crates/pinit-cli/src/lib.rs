#![forbid(unsafe_code)]

use clap::CommandFactory;

mod cli;

pub use cli::{ApplyArgs, Cli, Command, NewArgs, OverrideActionArg};

pub fn command() -> clap::Command {
    Cli::command()
}
