#![forbid(unsafe_code)]

//! Core template-application engine used by the pinit CLI.
//!
//! The API is intentionally small: callers supply a template directory, a destination
//! directory, and a strategy for resolving conflicts when destination files already exist.

pub mod config;
pub mod licensing;
mod merge;
pub mod resolve;

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use tracing::{debug, instrument, trace};

/// Action to take when the destination file already exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExistingFileAction {
    Overwrite,
    Merge,
    Skip,
}

impl ExistingFileAction {
    /// String label used for logging and diagnostics.
    pub fn as_str(self) -> &'static str {
        match self {
            ExistingFileAction::Overwrite => "overwrite",
            ExistingFileAction::Merge => "merge",
            ExistingFileAction::Skip => "skip",
        }
    }
}

/// Context describing an existing destination file and its candidate replacements.
pub struct ExistingFileDecisionContext<'a> {
    /// Relative path within the destination.
    pub rel_path: &'a Path,
    /// Destination path on disk.
    pub dest_path: &'a Path,
    /// Bytes from the template.
    pub src_bytes: &'a [u8],
    /// Bytes from the destination.
    pub dest_bytes: &'a [u8],
    /// Merged bytes, if a merge driver could produce them.
    pub merge_bytes: Option<&'a [u8]>,
}

/// Decide what to do when a destination file already exists.
pub trait ExistingFileDecider {
    fn decide(&mut self, ctx: ExistingFileDecisionContext<'_>) -> ExistingFileAction;
}

/// Decider that always skips existing files.
#[derive(Clone, Copy, Debug, Default)]
pub struct SkipExisting;

impl ExistingFileDecider for SkipExisting {
    fn decide(&mut self, _ctx: ExistingFileDecisionContext<'_>) -> ExistingFileAction {
        ExistingFileAction::Skip
    }
}

/// Options that control template application.
#[derive(Clone, Copy, Debug, Default)]
pub struct ApplyOptions {
    /// When true, compute changes but do not write to disk.
    pub dry_run: bool,
}

/// Summary of work performed during template application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// Files created because they did not exist in the destination.
    pub created_files: usize,
    /// Files updated after an overwrite or merge action.
    pub updated_files: usize,
    /// Files skipped due to identical contents or a skip decision.
    pub skipped_files: usize,
    /// Paths ignored by destination gitignore rules.
    pub ignored_paths: usize,
}

/// Errors that can occur when applying a template directory.
#[derive(Debug)]
pub enum ApplyError {
    TemplateDirNotFound(PathBuf),
    TemplateDirNotDir(PathBuf),
    DestDirNotDir(PathBuf),
    SymlinkNotSupported(PathBuf),
    GitIgnoreFailed {
        cmd: String,
        status: i32,
        stderr: String,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
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
            ApplyError::DestDirNotDir(path) => {
                write!(f, "destination is not a directory: {}", path.display())
            }
            ApplyError::SymlinkNotSupported(path) => {
                write!(f, "symlinks are not supported (yet): {}", path.display())
            }
            ApplyError::GitIgnoreFailed {
                cmd,
                status,
                stderr,
            } => {
                write!(
                    f,
                    "git ignore check failed ({status}) running {cmd}: {stderr}"
                )
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

/// Apply an entire template directory into a destination directory.
///
/// This function walks the template tree, respects destination ignore rules, and
/// asks the provided decider what to do when existing files differ.
///
/// # Examples
/// ```no_run
/// use pinit_core::{apply_template_dir, ApplyOptions, SkipExisting};
///
/// let mut decider = SkipExisting::default();
/// let options = ApplyOptions { dry_run: true };
/// let _report = apply_template_dir("templates/rust", ".", options, &mut decider).unwrap();
/// ```
#[instrument(skip(options, decider), fields(template_dir = %template_dir.as_ref().display(), dest_dir = %dest_dir.as_ref().display(), dry_run = options.dry_run))]
pub fn apply_template_dir(
    template_dir: impl AsRef<Path>,
    dest_dir: impl AsRef<Path>,
    options: ApplyOptions,
    decider: &mut dyn ExistingFileDecider,
) -> Result<ApplyReport, ApplyError> {
    let template_dir = template_dir.as_ref();
    let dest_dir = dest_dir.as_ref();

    let template_meta = fs::symlink_metadata(template_dir).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            ApplyError::TemplateDirNotFound(template_dir.to_path_buf())
        } else {
            ApplyError::Io {
                path: template_dir.to_path_buf(),
                source: e,
            }
        }
    })?;
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
        fs::create_dir_all(dest_dir).map_err(|e| ApplyError::Io {
            path: dest_dir.to_path_buf(),
            source: e,
        })?;
    }

    let git_ignore = GitIgnore::detect(dest_dir)?;
    let mut report = ApplyReport::default();
    apply_dir_recursive(
        template_dir,
        template_dir,
        dest_dir,
        options,
        &git_ignore,
        decider,
        &mut report,
    )?;
    Ok(report)
}

