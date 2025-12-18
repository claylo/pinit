#![forbid(unsafe_code)]

pub mod config;
pub mod resolve;

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, Default)]
pub struct ApplyOptions {
    pub dry_run: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub created_files: usize,
    pub skipped_files: usize,
    pub ignored_paths: usize,
}

#[derive(Debug)]
pub enum ApplyError {
    TemplateDirNotFound(PathBuf),
    TemplateDirNotDir(PathBuf),
    DestDirNotDir(PathBuf),
    SymlinkNotSupported(PathBuf),
    GitIgnoreFailed { cmd: String, status: i32, stderr: String },
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApplyError::TemplateDirNotFound(path) => {
                write!(f, "template directory not found: {}", path.display())
            }
            ApplyError::TemplateDirNotDir(path) => {
                write!(f, "template path is not a directory: {}", path.display())
            }
            ApplyError::DestDirNotDir(path) => write!(f, "destination is not a directory: {}", path.display()),
            ApplyError::SymlinkNotSupported(path) => {
                write!(f, "symlinks are not supported (yet): {}", path.display())
            }
            ApplyError::GitIgnoreFailed { cmd, status, stderr } => {
                write!(f, "git ignore check failed ({status}) running {cmd}: {stderr}")
            }
            ApplyError::Io { path, source } => write!(f, "{}: {}", path.display(), source),
        }
    }
}

impl std::error::Error for ApplyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ApplyError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn apply_template_dir(
    template_dir: impl AsRef<Path>,
    dest_dir: impl AsRef<Path>,
    options: ApplyOptions,
) -> Result<ApplyReport, ApplyError> {
    let template_dir = template_dir.as_ref();
    let dest_dir = dest_dir.as_ref();

    let template_meta =
        fs::symlink_metadata(template_dir).map_err(|e| ApplyError::Io { path: template_dir.to_path_buf(), source: e })?;
    if template_meta.file_type().is_symlink() {
        return Err(ApplyError::SymlinkNotSupported(template_dir.to_path_buf()));
    }
    if !template_meta.is_dir() {
        return Err(ApplyError::TemplateDirNotDir(template_dir.to_path_buf()));
    }

    if let Ok(dest_meta) = fs::symlink_metadata(dest_dir) {
        if dest_meta.file_type().is_symlink() {
            return Err(ApplyError::SymlinkNotSupported(dest_dir.to_path_buf()));
        }
        if !dest_meta.is_dir() {
            return Err(ApplyError::DestDirNotDir(dest_dir.to_path_buf()));
        }
    } else if !options.dry_run {
        fs::create_dir_all(dest_dir).map_err(|e| ApplyError::Io { path: dest_dir.to_path_buf(), source: e })?;
    }

    let git_ignore = GitIgnore::detect(dest_dir)?;
    let mut report = ApplyReport::default();
    apply_dir_recursive(template_dir, template_dir, dest_dir, options, &git_ignore, &mut report)?;
    Ok(report)
}

