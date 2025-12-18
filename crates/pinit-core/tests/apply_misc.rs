use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use pinit_core::{ExistingFileAction, ExistingFileDecider, ExistingFileDecisionContext};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn make_temp_root() -> TempRoot {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("pinit-apply-misc-test-{}-{n}", std::process::id()));
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

struct FixedDecider(ExistingFileAction);

impl ExistingFileDecider for FixedDecider {
    fn decide(&mut self, _ctx: ExistingFileDecisionContext<'_>) -> ExistingFileAction {
        self.0
    }
}

#[test]
fn skip_existing_decider_is_used_for_existing_files() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join("hello.txt"), "template\n").unwrap();
    fs::write(dest_dir.join("hello.txt"), "dest\n").unwrap();

    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut pinit_core::SkipExisting,
    )
    .unwrap();

    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 1);
    assert_eq!(fs::read_to_string(dest_dir.join("hello.txt")).unwrap(), "dest\n");
}

#[test]
fn overwrite_dry_run_counts_update_without_writing() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join("hello.txt"), "template\n").unwrap();
    fs::write(dest_dir.join("hello.txt"), "dest\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: true },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.updated_files, 1);
    assert_eq!(fs::read_to_string(dest_dir.join("hello.txt")).unwrap(), "dest\n");
}

#[test]
fn apply_skips_identical_files() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join("hello.txt"), "same\n").unwrap();
    fs::write(dest_dir.join("hello.txt"), "same\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();
    assert_eq!(report.skipped_files, 1);
}

#[test]
fn apply_always_ignores_dot_git_paths() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(template_dir.join(".git")).unwrap();
    fs::write(template_dir.join(".git/config"), "x").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();
    assert_eq!(report.ignored_paths, 1);
    assert!(!dest_dir.join(".git/config").exists());
}

#[test]
fn apply_generated_dry_run_reports_updates_without_writing() {
    let root = make_temp_root();
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("LICENSE"), "old\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let report = pinit_core::apply_generated_file(
        &dest,
        "LICENSE",
        b"new\n",
        pinit_core::ApplyOptions { dry_run: true },
        &mut decider,
    )
    .unwrap();
    assert_eq!(report.updated_files, 1);
    assert_eq!(fs::read_to_string(dest.join("LICENSE")).unwrap(), "old\n");
}

#[test]
#[cfg(unix)]
fn overwrite_preserves_existing_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join("hello.txt"), "template\n").unwrap();
    fs::write(dest_dir.join("hello.txt"), "dest\n").unwrap();

    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(dest_dir.join("hello.txt"), perms).unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let _ = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    let out_perms = fs::metadata(dest_dir.join("hello.txt")).unwrap().permissions().mode() & 0o777;
    assert_eq!(out_perms, 0o600);
}
