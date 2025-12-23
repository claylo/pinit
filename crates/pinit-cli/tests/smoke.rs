use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn make_temp_root() -> TempRoot {
    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("pinit-smoke-{}-{n}", std::process::id()));
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
fn apply_copies_missing_files() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");

    fs::create_dir_all(&template_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut pinit_core::SkipExisting,
    )
    .unwrap();

    assert_eq!(report.created_files, 1);
    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 0);
    assert_eq!(
        fs::read_to_string(dest_dir.join("hello.txt")).unwrap(),
        "hello\n"
    );
}

#[test]
fn dry_run_does_not_write() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");

    fs::create_dir_all(&template_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: true },
        &mut pinit_core::SkipExisting,
    )
    .unwrap();

    assert_eq!(report.created_files, 1);
    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 0);
    assert!(!dest_dir.join("hello.txt").exists());
}
