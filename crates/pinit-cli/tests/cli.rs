use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn make_temp_root() -> TempRoot {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("pinit-cli-integ-{}-{n}", std::process::id()));
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

fn pinit() -> Command {
    Command::new(env!("CARGO_BIN_EXE_pinit"))
}

#[test]
fn no_args_prints_help_and_exits_2() {
    let out = pinit().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Apply project template baselines"));
}

#[test]
fn list_without_config_is_ok() {
    let out = pinit().arg("list").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("no config found"));
}

#[test]
fn list_with_config_shows_templates_targets_and_recipes() {
    let root = make_temp_root();
    let cfg = root.join("pinit.toml");
    fs::write(
        &cfg,
        r#"
common = "common"

[templates]
common = "/tmp/common"
rust = "/tmp/rust"

[targets]
rust = ["common", "rust"]

[recipes.rust-lite]
templates = ["rust"]
"#,
    )
    .unwrap();

    let out = pinit().args(["--config", cfg.to_string_lossy().as_ref(), "list"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("templates:"));
    assert!(stdout.contains("targets:"));
    assert!(stdout.contains("recipes:"));
    assert!(stdout.contains("rust-lite"));
}

#[test]
fn list_recipe_with_no_templates_prints_dash() {
    let root = make_temp_root();
    let cfg = root.join("pinit.toml");
    fs::write(
        &cfg,
        r#"
common = "common"

[templates]
common = "/tmp/common"

[recipes.empty]
templates = []
"#,
    )
    .unwrap();

    let out = pinit().args(["--config", cfg.to_string_lossy().as_ref(), "list"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("empty (templates: -"));
}

#[test]
fn apply_from_template_dir_copies_files() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

    let out = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(dest_dir.join("hello.txt")).unwrap(), "hello\n");
}

#[test]
fn apply_from_config_dry_run_does_not_write() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

    let cfg = root.join("pinit.toml");
    fs::write(
        &cfg,
        format!(
            r#"
[templates]
rust = "{}"
"#,
            template_dir.display()
        ),
    )
    .unwrap();

    let dest_dir = root.join("dest");
    let out = pinit()
        .args([
            "--config",
            cfg.to_string_lossy().as_ref(),
            "apply",
            "rust",
            dest_dir.to_string_lossy().as_ref(),
            "--yes",
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!dest_dir.exists());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dry-run:"));
}

#[test]
fn apply_interactive_diff_then_skip_leaves_file_unchanged() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "from-template\n").unwrap();
    fs::write(dest_dir.join("hello.txt"), "from-dest\n").unwrap();

    let mut child = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"d\ns\n").unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(dest_dir.join("hello.txt")).unwrap(), "from-dest\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("diffs for"));
}

#[test]
fn apply_interactive_diff_reports_diff_too_large() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    let dest_bytes = vec![b'a'; 200_001];
    let mut tpl_bytes = vec![b'a'; 200_001];
    tpl_bytes[0] = b'b';

    fs::write(template_dir.join("big.txt"), &tpl_bytes).unwrap();
    fs::write(dest_dir.join("big.txt"), &dest_bytes).unwrap();

    let mut child = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"d\ns\n").unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("diff too large"));
}

#[test]
fn apply_interactive_diff_reports_binary_dest() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join("bin.txt"), "template\n").unwrap();
    fs::write(dest_dir.join("bin.txt"), vec![0xff, 0x00, 0xfe]).unwrap();

    let mut child = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"d\ns\n").unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--- merge (unavailable)"));
    assert!(stderr.contains("binary dest"));
}

#[test]
fn apply_interactive_diff_reports_binary_template() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join("bin.txt"), vec![0xff, 0x00, 0xfe]).unwrap();
    fs::write(dest_dir.join("bin.txt"), "dest\n").unwrap();

    let mut child = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"d\ns\n").unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("binary template/merged"));
}

#[test]
fn apply_interactive_diff_reports_no_textual_changes_for_merge() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join(".env"), "A=template\n").unwrap();
    fs::write(dest_dir.join(".env"), "A=dest\n").unwrap();

    let mut child = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"d\ns\n").unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("(no textual changes)"));
}

#[test]
fn apply_interactive_overwrite_updates_file() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "from-template\n").unwrap();
    fs::write(dest_dir.join("hello.txt"), "from-dest\n").unwrap();

    let mut child = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"o\n").unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(dest_dir.join("hello.txt")).unwrap(), "from-template\n");
}

