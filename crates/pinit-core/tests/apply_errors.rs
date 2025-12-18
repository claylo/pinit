use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn make_temp_root() -> TempRoot {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("pinit-apply-errors-test-{}-{n}", std::process::id()));
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
fn apply_template_dir_errors_when_template_missing() {
    let root = make_temp_root();
    let template = root.join("nope");
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();

    let err = pinit_core::apply_template_dir(
        &template,
        &dest,
        pinit_core::ApplyOptions { dry_run: false },
        &mut pinit_core::SkipExisting,
    )
    .unwrap_err();

    match err {
        pinit_core::ApplyError::TemplateDirNotFound(p) => assert_eq!(p, template),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn apply_template_dir_errors_when_template_not_dir() {
    let root = make_temp_root();
    let template = root.join("template");
    let dest = root.join("dest");
    fs::create_dir_all(&dest).unwrap();
    fs::write(&template, "file").unwrap();

    let err = pinit_core::apply_template_dir(
        &template,
        &dest,
        pinit_core::ApplyOptions { dry_run: false },
        &mut pinit_core::SkipExisting,
    )
    .unwrap_err();

    match err {
        pinit_core::ApplyError::TemplateDirNotDir(p) => assert_eq!(p, template),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn apply_template_dir_errors_on_symlink_entry() {
    let root = make_temp_root();
    let template = root.join("template");
    let dest = root.join("dest");
    fs::create_dir_all(&template).unwrap();
    fs::create_dir_all(&dest).unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(dest.join("real.txt"), template.join("link.txt")).unwrap();
    }

    let err = pinit_core::apply_template_dir(
        &template,
        &dest,
        pinit_core::ApplyOptions { dry_run: false },
        &mut pinit_core::SkipExisting,
    )
    .unwrap_err();

    match err {
        pinit_core::ApplyError::SymlinkNotSupported(_) => {}
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn apply_template_dir_errors_when_dest_is_not_dir() {
    let root = make_temp_root();
    let template = root.join("template");
    let dest = root.join("dest");
    fs::create_dir_all(&template).unwrap();
    fs::write(&dest, "file").unwrap();

    let err = pinit_core::apply_template_dir(
        &template,
        &dest,
        pinit_core::ApplyOptions { dry_run: false },
        &mut pinit_core::SkipExisting,
    )
    .unwrap_err();

    assert!(matches!(err, pinit_core::ApplyError::DestDirNotDir(_)));
}

#[test]
#[cfg(unix)]
fn apply_template_dir_errors_when_template_is_symlink() {
    use std::os::unix::fs::symlink;

    let root = make_temp_root();
    let real_template = root.join("real");
    let template = root.join("template");
    let dest = root.join("dest");
    fs::create_dir_all(&real_template).unwrap();
    fs::create_dir_all(&dest).unwrap();
    symlink(&real_template, &template).unwrap();

    let err = pinit_core::apply_template_dir(
        &template,
        &dest,
        pinit_core::ApplyOptions { dry_run: false },
        &mut pinit_core::SkipExisting,
    )
    .unwrap_err();

    assert!(matches!(err, pinit_core::ApplyError::SymlinkNotSupported(_)));
}
