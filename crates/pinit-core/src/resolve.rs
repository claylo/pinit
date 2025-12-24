#![forbid(unsafe_code)]

//! Template source resolution for pinit.
//!
//! Resolves template names into local paths, fetching git sources into a cache
//! directory when needed.

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{Config, GitProtocol, TemplateDef};

use tracing::{debug, instrument};

/// Errors encountered while resolving template sources.
#[derive(Debug)]
pub enum ResolveError {
    NoHomeDir,
    UnknownTemplate(String),
    UnknownSource(String),
    TemplatePathNotDir(PathBuf),
    SourcePathMissing {
        source: String,
    },
    SourceRepoMissing {
        source: String,
    },
    GitCommandFailed {
        cmd: String,
        status: i32,
        stderr: String,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::NoHomeDir => write!(f, "could not determine a cache directory"),
            ResolveError::UnknownTemplate(name) => write!(f, "unknown template: {name}"),
            ResolveError::UnknownSource(name) => write!(f, "unknown template source: {name}"),
            ResolveError::TemplatePathNotDir(path) => {
                write!(f, "template path is not a directory: {}", path.display())
            }
            ResolveError::SourcePathMissing { source } => {
                write!(f, "source '{source}' is missing 'path'")
            }
            ResolveError::SourceRepoMissing { source } => {
                write!(f, "source '{source}' is missing 'repo'")
            }
            ResolveError::GitCommandFailed {
                cmd,
                status,
                stderr,
            } => {
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

/// Resolver for template directories with optional git-backed sources.
#[derive(Clone, Debug)]
pub struct TemplateResolver {
    cache_dir: PathBuf,
}

/// Resolved template entry with its name and local directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTemplate {
    pub name: String,
    pub dir: PathBuf,
    pub index: usize,
}

impl TemplateResolver {
    pub fn with_default_cache() -> Result<Self, ResolveError> {
        let base = directories::BaseDirs::new().ok_or(ResolveError::NoHomeDir)?;
        Ok(Self {
            cache_dir: base.cache_dir().join("pinit"),
        })
    }

    /// Create a resolver using an explicit cache directory.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Return the cache directory path.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Resolve a recipe name to a list of template directories.
    pub fn resolve_recipe_template_dirs(
        &self,
        cfg: &Config,
        recipe_or_template: &str,
    ) -> Result<Vec<PathBuf>, ResolveError> {
        let entries = self.resolve_recipe_templates(cfg, recipe_or_template)?;
        Ok(entries.into_iter().map(|entry| entry.dir).collect())
    }

    /// Resolve a recipe name to a list of template directories with metadata.
    pub fn resolve_recipe_templates(
        &self,
        cfg: &Config,
        recipe_or_template: &str,
    ) -> Result<Vec<ResolvedTemplate>, ResolveError> {
        let resolved = cfg
            .resolve_recipe(recipe_or_template)
            .ok_or_else(|| ResolveError::UnknownTemplate(recipe_or_template.to_string()))?;

        debug!(
            name = recipe_or_template,
            templates = resolved.templates.len(),
            "resolve recipe"
        );
        let mut out = Vec::new();
        for (index, name) in resolved.templates.into_iter().enumerate() {
            let dir = self.resolve_template_dir(cfg, &name)?;
            out.push(ResolvedTemplate { name, dir, index });
        }
        Ok(out)
    }

    #[instrument(skip_all, fields(template = template_name))]
    /// Resolve a single template name to its directory on disk.
    pub fn resolve_template_dir(
        &self,
        cfg: &Config,
        template_name: &str,
    ) -> Result<PathBuf, ResolveError> {
        let def = cfg
            .templates
            .get(template_name)
            .ok_or_else(|| ResolveError::UnknownTemplate(template_name.to_string()))?;

        let path = self.resolve_template_def(cfg, template_name, def)?;
        ensure_is_dir(&path)?;
        Ok(path)
    }

    fn resolve_template_def(
        &self,
        cfg: &Config,
        template_name: &str,
        def: &TemplateDef,
    ) -> Result<PathBuf, ResolveError> {
        let path = def.path();
        if path.is_absolute() {
            debug!(template = template_name, path = %path.display(), "resolve absolute");
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
            debug!(template = template_name, source = source_name, root = %root.display(), path = %path.display(), "resolve local");
            return Ok(root.join(path));
        }

        let Some(repo) = &source.repo else {
            return Err(ResolveError::SourceRepoMissing {
                source: source.name.clone(),
            });
        };
        let repo = normalize_repo(repo, source.git_protocol.unwrap_or(GitProtocol::Ssh));
        let git_ref = source.git_ref.as_deref().unwrap_or("HEAD");
        debug!(template = template_name, source = source_name, repo = %repo, git_ref = %git_ref, "resolve git");
        let repo_root = self.ensure_repo_checkout(&repo, git_ref)?;
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
            fs::create_dir_all(repo_dir.parent().unwrap()).map_err(|e| ResolveError::Io {
                path: repo_dir.clone(),
                source: e,
            })?;
            debug!(repo = %repo, dest = %repo_dir.display(), "git clone");
            git(&["clone", repo, repo_dir.to_string_lossy().as_ref()], None)?;
        } else {
            // Best-effort update.
            debug!(repo = %repo, dest = %repo_dir.display(), "git fetch");
            let _ = git(
                &[
                    "-C",
                    repo_dir.to_string_lossy().as_ref(),
                    "fetch",
                    "--tags",
                    "--prune",
                    "origin",
                ],
                None,
            );
        }

        // Check out the requested ref in a detached HEAD state. If `ref` is a branch name,
        // try `origin/<ref>` as a fallback.
        if git_checkout_detach(&repo_dir, git_ref).is_err()
            && !git_ref.contains('/')
            && !looks_like_hex(git_ref)
        {
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
    s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn git(args: &[&str], cwd: Option<&Path>) -> Result<(), ResolveError> {
    let mut cmd = Command::new("git");
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    debug!(cmd = %format!("git {}", args.join(" ")), "run");
    let out = cmd.output().map_err(|e| ResolveError::Io {
        path: PathBuf::from("git"),
        source: e,
    })?;
    if out.status.success() {
        return Ok(());
    }
    let status = out.status.code().unwrap_or(1);
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(ResolveError::GitCommandFailed {
        cmd: format!("git {}", args.join(" ")),
        status,
        stderr,
    })
}

fn ensure_is_dir(path: &Path) -> Result<(), ResolveError> {
    let meta = fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ResolveError::TemplatePathNotDir(path.to_path_buf())
        } else {
            ResolveError::Io {
                path: path.to_path_buf(),
                source: e,
            }
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

fn normalize_repo(repo: &str, protocol: GitProtocol) -> String {
    if is_github_shorthand(repo) {
        match protocol {
            GitProtocol::Ssh => format!("git@github.com:{repo}.git"),
            GitProtocol::Https => format!("https://github.com/{repo}.git"),
        }
    } else {
        repo.to_string()
    }
}

fn is_github_shorthand(repo: &str) -> bool {
    if repo.is_empty()
        || repo.contains("://")
        || repo.contains(':')
        || repo.starts_with("git@")
        || repo.contains('\\')
    {
        return false;
    }

    let mut parts = repo.split('/');
    let Some(owner) = parts.next() else {
        return false;
    };
    let Some(name) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    if owner.is_empty() || name.is_empty() || name.ends_with(".git") {
        return false;
    }
    if !is_repo_component(owner) || !is_repo_component(name) {
        return false;
    }
    true
}

fn is_repo_component(s: &str) -> bool {
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Return true if the path appears to be a git repository checkout.
pub fn path_is_git_dir(path: &Path) -> bool {
    path.join(".git").is_dir() || (path.file_name() == Some(OsStr::new(".git")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_hex_requires_min_len_and_hex_chars() {
        assert!(!looks_like_hex("abc"));
        assert!(!looks_like_hex("zzzzzzz"));
        assert!(looks_like_hex("0123456"));
        assert!(looks_like_hex("deadBEEF"));
    }

    #[test]
    fn path_is_git_dir_matches_dot_git_dir() {
        let tmp = std::env::temp_dir().join(format!("pinit-path-is-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join(".git")).unwrap();

        assert!(path_is_git_dir(&tmp));
        assert!(path_is_git_dir(&tmp.join(".git")));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cache_key_is_stable() {
        let a = cache_key("repo", "ref");
        let b = cache_key("repo", "ref");
        assert_eq!(a, b);
        assert_ne!(a, cache_key("repo", "ref2"));
    }

    #[test]
    fn normalize_repo_expands_github_shorthand_to_ssh() {
        let repo = normalize_repo("foo/bar", GitProtocol::Ssh);
        assert_eq!(repo, "git@github.com:foo/bar.git");
    }

    #[test]
    fn normalize_repo_expands_github_shorthand_to_https() {
        let repo = normalize_repo("foo/bar", GitProtocol::Https);
        assert_eq!(repo, "https://github.com/foo/bar.git");
    }

    #[test]
    fn normalize_repo_leaves_full_urls_untouched() {
        let https = "https://github.com/foo/bar.git";
        let ssh = "git@github.com:foo/bar.git";
        assert_eq!(normalize_repo(https, GitProtocol::Ssh), https);
        assert_eq!(normalize_repo(ssh, GitProtocol::Https), ssh);
    }
}
