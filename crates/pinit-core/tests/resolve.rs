use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use pinit_core::config::{Config, Source, TemplateDef};
use pinit_core::resolve::{ResolveError, TemplateResolver};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn make_temp_root() -> TempRoot {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("pinit-resolve-test-{}-{n}", std::process::id()));
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

fn git(repo_dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git").arg("-C").arg(repo_dir).args(args).output().unwrap()
}

fn git_ok(repo_dir: &Path, args: &[&str]) {
    let out = git(repo_dir, args);
    assert!(
        out.status.success(),
        "git failed: git -C {} {}: {}",
        repo_dir.display(),
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_stdout(repo_dir: &Path, args: &[&str]) -> String {
    let out = git(repo_dir, args);
    assert!(out.status.success(), "git failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn resolves_local_template_from_source_path() {
    let root = make_temp_root();
    let templates_root = root.join("templates");
    fs::create_dir_all(templates_root.join("rust")).unwrap();

    let mut cfg = Config::default();
    cfg.sources.push(Source { name: "local".into(), path: Some(templates_root.clone()), ..Default::default() });
    cfg.templates.insert(
        "rust".into(),
        TemplateDef::Detailed { source: Some("local".into()), path: PathBuf::from("rust") },
    );

    let resolver = TemplateResolver::new(root.join("cache"));
    let resolved = resolver.resolve_template_dir(&cfg, "rust").unwrap();
    assert_eq!(resolved, templates_root.join("rust"));
}

#[test]
fn resolves_git_template_from_cached_clone() {
    if !git_available() {
        return;
    }

    let root = make_temp_root();
    let repo_dir = root.join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    assert!(Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(&repo_dir)
        .output()
        .unwrap()
        .status
        .success());

    git_ok(&repo_dir, &["config", "user.email", "pinit@example.invalid"]);
    git_ok(&repo_dir, &["config", "user.name", "pinit"]);

    fs::create_dir_all(repo_dir.join("templates/rust")).unwrap();
    fs::write(repo_dir.join("templates/rust/hello.txt"), "hello\n").unwrap();
    git_ok(&repo_dir, &["add", "."]);
    git_ok(&repo_dir, &["commit", "-m", "init"]);

    let commit = git_stdout(&repo_dir, &["rev-parse", "HEAD"]);

    let mut cfg = Config::default();
    cfg.sources.push(Source {
        name: "repo".into(),
        repo: Some(repo_dir.to_string_lossy().to_string()),
        git_ref: Some(commit),
        subdir: Some(PathBuf::from("templates")),
        ..Default::default()
    });
    cfg.templates.insert(
        "rust".into(),
        TemplateDef::Detailed { source: Some("repo".into()), path: PathBuf::from("rust") },
    );

    let cache_dir = root.join("cache");
    let resolver = TemplateResolver::new(cache_dir);
    let resolved = resolver.resolve_template_dir(&cfg, "rust").unwrap();
    assert!(resolved.is_dir());
    assert!(resolved.join("hello.txt").is_file());
}

#[test]
fn missing_git_ref_returns_error() {
    if !git_available() {
        return;
    }

    let root = make_temp_root();
    let repo_dir = root.join("repo");
    fs::create_dir_all(&repo_dir).unwrap();

    assert!(Command::new("git")
        .arg("init")
        .arg("-q")
        .arg(&repo_dir)
        .output()
        .unwrap()
        .status
        .success());

    let mut cfg = Config::default();
    cfg.sources.push(Source {
        name: "repo".into(),
        repo: Some(repo_dir.to_string_lossy().to_string()),
        git_ref: Some("definitely-not-a-ref".into()),
        subdir: Some(PathBuf::from("templates")),
        ..Default::default()
    });
    cfg.templates.insert(
        "rust".into(),
        TemplateDef::Detailed { source: Some("repo".into()), path: PathBuf::from("rust") },
    );

    let resolver = TemplateResolver::new(root.join("cache"));
    let err = resolver.resolve_template_dir(&cfg, "rust").unwrap_err();
    match err {
        ResolveError::GitCommandFailed { .. } => {}
        other => panic!("unexpected error: {other:?}"),
    }
}