/// Apply a generated file into the destination directory.
///
/// Generated files bypass merge drivers; if the destination exists the decider
/// can choose to overwrite or skip.
///
/// # Examples
/// ```no_run
/// use pinit_core::{apply_generated_file, ApplyOptions, SkipExisting};
///
/// let mut decider = SkipExisting::default();
/// let options = ApplyOptions { dry_run: true };
/// let _report = apply_generated_file(".", "LICENSE", b"MIT\n", options, &mut decider).unwrap();
/// ```
#[instrument(skip(options, decider, contents), fields(dest_dir = %dest_dir.as_ref().display(), rel_path = %rel_path.as_ref().display(), dry_run = options.dry_run))]
pub fn apply_generated_file(
    dest_dir: impl AsRef<Path>,
    rel_path: impl AsRef<Path>,
    contents: &[u8],
    options: ApplyOptions,
    decider: &mut dyn ExistingFileDecider,
) -> Result<ApplyReport, ApplyError> {
    let dest_dir = dest_dir.as_ref();
    let rel_path = rel_path.as_ref();

    if rel_path.as_os_str() == OsStr::new("") {
        return Ok(ApplyReport::default());
    }
    if should_always_ignore(rel_path) {
        trace!(path = %rel_path.display(), "ignored (always)");
        return Ok(ApplyReport {
            ignored_paths: 1,
            ..ApplyReport::default()
        });
    }

    if let Ok(dest_meta) = fs::symlink_metadata(dest_dir) {
        if dest_meta.file_type().is_symlink() {
            return Err(ApplyError::SymlinkNotSupported(dest_dir.to_path_buf()));
        }
        if !dest_meta.is_dir() {
            return Err(ApplyError::DestDirNotDir(dest_dir.to_path_buf()));
        }
    } else if !options.dry_run {
        fs::create_dir_all(dest_dir).map_err(|e| ApplyError::Io {
            path: dest_dir.to_path_buf(),
            source: e,
        })?;
    }

    let git_ignore = GitIgnore::detect(dest_dir)?;
    if let Some(g) = &git_ignore {
        let query = format_git_rel(rel_path, false);
        if g.ignored_set(&[query.clone()])?.contains(&query) {
            trace!(path = %query, "ignored (git)");
            return Ok(ApplyReport {
                ignored_paths: 1,
                ..ApplyReport::default()
            });
        }
    }

    let dest_path = dest_dir.join(rel_path);
    if dest_path.exists() {
        let dest_bytes = fs::read(&dest_path).map_err(|e| ApplyError::Io {
            path: dest_path.clone(),
            source: e,
        })?;
        if dest_bytes == contents {
            trace!(path = %rel_path.display(), "skip (identical)");
            return Ok(ApplyReport {
                skipped_files: 1,
                ..ApplyReport::default()
            });
        }

        let action = decider.decide(ExistingFileDecisionContext {
            rel_path,
            dest_path: &dest_path,
            src_bytes: contents,
            dest_bytes: &dest_bytes,
            merge_bytes: None,
        });

        trace!(path = %rel_path.display(), action = action.as_str(), "existing file decision (generated)");

        match action {
            ExistingFileAction::Skip | ExistingFileAction::Merge => {
                if action == ExistingFileAction::Merge {
                    debug!(path = %rel_path.display(), "merge unavailable for generated file; skipping");
                }
                return Ok(ApplyReport {
                    skipped_files: 1,
                    ..ApplyReport::default()
                });
            }
            ExistingFileAction::Overwrite => {}
        }

        if options.dry_run {
            return Ok(ApplyReport {
                updated_files: 1,
                ..ApplyReport::default()
            });
        }

        let existing_perms = fs::metadata(&dest_path)
            .map(|m| m.permissions())
            .map_err(|e| ApplyError::Io {
                path: dest_path.clone(),
                source: e,
            })?;
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).map_err(|e| ApplyError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }
        fs::write(&dest_path, contents).map_err(|e| ApplyError::Io {
            path: dest_path.clone(),
            source: e,
        })?;
        fs::set_permissions(&dest_path, existing_perms).map_err(|e| ApplyError::Io {
            path: dest_path.clone(),
            source: e,
        })?;
        return Ok(ApplyReport {
            updated_files: 1,
            ..ApplyReport::default()
        });
    }

    if options.dry_run {
        return Ok(ApplyReport {
            created_files: 1,
            ..ApplyReport::default()
        });
    }

    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent).map_err(|e| ApplyError::Io {
            path: parent.to_path_buf(),
            source: e,
        })?;
    }
    fs::write(&dest_path, contents).map_err(|e| ApplyError::Io {
        path: dest_path.clone(),
        source: e,
    })?;
    Ok(ApplyReport {
        created_files: 1,
        ..ApplyReport::default()
    })
}

