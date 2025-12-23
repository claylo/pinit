use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn make_temp_root() -> TempRoot {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "pinit-apply-ignore-test-{}-{n}",
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

#[test]
fn always_ignores_ds_store() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");

    fs::create_dir_all(&template_dir).unwrap();
    fs::write(template_dir.join(".DS_Store"), "junk").unwrap();
    fs::write(template_dir.join("ok.txt"), "ok\n").unwrap();

    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut pinit_core::SkipExisting,
    )
    .unwrap();
    assert_eq!(report.created_files, 1);
    assert_eq!(report.updated_files, 0);
    assert!(dest_dir.join("ok.txt").is_file());
    assert!(!dest_dir.join(".DS_Store").exists());
}

#[test]
fn honors_destination_gitignore() {
    if !git_available() {
        return;
    }

    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");

    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    // Initialize a git repo so `git check-ignore` uses repo + global excludes.
    assert!(
        Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(&dest_dir)
            .output()
            .unwrap()
            .status
            .success()
    );

    fs::write(dest_dir.join(".gitignore"), "ignored.txt\nignored-dir/\n").unwrap();

    fs::write(template_dir.join("ignored.txt"), "nope\n").unwrap();
    fs::write(template_dir.join("ok.txt"), "ok\n").unwrap();
    fs::create_dir_all(template_dir.join("ignored-dir")).unwrap();
    fs::write(template_dir.join("ignored-dir/file.txt"), "nope\n").unwrap();

    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut pinit_core::SkipExisting,
    )
    .unwrap();
    assert_eq!(report.created_files, 1);
    assert_eq!(report.updated_files, 0);
    assert!(dest_dir.join("ok.txt").is_file());
    assert!(!dest_dir.join("ignored.txt").exists());
    assert!(!dest_dir.join("ignored-dir/file.txt").exists());
}
