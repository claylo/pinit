#![forbid(unsafe_code)]

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default)]
pub struct ApplyOptions {
    pub dry_run: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApplyReport {
    pub created_files: usize,
    pub skipped_files: usize,
}

#[derive(Debug)]
pub enum ApplyError {
    TemplateDirNotFound(PathBuf),
    TemplateDirNotDir(PathBuf),
    DestDirNotDir(PathBuf),
    SymlinkNotSupported(PathBuf),
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

    let mut report = ApplyReport::default();
    apply_dir_recursive(template_dir, template_dir, dest_dir, options, &mut report)?;
    Ok(report)
}

fn apply_dir_recursive(
    root: &Path,
    current: &Path,
    dest_root: &Path,
    options: ApplyOptions,
    report: &mut ApplyReport,
) -> Result<(), ApplyError> {
    let mut entries: Vec<_> = fs::read_dir(current)
        .map_err(|e| ApplyError::Io { path: current.to_path_buf(), source: e })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApplyError::Io { path: current.to_path_buf(), source: e })?;

    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| ApplyError::Io { path: path.clone(), source: e })?;
        if meta.file_type().is_symlink() {
            return Err(ApplyError::SymlinkNotSupported(path));
        }

        if meta.is_dir() {
            apply_dir_recursive(root, &path, dest_root, options, report)?;
            continue;
        }

        if !meta.is_file() {
            continue;
        }

        let rel = path.strip_prefix(root).unwrap_or(&path);
        if rel.as_os_str() == OsStr::new("") {
            continue;
        }

        let dest_path = dest_root.join(rel);
        if dest_path.exists() {
            report.skipped_files += 1;
            continue;
        }

        if !options.dry_run {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| ApplyError::Io { path: parent.to_path_buf(), source: e })?;
            }
            fs::copy(&path, &dest_path).map_err(|e| ApplyError::Io { path: dest_path.clone(), source: e })?;
        }
        report.created_files += 1;
    }

    Ok(())
}

