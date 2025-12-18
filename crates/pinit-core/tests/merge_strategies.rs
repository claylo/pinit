use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use pinit_core::{ExistingFileAction, ExistingFileDecider, ExistingFileDecisionContext};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn make_temp_root() -> TempRoot {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("pinit-merge-strategies-test-{}-{n}", std::process::id()));
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

fn run_merge(file_name: &str, dest_contents: &[u8], template_contents: &[u8]) -> (String, pinit_core::ApplyReport) {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");

    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(template_dir.join(file_name), template_contents).unwrap();
    fs::write(dest_dir.join(file_name), dest_contents).unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    let out = fs::read_to_string(dest_dir.join(file_name)).unwrap_or_default();
    (out, report)
}

#[test]
fn merge_envrc_adds_missing_without_duplicate_vars() {
    let (out, report) = run_merge(
        ".envrc",
        b"export FOO=dest\nuse flake\n",
        b"export FOO=template\nexport BAR=template\nuse flake\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("export FOO=dest\n"));
    assert!(out.contains("export BAR=template\n"));
    assert!(!out.contains("export FOO=template\n"));
}

#[test]
fn merge_toml_inserts_missing_keys_recursively() {
    let (out, report) = run_merge(
        "config.toml",
        b"[a]\nx = 1\n\n[b]\ny = 2\n",
        b"[a]\nx = 9\nz = 3\n\n[c]\nk = 1\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("x = 1"));
    assert!(out.contains("z = 3"));
    assert!(out.contains("[c]"));
    assert!(out.contains("k = 1"));
}

#[test]
fn merge_yaml_inserts_missing_mapping_keys() {
    let (out, report) = run_merge(
        "config.yaml",
        b"a:\n  x: 1\n",
        b"a:\n  x: 2\n  y: 3\nb: 4\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("x: 1"));
    assert!(out.contains("y: 3"));
    assert!(out.contains("b: 4"));
}

#[test]
fn merge_lines_appends_missing_exact_lines() {
    let (out, report) = run_merge("notes.txt", b"a\nb", b"b\nc\n");
    assert_eq!(report.updated_files, 1);
    assert_eq!(out, "a\nb\nc\n");
}

#[test]
fn merge_toml_ignores_type_mismatches_but_inserts_other_keys() {
    let (out, report) = run_merge(
        "config.toml",
        b"a = 1\n",
        b"b = 2\n[a]\nx = 2\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("a = 1"));
    assert!(out.contains("b = 2"));
    assert!(!out.contains("x = 2"));
}

#[test]
fn merge_lines_no_new_lines_results_in_skip_after_merge() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(dest_dir.join("notes.txt"), "a\nb\n").unwrap();
    fs::write(template_dir.join("notes.txt"), "a\nb").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 1);
    assert_eq!(fs::read_to_string(dest_dir.join("notes.txt")).unwrap(), "a\nb\n");
}

#[test]
fn merge_env_no_missing_keys_results_in_skip_after_merge() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(dest_dir.join(".env"), "A=dest\n").unwrap();
    fs::write(template_dir.join(".env"), "A=template\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 1);
    assert_eq!(fs::read_to_string(dest_dir.join(".env")).unwrap(), "A=dest\n");
}

#[test]
fn merge_env_skips_invalid_and_duplicate_keys_from_template() {
    let (out, report) = run_merge(
        ".env",
        b"A=dest\n",
        b"# comment\n\nexport A=template\nB=2\n=bad\n1BAD=3\n_OK=4\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("A=dest\n"));
    assert!(out.contains("B=2\n"));
    assert!(out.contains("_OK=4\n"));
    assert!(!out.contains("export A=template\n"));
    assert!(!out.contains("1BAD=3\n"));
    assert!(!out.contains("=bad\n"));
}

#[test]
fn merge_env_with_comment_makes_merge_unavailable() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(dest_dir.join(".env"), "# comment\nA=dest\n").unwrap();
    fs::write(template_dir.join(".env"), "A=template\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 1);
}

#[test]
fn merge_binary_file_unavailable_skips() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    let dest_bytes = vec![0xff, 0x00, 0xfe];
    let tpl_bytes = vec![0x01, 0x02, 0x03, 0x04];
    fs::write(template_dir.join("bin.dat"), &tpl_bytes).unwrap();
    fs::write(dest_dir.join("bin.dat"), &dest_bytes).unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 1);
    assert_eq!(fs::read(dest_dir.join("bin.dat")).unwrap(), dest_bytes);
}

#[test]
fn merge_python_inserts_import_after_shebang() {
    let (out, report) = run_merge(
        "main.py",
        b"#!/usr/bin/env python3\nimport os\n\n\ndef foo():\n    return 1\n",
        b"import sys\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.starts_with("#!/usr/bin/env python3\n"));
    assert!(out.contains("import os\n"));
    assert!(out.contains("import sys\n"));
    assert!(out.find("import sys").unwrap() > out.find("#!/usr/bin/env python3").unwrap());
    assert!(out.find("import sys").unwrap() < out.find("def foo").unwrap());
}

#[test]
fn merge_ruby_treats_require_as_import_like() {
    let (out, report) = run_merge(
        "main.rb",
        "require 'a'\n\nclass Foo\nend\n".as_bytes(),
        "require 'b'\n\nclass Bar\nend\n".as_bytes(),
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("require 'a'"));
    assert!(out.contains("require 'b'"));
    assert!(out.find("require 'b'").unwrap() < out.find("class Foo").unwrap());
}

#[test]
fn merge_ruby_call_is_not_treated_as_require_import() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(dest_dir.join("main.rb"), "require 'a'\n").unwrap();
    fs::write(template_dir.join("main.rb"), "puts 'hi'\nrequire 'a'\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 1);
    assert_eq!(fs::read_to_string(dest_dir.join("main.rb")).unwrap(), "require 'a'\n");
}

#[test]
fn merge_rust_inserts_use_and_appends_functions() {
    let (out, report) = run_merge(
        "lib.rs",
        b"use std::fmt;\n\nfn foo() {}\n",
        b"use std::io;\n\nfn bar() {}\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("use std::fmt;"));
    assert!(out.contains("use std::io;"));
    assert!(out.find("use std::io").unwrap() < out.find("fn foo").unwrap());
    assert!(out.contains("fn bar"));
}

#[test]
fn merge_rust_skips_already_present_imports_and_named_items() {
    let (out, report) = run_merge(
        "lib.rs",
        b"use std::io;\n\nfn foo() {}\n",
        b"use std::io;\nuse std::fmt;\n\nfn foo() {}\nfn bar() {}\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("use std::io;"));
    assert!(out.contains("use std::fmt;"));
    assert_eq!(out.matches("use std::io;").count(), 1);
    assert!(out.contains("fn foo()"));
    assert!(out.contains("fn bar()"));
    assert_eq!(out.matches("fn foo").count(), 1);
}

#[test]
fn merge_rust_no_additions_results_in_skip_after_merge() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(dest_dir.join("lib.rs"), "use std::fmt;\n\nfn foo() {}\n").unwrap();
    fs::write(template_dir.join("lib.rs"), "use std::fmt;\nfn foo() {}\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 1);
    assert_eq!(fs::read_to_string(dest_dir.join("lib.rs")).unwrap(), "use std::fmt;\n\nfn foo() {}\n");
}

#[test]
fn merge_typescript_export_statement_names_are_detected_via_declaration() {
    let (out, report) = run_merge(
        "main.ts",
        "export function foo() { return 1; }\n".as_bytes(),
        "export function foo() { return 2; }\nexport function bar() { return 3; }\n".as_bytes(),
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("export function foo"));
    assert!(out.contains("export function bar"));
}

#[test]
fn merge_tsx_inserts_imports() {
    let (out, report) = run_merge(
        "main.tsx",
        "import React from 'react';\n\nexport function A() { return null; }\n".as_bytes(),
        "import X from 'x';\n\nexport function B() { return null; }\n".as_bytes(),
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("import React"));
    assert!(out.contains("import X"));
    assert!(out.contains("export function B"));
}

#[test]
fn merge_css_appends_missing_rules_and_inserts_newline() {
    let (out, report) = run_merge(
        "styles.css",
        b"body { color: red; }",
        b"/* comment */\nbody { color: red; }\nhtml { color: blue; }\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("body { color: red; }"));
    assert!(out.contains("html { color: blue; }"));
    assert!(out.ends_with('\n'));
}

#[test]
fn merge_css_no_additions_results_in_skip_after_merge() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(dest_dir.join("styles.css"), "/* keep */\nbody { color: red; }\n").unwrap();
    fs::write(template_dir.join("styles.css"), "body { color: red; }\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 1);
    assert_eq!(fs::read_to_string(dest_dir.join("styles.css")).unwrap(), "/* keep */\nbody { color: red; }\n");
}

#[test]
fn merge_markdown_appends_setext_sections_and_ignores_empty_atx_titles() {
    let (out, report) = run_merge(
        "README.md",
        b"dest",
        b"Title\n=====\n\nbody\n\n## ###\n\nSub\n----\n\nmore\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.starts_with("dest\n"));
    assert!(out.contains("Title\n====="));
    assert!(out.contains("Sub\n----"));
}

#[test]
fn merge_markdown_no_additions_results_in_skip_after_merge() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(dest_dir.join("README.md"), "Title\n=====\n\nkeep\n").unwrap();
    fs::write(template_dir.join("README.md"), "Title\n=====\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 1);
    assert_eq!(fs::read_to_string(dest_dir.join("README.md")).unwrap(), "Title\n=====\n\nkeep\n");
}

#[test]
fn merge_html_appends_missing_assets_and_inserts_newline() {
    let (out, report) = run_merge(
        "index.html",
        b"<link rel=\"stylesheet\" href=\"a.css\">",
        b"<link rel=\"stylesheet\" href=\"a.css\">\n<script src=\"b.js\"></script>\n",
    );
    assert_eq!(report.updated_files, 1);
    assert!(out.contains("href=\"a.css\""));
    assert!(out.contains("src=\"b.js\""));
    assert!(out.ends_with('\n'));
}

#[test]
fn merge_html_no_additions_results_in_skip_after_merge() {
    let root = make_temp_root();
    let template_dir = root.join("template");
    let dest_dir = root.join("dest");
    fs::create_dir_all(&template_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(dest_dir.join("index.html"), "<script src=\"a.js\"></script>").unwrap();
    fs::write(template_dir.join("index.html"), "<script src=\"a.js\"></script>\n").unwrap();

    let mut decider = FixedDecider(ExistingFileAction::Merge);
    let report = pinit_core::apply_template_dir(
        &template_dir,
        &dest_dir,
        pinit_core::ApplyOptions { dry_run: false },
        &mut decider,
    )
    .unwrap();

    assert_eq!(report.updated_files, 0);
    assert_eq!(report.skipped_files, 1);
    assert_eq!(fs::read_to_string(dest_dir.join("index.html")).unwrap(), "<script src=\"a.js\"></script>");
}
