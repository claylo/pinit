#![forbid(unsafe_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "xtask")]
#[command(about = "Project maintenance tasks")]
struct Xtask {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand, Debug)]
enum Task {
    /// Generate a manpage for the pinit CLI.
    Man {
        /// Output directory (default: target/man)
        #[arg(long = "out-dir", default_value = "target/man")]
        out_dir: PathBuf,
    },

    /// Build and install the pinit CLI into ~/.bin for local testing.
    Install {
        /// Destination directory for the installed binary (default: ~/.bin)
        #[arg(long = "bin-dir", default_value = "~/.bin")]
        bin_dir: String,

        /// Cargo profile to build (default: release)
        #[arg(long = "profile", default_value = "release")]
        profile: String,
    },
}

fn main() -> Result<(), String> {
    let task = Xtask::parse();
    match task.command {
        Task::Man { out_dir } => generate_manpage(&out_dir),
        Task::Install { bin_dir, profile } => install_cli(&bin_dir, &profile),
    }
}

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap_or(&manifest_dir).to_path_buf()
}

fn generate_manpage(out_dir: &Path) -> Result<(), String> {
    let out_dir = workspace_root().join(out_dir);
    fs::create_dir_all(&out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;

    let cmd = pinit_cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut buffer: Vec<u8> = Vec::new();
    man.render(&mut buffer).map_err(|e| format!("render manpage: {e}"))?;

    let man_path = out_dir.join("pinit.1");
    fs::write(&man_path, buffer).map_err(|e| format!("{}: {e}", man_path.display()))?;
    println!("wrote {}", man_path.display());
    Ok(())
}

fn install_cli(bin_dir: &str, profile: &str) -> Result<(), String> {
    let bin_dir = expand_tilde(bin_dir)?;
    fs::create_dir_all(&bin_dir).map_err(|e| format!("{}: {e}", bin_dir.display()))?;

    let root = workspace_root();
    let status = build_cli(&root, profile)?;
    if !status.success() {
        return Err(format!("cargo build failed with status {}", status.code().unwrap_or(1)));
    }

    let bin_path = built_binary(&root, profile);
    if !bin_path.exists() {
        return Err(format!("built binary not found at {}", bin_path.display()));
    }

    let dest = bin_dir.join("pinit");
    fs::copy(&bin_path, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    set_executable(&dest)?;
    println!("installed {}", dest.display());
    Ok(())
}

fn build_cli(root: &Path, profile: &str) -> Result<std::process::ExitStatus, String> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build").arg("-p").arg("pinit-cli").current_dir(root);
    if profile == "release" {
        cmd.arg("--release");
    } else {
        cmd.arg("--profile").arg(profile);
    }
    cmd.status().map_err(|e| format!("failed to run cargo: {e}"))
}

fn built_binary(root: &Path, profile: &str) -> PathBuf {
    if profile == "release" {
        root.join("target").join("release").join("pinit")
    } else {
        root.join("target").join(profile).join("pinit")
    }
}

fn expand_tilde(path: &str) -> Result<PathBuf, String> {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
        return Ok(PathBuf::from(home).join(rest));
    }
    if path == "~" {
        let home = std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
        return Ok(PathBuf::from(home));
    }
    Ok(PathBuf::from(path))
}

fn set_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}