fn apply_dir_recursive(
    root: &Path,
    current: &Path,
    dest_root: &Path,
    options: ApplyOptions,
    git_ignore: &Option<GitIgnore>,
    report: &mut ApplyReport,
) -> Result<(), ApplyError> {
    let mut entries: Vec<_> = fs::read_dir(current)
        .map_err(|e| ApplyError::Io { path: current.to_path_buf(), source: e })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApplyError::Io { path: current.to_path_buf(), source: e })?;

    entries.sort_by_key(|e| e.file_name());

    // Precompute ignore matches for this directory level so we don't spawn one `git` process per path.
    let mut queries: Vec<String> = Vec::with_capacity(entries.len());
    let mut rel_for_query: Vec<(PathBuf, bool)> = Vec::with_capacity(entries.len()); // (rel, is_dir)

    for entry in &entries {
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        if rel.as_os_str() == OsStr::new("") {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| ApplyError::Io { path: path.clone(), source: e })?;
        let is_dir = meta.is_dir();
        let q = format_git_rel(&rel, is_dir);
        queries.push(q);
        rel_for_query.push((rel, is_dir));
    }

    let ignored = match git_ignore {
        Some(g) => g.ignored_set(&queries)?,
        None => std::collections::HashSet::new(),
    };

    for entry in entries {
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| ApplyError::Io { path: path.clone(), source: e })?;
        if meta.file_type().is_symlink() {
            return Err(ApplyError::SymlinkNotSupported(path));
        }

        let rel = path.strip_prefix(root).unwrap_or(&path);
        if rel.as_os_str() == OsStr::new("") {
            continue;
        }

        if should_always_ignore(rel) {
            report.ignored_paths += 1;
            continue;
        }

        let is_dir = meta.is_dir();
        let query = format_git_rel(rel, is_dir);
        if ignored.contains(&query) {
            report.ignored_paths += 1;
            continue;
        }

        if is_dir {
            apply_dir_recursive(root, &path, dest_root, options, git_ignore, report)?;
            continue;
        }

        if !meta.is_file() {
            continue;
        }

        let dest_path = dest_root.join(rel);
        if dest_path.exists() {
            report.skipped_files += 1;
            continue;
        }

        if !options.dry_run {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).map_err(|e| ApplyError::Io { path: parent.to_path_buf(), source: e })?;
            }
            fs::copy(&path, &dest_path).map_err(|e| ApplyError::Io { path: dest_path.clone(), source: e })?;
        }
        report.created_files += 1;
    }

    Ok(())
}

fn should_always_ignore(rel: &Path) -> bool {
    if rel.file_name() == Some(OsStr::new(".DS_Store")) {
        return true;
    }
    matches!(rel.components().next(), Some(std::path::Component::Normal(s)) if s == OsStr::new(".git"))
}

fn format_git_rel(rel: &Path, is_dir: bool) -> String {
    // git expects forward slashes regardless of OS.
    let mut s = rel.to_string_lossy().replace('\\', "/");
    if is_dir && !s.ends_with('/') {
        s.push('/');
    }
    s
}

#[derive(Clone, Debug)]
struct GitIgnore {
    cwd: PathBuf,
}

impl GitIgnore {
    fn detect(dest_root: &Path) -> Result<Option<Self>, ApplyError> {
        if !dest_root.exists() {
            return Ok(None);
        }
        let out = Command::new("git")
            .arg("-C")
            .arg(dest_root)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output();

        let Ok(out) = out else {
            return Ok(None);
        };
        if !out.status.success() {
            return Ok(None);
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        if stdout.trim() != "true" {
            return Ok(None);
        }
        Ok(Some(Self { cwd: dest_root.to_path_buf() }))
    }

    fn ignored_set(&self, rel_paths: &[String]) -> Result<std::collections::HashSet<String>, ApplyError> {
        if rel_paths.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        let mut child = Command::new("git")
            .arg("-C")
            .arg(&self.cwd)
            .args(["check-ignore", "--stdin", "--verbose", "--non-matching"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ApplyError::Io { path: PathBuf::from("git"), source: e })?;

        {
            let mut stdin = child.stdin.take().expect("stdin piped");
            use std::io::Write;
            for p in rel_paths {
                stdin
                    .write_all(p.as_bytes())
                    .map_err(|e| ApplyError::Io { path: PathBuf::from("git stdin"), source: e })?;
                stdin
                    .write_all(b"\n")
                    .map_err(|e| ApplyError::Io { path: PathBuf::from("git stdin"), source: e })?;
            }
        }

        let out = child
            .wait_with_output()
            .map_err(|e| ApplyError::Io { path: PathBuf::from("git"), source: e })?;

        if !out.status.success() {
            let status = out.status.code().unwrap_or(1);
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(ApplyError::GitIgnoreFailed {
                cmd: "git check-ignore --stdin --verbose --non-matching".to_string(),
                status,
                stderr,
            });
        }

        let mut ignored = std::collections::HashSet::new();
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let Some((left, path)) = line.split_once('\t') else { continue };
            if left.starts_with("::") { continue };
            ignored.insert(path.to_string());
        }
        Ok(ignored)
    }
}
