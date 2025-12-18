#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand};
use pinit_core::{ExistingFileAction, ExistingFileDecider, ExistingFileDecisionContext};
use similar::TextDiff;
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

    /// List available recipes/templates
    List,

    /// Create a new project directory from a recipe/template
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

    /// Non-interactive; apply the selected behavior to all files
    #[arg(short = 'y', long = "yes")]
    yes: bool,

    /// When a file exists, overwrite it
    #[arg(long, conflicts_with_all = ["merge", "skip"])]
    overwrite: bool,

    /// When a file exists, attempt an additive merge (default)
    #[arg(long, conflicts_with_all = ["overwrite", "skip"])]
    merge: bool,

    /// When a file exists, skip it
    #[arg(long, conflicts_with_all = ["overwrite", "merge"])]
    skip: bool,
}

#[derive(Args, Debug)]
struct NewArgs {
    template: String,
    dir: PathBuf,

    /// Print what would change without writing
    #[arg(short = 'n', long = "dry-run")]
    dry_run: bool,

    /// Non-interactive; apply the selected behavior to all files
    #[arg(short = 'y', long = "yes")]
    yes: bool,

    /// When a file exists, overwrite it
    #[arg(long, conflicts_with_all = ["merge", "skip"])]
    overwrite: bool,

    /// When a file exists, attempt an additive merge (default)
    #[arg(long, conflicts_with_all = ["overwrite", "skip"])]
    merge: bool,

    /// When a file exists, skip it
    #[arg(long, conflicts_with_all = ["overwrite", "merge"])]
    skip: bool,

    /// Initialize a git repository (default: on)
    #[arg(long = "git", action = ArgAction::SetTrue, conflicts_with = "no_git")]
    git: bool,

    /// Do not initialize a git repository
    #[arg(long = "no-git", action = ArgAction::SetTrue)]
    no_git: bool,

    /// Initial branch name (default: main)
    #[arg(long = "branch", default_value = "main")]
    branch: String,
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
        Command::New(args) => cmd_new(cli.config.as_deref(), args),
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

    let default_action = if args.overwrite {
        ExistingFileAction::Overwrite
    } else if args.skip {
        ExistingFileAction::Skip
    } else {
        ExistingFileAction::Merge
    };

    let mut decider = CliDecider::new(default_action, args.yes || args.overwrite || args.merge || args.skip);

    let report = apply_template_stack(
        config_path,
        &args.template,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: args.dry_run },
        &mut decider,
    )?;

    print_apply_summary(args.dry_run, report);
    Ok(())
}

fn cmd_new(config_path: Option<&std::path::Path>, args: NewArgs) -> Result<(), String> {
    tracing::debug!(
        template = %args.template,
        dir = %args.dir.display(),
        dry_run = args.dry_run,
        git = %(args.no_git == false),
        branch = %args.branch,
        "new"
    );

    if args.dry_run {
        let default_action = if args.overwrite {
            ExistingFileAction::Overwrite
        } else if args.skip {
            ExistingFileAction::Skip
        } else {
            ExistingFileAction::Merge
        };

        let mut decider = CliDecider::new(default_action, true);
        let report = apply_template_stack(
            config_path,
            &args.template,
            &args.dir,
            pinit_core::ApplyOptions { dry_run: true },
            &mut decider,
        )?;

        eprintln!("dry-run: would create directory {}", args.dir.display());
        if args.no_git {
            eprintln!("dry-run: would skip git init");
        } else {
            eprintln!("dry-run: would run git init (branch {})", args.branch);
        }
        print_apply_summary(true, report);
        return Ok(());
    }

    if args.dir.exists() {
        let meta = std::fs::metadata(&args.dir).map_err(|e| format!("{}: {e}", args.dir.display()))?;
        if !meta.is_dir() {
            return Err(format!("destination is not a directory: {}", args.dir.display()));
        }
        let mut iter = std::fs::read_dir(&args.dir).map_err(|e| format!("{}: {e}", args.dir.display()))?;
        if iter.next().is_some() {
            return Err(format!("destination already exists and is not empty: {}", args.dir.display()));
        }
    } else {
        std::fs::create_dir_all(&args.dir).map_err(|e| format!("{}: {e}", args.dir.display()))?;
    }

    if !args.no_git {
        git_init(&args.dir, &args.branch)?;
    }

    let default_action = if args.overwrite {
        ExistingFileAction::Overwrite
    } else if args.skip {
        ExistingFileAction::Skip
    } else {
        ExistingFileAction::Merge
    };

    let mut decider = CliDecider::new(default_action, args.yes || args.overwrite || args.merge || args.skip);
    let report = apply_template_stack(
        config_path,
        &args.template,
        &args.dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )?;

    print_apply_summary(false, report);
    Ok(())
}