#[test]
fn apply_unknown_template_errors() {
    let out = pinit().args(["apply", "nope"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error:"));
}

#[test]
fn apply_errors_on_absolute_license_output_path() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

    let cfg = root.join("pinit.toml");
    fs::write(
        &cfg,
        format!(
            r#"
[license]
spdx = "MIT"
output = "/tmp/NOPE"

[templates]
rust = "{}"
"#,
            template_dir.display()
        ),
    )
    .unwrap();

    let out = pinit()
        .args([
            "--config",
            cfg.to_string_lossy().as_ref(),
            "apply",
            "rust",
            dest_dir.to_string_lossy().as_ref(),
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("license.output must be a relative path"));
}

#[test]
fn list_with_invalid_config_errors() {
    let root = make_temp_root();
    let cfg = root.join("pinit.toml");
    fs::write(&cfg, "not = toml = ").unwrap();

    let out = pinit().args(["--config", cfg.to_string_lossy().as_ref(), "list"]).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error:"));
}

#[test]
fn apply_yes_skip_does_not_overwrite_existing() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "from-template\n").unwrap();
    fs::write(dest_dir.join("hello.txt"), "from-dest\n").unwrap();

    let out = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
            "--yes",
            "--skip",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(dest_dir.join("hello.txt")).unwrap(), "from-dest\n");
}

#[test]
fn apply_yes_merge_unavailable_defaults_to_skip() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join(".env"), "A=template\n").unwrap();
    fs::write(dest_dir.join(".env"), "# comment\nA=dest\n").unwrap();

    let out = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(dest_dir.join(".env")).unwrap(), "# comment\nA=dest\n");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("skipped 1 file"));
}

#[test]
fn apply_yes_overwrite_overwrites_existing() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join("hello.txt"), "from-template\n").unwrap();
    fs::write(dest_dir.join("hello.txt"), "from-dest\n").unwrap();

    let out = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
            "--yes",
            "--overwrite",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(fs::read_to_string(dest_dir.join("hello.txt")).unwrap(), "from-template\n");
}

#[test]
fn new_dry_run_does_not_create_dir_and_mentions_git_init() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

    let dest = root.join("proj");
    let out = pinit()
        .args([
            "new",
            template_dir.to_string_lossy().as_ref(),
            dest.to_string_lossy().as_ref(),
            "--dry-run",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!dest.exists());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("dry-run: would create directory"));
    assert!(stderr.contains("dry-run: would run git init"));
}

#[test]
fn verbose_flag_debug_level_path_is_reachable() {
    let out = pinit().args(["-vv", "list"]).output().unwrap();
    assert!(out.status.success());
}

#[test]
fn apply_interactive_default_merge_appends_missing_lines() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "template\n").unwrap();
    fs::write(dest_dir.join("hello.txt"), "dest\n").unwrap();

    let mut child = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"\n").unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let contents = fs::read_to_string(dest_dir.join("hello.txt")).unwrap();
    assert!(contents.contains("dest\n"));
    assert!(contents.contains("template\n"));
}

#[test]
fn apply_interactive_handles_merge_unavailable_and_unknown_choice() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();
    fs::write(template_dir.join(".env"), "A=template\n").unwrap();
    fs::write(dest_dir.join(".env"), "# comment\nA=dest\n").unwrap();

    let mut child = pinit()
        .args([
            "apply",
            template_dir.to_string_lossy().as_ref(),
            dest_dir.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(b"x\n\ns\n").unwrap();
    }

    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown choice"));
    assert!(stderr.contains("merge is unavailable"));
}

#[test]
fn verbose_flag_triggers_trace_level_path() {
    let out = pinit().args(["-vvv", "list"]).output().unwrap();
    assert!(out.status.success());
}

#[test]
fn new_errors_when_dest_is_file() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

    let dest = root.join("proj");
    fs::write(&dest, "file").unwrap();

    let out = pinit()
        .args([
            "new",
            template_dir.to_string_lossy().as_ref(),
            dest.to_string_lossy().as_ref(),
            "--no-git",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("destination is not a directory"));
}

#[test]
fn new_errors_when_dest_dir_not_empty() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(template_dir.join("hello.txt"), "hello\n").unwrap();

    let dest = root.join("proj");
    fs::create_dir_all(&dest).unwrap();
    fs::write(dest.join("already.txt"), "x").unwrap();

    let out = pinit()
        .args([
            "new",
            template_dir.to_string_lossy().as_ref(),
            dest.to_string_lossy().as_ref(),
            "--no-git",
            "--yes",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("destination already exists and is not empty"));
}
