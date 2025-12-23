use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use pinit_core::{ExistingFileAction, ExistingFileDecider, ExistingFileDecisionContext};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn make_temp_root() -> TempRoot {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "pinit-apply-existing-test-{}-{n}",
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
fn existing_file_overwrite_replaces_contents() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");

    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join("hello.txt"), "from-template\n").unwrap();
    fs::write(dest_dir.join("hello.txt"), "from-dest\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Overwrite);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.created_files, 0);
    assert_eq!(report.updated_files, 1);
    assert_eq!(report.skipped_files, 0);
    assert_eq!(
        fs::read_to_string(dest_dir.join("hello.txt")).unwrap(),
        "from-template\n"
    );
}

#[test]
fn existing_env_merge_adds_missing_keys_only() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");

    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join(".env"), "A=template\nB=template\n").unwrap();
    fs::write(dest_dir.join(".env"), "A=dest\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.created_files, 0);
    assert_eq!(report.updated_files, 1);
    assert_eq!(report.skipped_files, 0);

    let out = fs::read_to_string(dest_dir.join(".env")).unwrap();
    assert!(out.contains("A=dest\n"));
    assert!(out.contains("B=template\n"));
    assert!(!out.contains("A=template\n"));
}
