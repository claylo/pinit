#![forbid(unsafe_code)]

use std::path::PathBuf;

use clap::{Args, CommandFactory, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "pinit")]
#[command(about = "Apply project template baselines", long_about = None)]
struct Cli {
    /// Config file path (overrides default discovery)
    #[arg(long = "config", global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Apply a template directory into a destination directory
    Apply(ApplyArgs),

    /// List available recipes/templates (not implemented yet)
    List,

    /// Create a new project directory from a recipe/template (not implemented yet)
    New(NewArgs),
}

#[derive(Args, Debug)]
struct ApplyArgs {
    /// Template directory to apply
    template_dir: PathBuf,

    /// Destination directory (default: current directory)
    dest_dir: Option<PathBuf>,

    /// Print what would change without writing
    #[arg(short = 'n', long = "dry-run")]
    dry_run: bool,
}

#[derive(Args, Debug)]
struct NewArgs {
    template: String,
    dir: PathBuf,
}

fn main() {
    let cli = Cli::parse();

    let Some(command) = cli.command else {
        let _ = Cli::command().print_help();
        println!();
        std::process::exit(2);
    };

    let result = match command {
        Command::Apply(args) => cmd_apply(args),
        Command::List => cmd_list(cli.config.as_deref()),
        Command::New(_) => Err("new is not implemented yet".to_string()),
    };

    if let Err(message) = result {
        eprintln!("error: {message}");
        std::process::exit(1);
    }
}

fn cmd_apply(args: ApplyArgs) -> Result<(), String> {
    let dest_dir = args.dest_dir.unwrap_or_else(|| PathBuf::from("."));
    let report = pinit_core::apply_template_dir(
        &args.template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: args.dry_run },
    )
    .map_err(|e| e.to_string())?;

    if args.dry_run {
        println!(
            "dry-run: would create {} file(s), skip {} file(s)",
            report.created_files, report.skipped_files
        );
    } else {
        println!(
            "created {} file(s), skipped {} file(s)",
            report.created_files, report.skipped_files
        );
    }
    Ok(())
}

fn cmd_list(config_path: Option<&std::path::Path>) -> Result<(), String> {
    match pinit_core::config::load_config(config_path) {
        Ok((path, cfg)) => {
            println!("config: {}", path.display());

            if !cfg.templates.is_empty() {
                println!("\ntemplates:");
                for (name, def) in &cfg.templates {
                    let source = def.source().unwrap_or("-");
                    println!("  {name} (source: {source}, path: {})", def.path().display());
                }
            }

            if !cfg.targets.is_empty() {
                println!("\ntargets:");
                for (name, stack) in &cfg.targets {
                    println!("  {name} = {}", stack.join(" + "));
                }
            }

            if !cfg.recipes.is_empty() {
                println!("\nrecipes:");
                for (name, recipe) in &cfg.recipes {
                    let tmpl = if recipe.templates.is_empty() {
                        "-".to_string()
                    } else {
                        recipe.templates.join(" + ")
                    };
                    println!("  {name} (templates: {tmpl}, filesets: {})", recipe.files.len());
                }
            }

            Ok(())
        }
        Err(pinit_core::config::ConfigError::NotFound) => {
            println!("no config found");
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}
