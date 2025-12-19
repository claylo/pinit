use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "pinit")]
#[command(about = "Apply project template baselines", long_about = None)]
#[command(version)]
pub struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short = 'v', long = "verbose", action = ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Config file path (overrides default discovery)
    #[arg(long = "config", global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Apply a template directory into a destination directory
    Apply(ApplyArgs),

    /// List available recipes/templates
    List,

    /// Create a new project directory from a recipe/template
    New(NewArgs),

    /// Print the CLI version
    Version,
}

#[derive(Args, Debug)]
pub struct ApplyArgs {
    /// Template/recipe name from config, or a path to a template directory
    pub template: String,

    /// Destination directory (default: current directory)
    pub dest_dir: Option<PathBuf>,

    /// Print what would change without writing
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,

    /// Non-interactive; apply the selected behavior to all files
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,

    /// When a file exists, overwrite it
    #[arg(long, conflicts_with_all = ["merge", "skip"])]
    pub overwrite: bool,

    /// When a file exists, attempt an additive merge (default)
    #[arg(long, conflicts_with_all = ["overwrite", "skip"])]
    pub merge: bool,

    /// When a file exists, skip it
    #[arg(long, conflicts_with_all = ["overwrite", "merge"])]
    pub skip: bool,
}

#[derive(Args, Debug)]
pub struct NewArgs {
    pub template: String,
    pub dir: PathBuf,

    /// Print what would change without writing
    #[arg(short = 'n', long = "dry-run")]
    pub dry_run: bool,

    /// Non-interactive; apply the selected behavior to all files
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,

    /// When a file exists, overwrite it
    #[arg(long, conflicts_with_all = ["merge", "skip"])]
    pub overwrite: bool,

    /// When a file exists, attempt an additive merge (default)
    #[arg(long, conflicts_with_all = ["overwrite", "skip"])]
    pub merge: bool,

    /// When a file exists, skip it
    #[arg(long, conflicts_with_all = ["overwrite", "merge"])]
    pub skip: bool,

    /// Initialize a git repository (default: on)
    #[arg(long = "git", action = ArgAction::SetTrue, conflicts_with = "no_git")]
    pub git: bool,

    /// Do not initialize a git repository
    #[arg(long = "no-git", action = ArgAction::SetTrue)]
    pub no_git: bool,

    /// Initial branch name (default: main)
    #[arg(long = "branch", default_value = "main")]
    pub branch: String,
}