fn apply_template_stack(
    config_path: Option<&std::path::Path>,
    template: &str,
    dest_dir: &std::path::Path,
    options: pinit_core::ApplyOptions,
    decider: &mut dyn ExistingFileDecider,
) -> Result<pinit_core::ApplyReport, String> {
    let template_dirs = resolve_template_dirs(config_path, template)?;

    let mut report = pinit_core::ApplyReport::default();
    for dir in template_dirs {
        tracing::info!(template_dir = %dir.display(), "apply template dir");
        let r = pinit_core::apply_template_dir(&dir, dest_dir, options, decider).map_err(|e| e.to_string())?;
        report.created_files += r.created_files;
        report.updated_files += r.updated_files;
        report.skipped_files += r.skipped_files;
        report.ignored_paths += r.ignored_paths;
    }
    Ok(report)
}

fn resolve_template_dirs(config_path: Option<&std::path::Path>, template: &str) -> Result<Vec<PathBuf>, String> {
    let template_path = PathBuf::from(template);
    if template_path.is_dir() {
        return Ok(vec![template_path]);
    }

    let (_path, cfg) = pinit_core::config::load_config(config_path).map_err(|e| e.to_string())?;
    let resolver = pinit_core::resolve::TemplateResolver::with_default_cache().map_err(|e| e.to_string())?;
    resolver
        .resolve_recipe_template_dirs(&cfg, template)
        .map_err(|e| e.to_string())
}

fn print_apply_summary(dry_run: bool, report: pinit_core::ApplyReport) {
    if dry_run {
        println!(
            "dry-run: would create {} file(s), update {} file(s), skip {} file(s)",
            report.created_files, report.updated_files, report.skipped_files
        );
    } else {
        println!(
            "created {} file(s), updated {} file(s), skipped {} file(s)",
            report.created_files, report.updated_files, report.skipped_files
        );
    }
}

