#![forbid(unsafe_code)]

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{Config, TemplateDef};

#[derive(Debug)]
pub enum ResolveError {
    NoHomeDir,
    UnknownTemplate(String),
    UnknownSource(String),
    TemplatePathNotDir(PathBuf),
    SourcePathMissing { source: String },
    SourceRepoMissing { source: String },
    GitCommandFailed { cmd: String, status: i32, stderr: String },
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::NoHomeDir => write!(f, "could not determine a cache directory"),
            ResolveError::UnknownTemplate(name) => write!(f, "unknown template: {name}"),
            ResolveError::UnknownSource(name) => write!(f, "unknown template source: {name}"),
            ResolveError::TemplatePathNotDir(path) => write!(f, "template path is not a directory: {}", path.display()),
            ResolveError::SourcePathMissing { source } => write!(f, "source '{source}' is missing 'path'"),
            ResolveError::SourceRepoMissing { source } => write!(f, "source '{source}' is missing 'repo'"),
            ResolveError::GitCommandFailed { cmd, status, stderr } => {
                write!(f, "git failed ({status}) running {cmd}: {stderr}")
            }
            ResolveError::Io { path, source } => write!(f, "{}: {}", path.display(), source),
        }
    }
}

impl std::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ResolveError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TemplateResolver {
    cache_dir: PathBuf,
}

impl TemplateResolver {
    pub fn with_default_cache() -> Result<Self, ResolveError> {
        let base = directories::BaseDirs::new().ok_or(ResolveError::NoHomeDir)?;
        Ok(Self { cache_dir: base.cache_dir().join("pinit") })
    }

    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn resolve_recipe_template_dirs(&self, cfg: &Config, recipe_or_template: &str) -> Result<Vec<PathBuf>, ResolveError> {
        let resolved = cfg
            .resolve_recipe(recipe_or_template)
            .ok_or_else(|| ResolveError::UnknownTemplate(recipe_or_template.to_string()))?;

        let mut out = Vec::new();
        for name in resolved.templates {
            out.push(self.resolve_template_dir(cfg, &name)?);
        }
        Ok(out)
    }

    pub fn resolve_template_dir(&self, cfg: &Config, template_name: &str) -> Result<PathBuf, ResolveError> {
        let def = cfg
            .templates
            .get(template_name)
            .ok_or_else(|| ResolveError::UnknownTemplate(template_name.to_string()))?;

        let path = self.resolve_template_def(cfg, template_name, def)?;
        ensure_is_dir(&path)?;
        Ok(path)
    }

    fn resolve_template_def(&self, cfg: &Config, template_name: &str, def: &TemplateDef) -> Result<PathBuf, ResolveError> {
        let path = def.path();
        if path.is_absolute() {
            return Ok(path.to_path_buf());
        }

        let Some(source_name) = def.source() else {
            return Err(ResolveError::UnknownSource(format!(
                "template '{template_name}' uses a relative path but has no source"
            )));
        };

        let source = cfg
            .sources
            .iter()
            .find(|s| s.name == source_name)
            .ok_or_else(|| ResolveError::UnknownSource(source_name.to_string()))?;

        if let Some(root) = &source.path {
            return Ok(root.join(path));
        }

        let Some(repo) = &source.repo else {
            return Err(ResolveError::SourceRepoMissing { source: source.name.clone() });
        };
        let git_ref = source.git_ref.as_deref().unwrap_or("HEAD");
        let repo_root = self.ensure_repo_checkout(repo, git_ref)?;
        let base = match &source.subdir {
            Some(subdir) => repo_root.join(subdir),
            None => repo_root,
        };

        Ok(base.join(path))
    }

    fn ensure_repo_checkout(&self, repo: &str, git_ref: &str) -> Result<PathBuf, ResolveError> {
        let key = cache_key(repo, git_ref);
        let repo_dir = self.cache_dir.join("repos").join(key).join("repo");

        if !repo_dir.exists() {
            fs::create_dir_all(repo_dir.parent().unwrap())
                .map_err(|e| ResolveError::Io { path: repo_dir.clone(), source: e })?;
            git(&["clone", repo, repo_dir.to_string_lossy().as_ref()], None)?;
        } else {
            // Best-effort update.
            let _ = git(&["-C", repo_dir.to_string_lossy().as_ref(), "fetch", "--tags", "--prune", "origin"], None);
        }

        // Check out the requested ref in a detached HEAD state. If `ref` is a branch name,
        // try `origin/<ref>` as a fallback.
        if git_checkout_detach(&repo_dir, git_ref).is_err() && !git_ref.contains('/') && !looks_like_hex(git_ref) {
            let origin_ref = format!("origin/{git_ref}");
            git_checkout_detach(&repo_dir, &origin_ref)?;
        }

        Ok(repo_dir)
    }
}

fn git_checkout_detach(repo_dir: &Path, git_ref: &str) -> Result<(), ResolveError> {
    git(
        &[
            "-C",
            repo_dir.to_string_lossy().as_ref(),
            "checkout",
            "--detach",
            "--force",
            git_ref,
        ],
        None,
    )
}

fn cache_key(repo: &str, git_ref: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(repo.as_bytes());
    hasher.update(b"\n");
    hasher.update(git_ref.as_bytes());
    let digest = hasher.finalize();
    digest.to_hex().to_string()
}

fn looks_like_hex(s: &str) -> bool {
    if s.len() < 7 {
        return false;
    }
    s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F'))
}

fn git(args: &[&str], cwd: Option<&Path>) -> Result<(), ResolveError> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let out = cmd.output().map_err(|e| ResolveError::Io { path: PathBuf::from("git"), source: e })?;
    if out.status.success() {
        return Ok(());
    }
    let status = out.status.code().unwrap_or(1);
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(ResolveError::GitCommandFailed { cmd: format!("git {}", args.join(" ")), status, stderr })
}

fn ensure_is_dir(path: &Path) -> Result<(), ResolveError> {
    let meta = fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ResolveError::TemplatePathNotDir(path.to_path_buf())
        } else {
            ResolveError::Io { path: path.to_path_buf(), source: e }
        }
    })?;

    if meta.file_type().is_symlink() {
        return Err(ResolveError::TemplatePathNotDir(path.to_path_buf()));
    }
    if !meta.is_dir() {
        return Err(ResolveError::TemplatePathNotDir(path.to_path_buf()));
    }
    Ok(())
}

pub fn path_is_git_dir(path: &Path) -> bool {
    path.join(".git").is_dir() || (path.file_name() == Some(OsStr::new(".git")))
}

