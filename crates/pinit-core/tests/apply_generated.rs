use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use pinit_core::{ExistingFileAction, ExistingFileDecider, ExistingFileDecisionContext};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn make_temp_root() -> TempRoot {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "pinit-apply-generated-test-{}-{n}",
        std::process::id()
    ));
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
fn apply_generated_creates_then_skips_identical() {
    let root = make_temp_root();
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let r1 = pinit_core::apply_generated_file(
        &dest,
        "LICENSE",
        b"hello\n",
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();
    assert_eq!(r1.created_files, 1);
    assert_eq!(fs::read_to_string(dest.join("LICENSE")).unwrap(), "hello\n");

    let r2 = pinit_core::apply_generated_file(
        &dest,
        "LICENSE",
        b"hello\n",
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();
    assert_eq!(r2.skipped_files, 1);
}

#[test]
fn apply_generated_overwrite_updates_existing() {
    let root = make_temp_root();
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("LICENSE"), "old\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let r = pinit_core::apply_generated_file(
        &dest,
        "LICENSE",
        b"new\n",
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();
    assert_eq!(r.updated_files, 1);
    assert_eq!(fs::read_to_string(dest.join("LICENSE")).unwrap(), "new\n");
}

#[test]
fn apply_generated_merge_or_skip_does_not_write() {
    let root = make_temp_root();
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("LICENSE"), "old\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let r = pinit_core::apply_generated_file(
        &dest,
        "LICENSE",
        b"new\n",
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();
    assert_eq!(r.skipped_files, 1);
    assert_eq!(fs::read_to_string(dest.join("LICENSE")).unwrap(), "old\n");
}

#[test]
fn apply_generated_respects_always_ignore() {
    let root = make_temp_root();
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let r = pinit_core::apply_generated_file(
        &dest,
        ".DS_Store",
        b"x",
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();
    assert_eq!(r.ignored_paths, 1);
    assert!(!dest.join(".DS_Store").exists());
}

#[test]
fn apply_generated_rel_path_empty_is_noop() {
    let root = make_temp_root();
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let report = pinit_core::apply_generated_file(
        &dest,
        "",
        b"x",
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();
    assert_eq!(report, pinit_core::ApplyReport::default());
}

#[test]
fn apply_generated_creates_dest_dir_when_missing() {
    let root = make_temp_root();
    let dest = root.join("dest");
    assert!(!dest.exists());

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let report = pinit_core::apply_generated_file(
        &dest,
        "LICENSE",
        b"hello\n",
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();
    assert_eq!(report.created_files, 1);
    assert!(dest.is_dir());
    assert_eq!(fs::read_to_string(dest.join("LICENSE")).unwrap(), "hello\n");
}

#[test]
#[cfg(unix)]
fn apply_generated_errors_when_dest_is_symlink() {
    use std::os::unix::fs::symlink;

    let root = make_temp_root();
    let real = root.join("real");
    let dest = root.join("dest");
    fs::create_dir_all(&real).unwrap();
    symlink(&real, &dest).unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let err = pinit_core::apply_generated_file(
        &dest,
        "LICENSE",
        b"x",
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap_err();

    assert!(matches!(
        err,
        pinit_core::ApplyError::SymlinkNotSupported(_)
    ));
}

#[test]
fn apply_generated_respects_destination_gitignore() {
    let root = make_temp_root();
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();

    assert!(
        std::process::Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(&dest)
            .output()
            .unwrap()
            .status
            .success()
    );

    fs::write(dest.join(".gitignore"), "LICENSE\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let report = pinit_core::apply_generated_file(
        &dest,
        "LICENSE",
        b"x",
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.ignored_paths, 1);
    assert!(!dest.join("LICENSE").exists());
}

#[test]
fn apply_generated_errors_when_dest_is_not_dir() {
    let root = make_temp_root();
    let dest = root.join("dest");
    fs::write(&dest, "not a dir").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let err = pinit_core::apply_generated_file(
        &dest,
        "LICENSE",
        b"x",
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap_err();

    match err {
        pinit_core::ApplyError::DestDirNotDir(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }
}