fn git_init(dir: &std::path::Path, branch: &str) -> Result<(), String> {
    tracing::info!(dir = %dir.display(), branch = %branch, "git init");

    let mut cmd = ProcessCommand::new("git");
    cmd.arg("init").arg("--initial-branch").arg(branch).current_dir(dir);
    match cmd.output() {
        Ok(out) if out.status.success() => return Ok(()),
        Ok(out) => {
            tracing::debug!(
                status = ?out.status.code(),
                stdout = %String::from_utf8_lossy(&out.stdout),
                stderr = %String::from_utf8_lossy(&out.stderr),
                "git init --initial-branch failed; falling back"
            );
        }
        Err(e) => return Err(format!("failed to run git: {e}")),
    }

    let out = ProcessCommand::new("git")
        .arg("init")
        .current_dir(dir)
        .output()
        .map_err(|e| format!("failed to run git init: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git init failed ({}): {}",
            out.status.code().unwrap_or(1),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    // Ensure the initial branch is as requested even on older git versions.
    let out = ProcessCommand::new("git")
        .arg("checkout")
        .arg("-B")
        .arg(branch)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("failed to run git checkout: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git checkout -B {branch} failed ({}): {}",
            out.status.code().unwrap_or(1),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    Ok(())
}

struct CliDecider {
    default_action: ExistingFileAction,
    non_interactive: bool,
}

impl CliDecider {
    fn new(default_action: ExistingFileAction, non_interactive: bool) -> Self {
        Self { default_action, non_interactive }
    }

    fn prompt(&self, ctx: &ExistingFileDecisionContext<'_>) -> ExistingFileAction {
        let rel = ctx.rel_path.display();
        let merge_available = ctx.merge_bytes.is_some();

        loop {
            eprintln!();
            eprintln!("file exists: {rel}");
            eprintln!("merge available: {}", if merge_available { "yes" } else { "no" });
            eprintln!("choose: (m)erge, (o)verwrite, (s)kip, (d)iff  [default: m]");
            eprint!("> ");
            {
                use std::io::Write;
                let mut stderr = std::io::stderr();
                let _ = stderr.flush();
            }

            let mut line = String::new();
            {
                use std::io::BufRead;
                let stdin = std::io::stdin();
                let mut lock = stdin.lock();
                if lock.read_line(&mut line).is_err() {
                    return ExistingFileAction::Skip;
                }
            }
            let choice = line.trim().to_ascii_lowercase();

            match choice.as_str() {
                "" | "m" => {
                    if merge_available {
                        return ExistingFileAction::Merge;
                    }
                    eprintln!("merge is unavailable for this file; choose overwrite or skip.");
                }
                "o" => return ExistingFileAction::Overwrite,
                "s" => return ExistingFileAction::Skip,
                "d" => {
                    self.print_diffs(ctx);
                }
                _ => eprintln!("unknown choice: {choice}"),
            }
        }
    }

    fn print_diffs(&self, ctx: &ExistingFileDecisionContext<'_>) {
        let rel = ctx.rel_path.display();
        eprintln!();
        eprintln!("diffs for {rel}:");
        eprintln!();

        if let Some(merge) = ctx.merge_bytes {
            eprintln!("--- merge");
            print_unified_diff("dest", "merged", ctx.dest_bytes, merge);
        } else {
            eprintln!("--- merge (unavailable)");
        }

        eprintln!();
        eprintln!("--- overwrite");
        print_unified_diff("dest", "template", ctx.dest_bytes, ctx.src_bytes);
        eprintln!();
    }
}

impl ExistingFileDecider for CliDecider {
    fn decide(&mut self, ctx: ExistingFileDecisionContext<'_>) -> ExistingFileAction {
        if self.non_interactive {
            if self.default_action == ExistingFileAction::Merge && ctx.merge_bytes.is_none() {
                return ExistingFileAction::Skip;
            }
            return self.default_action;
        }
        self.prompt(&ctx)
    }
}

fn print_unified_diff(old_label: &str, new_label: &str, old_bytes: &[u8], new_bytes: &[u8]) {
    const MAX_BYTES: usize = 200_000;
    if old_bytes.len() > MAX_BYTES || new_bytes.len() > MAX_BYTES {
        eprintln!("(diff too large: {} → {} bytes)", old_bytes.len(), new_bytes.len());
        return;
    }

    let Ok(old) = std::str::from_utf8(old_bytes) else {
        eprintln!("(binary dest; {} bytes)", old_bytes.len());
        return;
    };
    let Ok(new) = std::str::from_utf8(new_bytes) else {
        eprintln!("(binary template/merged; {} bytes)", new_bytes.len());
        return;
    };

    let diff = TextDiff::from_lines(old, new)
        .unified_diff()
        .header(old_label, new_label)
        .to_string();

    if diff.trim().is_empty() {
        eprintln!("(no textual changes)");
    } else {
        eprint!("{diff}");
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn make_temp_root() -> TempRoot {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("pinit-cli-new-test-{}-{n}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempRoot(path)
    }

    struct TempRoot(PathBuf);

    impl TempRoot {
        fn join(&self, path: impl AsRef<Path>) -> PathBuf {
            self.0.join(path)
        }
    }

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn new_dry_run_does_not_create_dir() {
        let root = make_temp_root();
        let template_dir = root.join("template");
        let dest = root.join("proj");

        fs::create_dir_all(&template_dir).unwrap();
        fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

        cmd_new(
            None,
            NewArgs {
                template: template_dir.to_string_lossy().to_string(),
                dir: dest.clone(),
                dry_run: true,
                yes: true,
                overwrite: false,
                merge: false,
                skip: false,
                git: false,
                no_git: true,
                branch: "main".to_string(),
            },
        )
        .unwrap();

        assert!(!dest.exists());
    }

    #[test]
    fn new_creates_dir_and_applies_without_git_when_no_git() {
        let root = make_temp_root();
        let template_dir = root.join("template");
        let dest = root.join("proj");

        fs::create_dir_all(&template_dir).unwrap();
        fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

        cmd_new(
            None,
            NewArgs {
                template: template_dir.to_string_lossy().to_string(),
                dir: dest.clone(),
                dry_run: false,
                yes: true,
                overwrite: false,
                merge: false,
                skip: false,
                git: false,
                no_git: true,
                branch: "main".to_string(),
            },
        )
        .unwrap();

        assert!(dest.is_dir());
        assert_eq!(fs::read_to_string(dest.join("hello.txt")).unwrap(), "hello\n");
        assert!(!dest.join(".git").exists());
    }

    #[test]
    fn new_inits_git_by_default_on_main_branch() {
        let root = make_temp_root();
        let template_dir = root.join("template");
        let dest = root.join("proj");

        fs::create_dir_all(&template_dir).unwrap();
        fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

        cmd_new(
            None,
            NewArgs {
                template: template_dir.to_string_lossy().to_string(),
                dir: dest.clone(),
                dry_run: false,
                yes: true,
                overwrite: false,
                merge: false,
                skip: false,
                git: false,
                no_git: false,
                branch: "main".to_string(),
            },
        )
        .unwrap();

        assert!(dest.join(".git").is_dir());
        let out = ProcessCommand::new("git")
            .arg("symbolic-ref")
            .arg("--short")
            .arg("HEAD")
            .current_dir(&dest)
            .output()
            .unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "main");
    }
}