fn apply_dir_recursive(
    root: &Path,
    current: &Path,
    dest_root: &Path,
    options: ApplyOptions,
    git_ignore: &Option<GitIgnore>,
    decider: &mut dyn ExistingFileDecider,
    report: &mut ApplyReport,
) -> Result<(), ApplyError> {
    let mut entries: Vec<_> = fs::read_dir(current)
        .map_err(|e| ApplyError::Io {
            path: current.to_path_buf(),
            source: e,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApplyError::Io {
            path: current.to_path_buf(),
            source: e,
        })?;

    entries.sort_by_key(|e| e.file_name());

    // Precompute ignore matches for this directory level so we don't spawn one `git` process per path.
    let mut queries: Vec<String> = Vec::with_capacity(entries.len());
    for entry in &entries {
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path);
        if rel.as_os_str() == OsStr::new("") {
            continue;
        }
        let meta = fs::symlink_metadata(&path).map_err(|e| ApplyError::Io {
            path: path.clone(),
            source: e,
        })?;
        let q = format_git_rel(rel, meta.is_dir());
        queries.push(q);
    }

    let ignored = match git_ignore {
        Some(g) => g.ignored_set(&queries)?,
        None => std::collections::HashSet::new(),
    };

    for entry in entries {
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| ApplyError::Io {
            path: path.clone(),
            source: e,
        })?;
        if meta.file_type().is_symlink() {
            return Err(ApplyError::SymlinkNotSupported(path));
        }

        let rel = path.strip_prefix(root).unwrap_or(&path);
        if rel.as_os_str() == OsStr::new("") {
            continue;
        }

        if should_always_ignore(rel) {
            trace!(path = %rel.display(), "ignored (always)");
            report.ignored_paths += 1;
            continue;
        }

        let is_dir = meta.is_dir();
        let query = format_git_rel(rel, is_dir);
        if ignored.contains(&query) {
            trace!(path = %query, "ignored (git)");
            report.ignored_paths += 1;
            continue;
        }

        if is_dir {
            apply_dir_recursive(root, &path, dest_root, options, git_ignore, decider, report)?;
            continue;
        }

        if !meta.is_file() {
            continue;
        }

        let dest_path = dest_root.join(rel);
        if dest_path.exists() {
            let src_bytes = fs::read(&path).map_err(|e| ApplyError::Io {
                path: path.clone(),
                source: e,
            })?;
            let dest_bytes = fs::read(&dest_path).map_err(|e| ApplyError::Io {
                path: dest_path.clone(),
                source: e,
            })?;

            if src_bytes == dest_bytes {
                trace!(path = %rel.display(), "skip (identical)");
                report.skipped_files += 1;
                continue;
            }

            let merge_bytes = merge::merge_file(rel, &dest_bytes, &src_bytes);
            let action = decider.decide(ExistingFileDecisionContext {
                rel_path: rel,
                dest_path: &dest_path,
                src_bytes: &src_bytes,
                dest_bytes: &dest_bytes,
                merge_bytes: merge_bytes.as_deref(),
            });

            trace!(path = %rel.display(), action = action.as_str(), "existing file decision");

            let output_bytes = match action {
                ExistingFileAction::Skip => {
                    report.skipped_files += 1;
                    continue;
                }
                ExistingFileAction::Overwrite => src_bytes,
                ExistingFileAction::Merge => {
                    let Some(merged) = merge_bytes else {
                        debug!(path = %rel.display(), "merge unavailable; skipping");
                        report.skipped_files += 1;
                        continue;
                    };
                    merged
                }
            };

            if output_bytes == dest_bytes {
                trace!(path = %rel.display(), action = action.as_str(), "no changes after action");
                report.skipped_files += 1;
                continue;
            }

            report.updated_files += 1;
            if options.dry_run {
                continue;
            }

            let existing_perms =
                fs::metadata(&dest_path)
                    .map(|m| m.permissions())
                    .map_err(|e| ApplyError::Io {
                        path: dest_path.clone(),
                        source: e,
                    })?;
            fs::write(&dest_path, &output_bytes).map_err(|e| ApplyError::Io {
                path: dest_path.clone(),
                source: e,
            })?;
            fs::set_permissions(&dest_path, existing_perms).map_err(|e| ApplyError::Io {
                path: dest_path.clone(),
                source: e,
            })?;

            continue;
        }

        if !options.dry_run {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).map_err(|e| ApplyError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
            trace!(src = %path.display(), dest = %dest_path.display(), "copy");
            fs::copy(&path, &dest_path).map_err(|e| ApplyError::Io {
                path: dest_path.clone(),
                source: e,
            })?;
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
            debug!(dest_root = %dest_root.display(), "gitignore: dest does not exist");
            return Ok(None);
        }
        let out = Command::new("git")
            .arg("-C")
            .arg(dest_root)
            .args(["rev-parse", "--is-inside-work-tree"])
            .output();

        let Ok(out) = out else {
            debug!(dest_root = %dest_root.display(), "gitignore: git not available");
            return Ok(None);
        };
        if !out.status.success() {
            debug!(dest_root = %dest_root.display(), "gitignore: not a git worktree");
            return Ok(None);
        }
        let stdout = String::from_utf8_lossy(&out.stdout);
        if stdout.trim() != "true" {
            debug!(dest_root = %dest_root.display(), inside = %stdout.trim(), "gitignore: not inside worktree");
            return Ok(None);
        }
        debug!(dest_root = %dest_root.display(), "gitignore: enabled");
        Ok(Some(Self {
            cwd: dest_root.to_path_buf(),
        }))
    }

    fn ignored_set(
        &self,
        rel_paths: &[String],
    ) -> Result<std::collections::HashSet<String>, ApplyError> {
        if rel_paths.is_empty() {
            return Ok(std::collections::HashSet::new());
        }

        trace!(count = rel_paths.len(), "gitignore: check");
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&self.cwd)
            .args(["check-ignore", "--stdin", "--verbose", "--non-matching"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ApplyError::Io {
                path: PathBuf::from("git"),
                source: e,
            })?;

        {
            let mut stdin = child.stdin.take().expect("stdin piped");
            use std::io::Write;
            for p in rel_paths {
                stdin.write_all(p.as_bytes()).map_err(|e| ApplyError::Io {
                    path: PathBuf::from("git stdin"),
                    source: e,
                })?;
                stdin.write_all(b"\n").map_err(|e| ApplyError::Io {
                    path: PathBuf::from("git stdin"),
                    source: e,
                })?;
            }
        }

        let out = child.wait_with_output().map_err(|e| ApplyError::Io {
            path: PathBuf::from("git"),
            source: e,
        })?;

        let status_code = out.status.code().unwrap_or(1);
        // `git check-ignore` returns exit status 1 when no paths are ignored.
        if !out.status.success() && status_code != 1 {
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
            let Some((left, path)) = line.split_once('\t') else {
                continue;
            };
            if left.starts_with("::") {
                continue;
            };
            ignored.insert(path.to_string());
        }
        Ok(ignored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "pinit-core-lib-{prefix}-{}-{n}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn gitignore_failed_variant_is_reachable() {
        let temp = make_temp_dir("gitignore-fail");
        let gi = GitIgnore {
            cwd: temp.join("missing"),
        };
        let err = gi.ignored_set(&["a.txt".to_string()]).unwrap_err();
        assert!(matches!(err, ApplyError::GitIgnoreFailed { .. }));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn format_git_rel_adds_trailing_slash_for_dirs() {
        assert_eq!(format_git_rel(Path::new("a/b"), true), "a/b/");
        assert_eq!(format_git_rel(Path::new("a/b/"), true), "a/b/");
        assert_eq!(format_git_rel(Path::new("a/b"), false), "a/b");
    }
}
