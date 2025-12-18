#![forbid(unsafe_code)]

use std::path::PathBuf;

use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "pinit")]
#[command(about = "Apply project template baselines", long_about = None)]
struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short = 'v', long = "verbose", action = ArgAction::Count, global = true)]
    verbose: u8,

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
    /// Template/recipe name from config, or a path to a template directory
    template: String,

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
    init_tracing(cli.verbose);

    let Some(command) = cli.command else {
        let _ = Cli::command().print_help();
        println!();
        std::process::exit(2);
    };

    let result = match command {
        Command::Apply(args) => cmd_apply(cli.config.as_deref(), args),
        Command::List => cmd_list(cli.config.as_deref()),
        Command::New(_) => Err("new is not implemented yet".to_string()),
    };

    if let Err(message) = result {
        eprintln!("error: {message}");
        std::process::exit(1);
    }
}

fn init_tracing(verbosity: u8) {
    let default_level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    let default_filter = format!("warn,pinit_cli={default_level},pinit_core={default_level}");

    let filter = EnvFilter::try_from_env("PINIT_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_level(true)
        .compact()
        .init();
}

fn cmd_apply(config_path: Option<&std::path::Path>, args: ApplyArgs) -> Result<(), String> {
    let dest_for_log = args
        .dest_dir
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new("."))
        .display()
        .to_string();
    tracing::debug!(template = %args.template, dest_dir = %dest_for_log, dry_run = args.dry_run, "apply");

    let dest_dir = args.dest_dir.unwrap_or_else(|| PathBuf::from("."));

    let template_path = PathBuf::from(&args.template);
    let template_dirs: Vec<PathBuf> = if template_path.is_dir() {
        vec![template_path]
    } else {
        let (_path, cfg) = pinit_core::config::load_config(config_path).map_err(|e| e.to_string())?;
        let resolver = pinit_core::resolve::TemplateResolver::with_default_cache().map_err(|e| e.to_string())?;
        resolver
            .resolve_recipe_template_dirs(&cfg, &args.template)
            .map_err(|e| e.to_string())?
    };

    let mut created = 0usize;
    let mut skipped = 0usize;
    for dir in template_dirs {
        tracing::info!(template_dir = %dir.display(), "apply template dir");
        let report = pinit_core::apply_template_dir(&dir, &dest_dir, pinit_core::ApplyOptions { dry_run: args.dry_run })
            .map_err(|e| e.to_string())?;
        created += report.created_files;
        skipped += report.skipped_files;
        // ignored paths are intentionally omitted from the default summary for now.
    }

    if args.dry_run {
        println!("dry-run: would create {} file(s), skip {} file(s)", created, skipped);
    } else {
        println!("created {} file(s), skipped {} file(s)", created, skipped);
    }
    Ok(())
}

fn cmd_list(config_path: Option<&std::path::Path>) -> Result<(), String> {
    match pinit_core::config::load_config(config_path) {
        Ok((path, cfg)) => {
            tracing::debug!(config = %path.display(), "loaded config");
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
