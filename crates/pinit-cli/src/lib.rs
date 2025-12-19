#![forbid(unsafe_code)]

use clap::CommandFactory;

mod cli;

pub use cli::{ApplyArgs, Cli, Command, NewArgs};

pub fn command() -> clap::Command {
    Cli::command()
}
