use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use pinit_core::{ExistingFileAction, ExistingFileDecider, ExistingFileDecisionContext};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

fn make_temp_root() -> TempRoot {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("pinit-apply-merge-treesitter-test-{}-{n}", std::process::id()));
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

fn run_merge(file_name: &str, dest_contents: &str, template_contents: &str) -> String {
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

    assert_eq!(report.created_files, 0);
    assert_eq!(report.updated_files, 1);
    assert_eq!(report.skipped_files, 0);

    fs::read_to_string(dest_dir.join(file_name)).unwrap()
}

#[test]
fn merge_python_inserts_imports_and_appends_defs() {
    let out = run_merge(
        "main.py",
        "import os\n\n\ndef foo():\n    return 1\n",
        "import sys\n\n\ndef bar():\n    return 2\n",
    );

    assert!(out.contains("import os\n"));
    assert!(out.contains("import sys\n"));
    assert!(out.contains("def foo():\n"));
    assert!(out.contains("def bar():\n"));
    assert!(out.find("import sys").unwrap() < out.find("def foo").unwrap());
}

#[test]
fn merge_javascript_inserts_imports_and_appends_exported_items() {
    let out = run_merge(
        "main.js",
        "import a from 'a';\n\nfunction foo() { return 1; }\n",
        "import b from 'b';\n\nexport function bar() { return 2; }\n",
    );

    assert!(out.contains("import a from 'a';\n"));
    assert!(out.contains("import b from 'b';\n"));
    assert!(out.contains("function foo()"));
    assert!(out.contains("export function bar()"));
    assert!(out.find("import b").unwrap() < out.find("function foo").unwrap());
}

#[test]
fn merge_typescript_inserts_imports_and_appends_types() {
    let out = run_merge(
        "main.ts",
        "import { A } from 'a';\n\ninterface Foo { a: string }\n",
        "import { B } from 'b';\n\ntype Bar = { b: number }\n",
    );

    assert!(out.contains("import { A } from 'a';\n"));
    assert!(out.contains("import { B } from 'b';\n"));
    assert!(out.contains("interface Foo"));
    assert!(out.contains("type Bar"));
    assert!(out.find("import { B }").unwrap() < out.find("interface Foo").unwrap());
}

#[test]
fn merge_php_inserts_use_after_namespace_and_appends_functions() {
    let out = run_merge(
        "main.php",
        "<?php\nnamespace Foo;\nuse A\\B;\n\nfunction foo(): void {}\n",
        "<?php\nnamespace Foo;\nuse C\\D;\n\nfunction bar(): void {}\n",
    );

    assert!(out.contains("namespace Foo;"));
    assert!(out.contains("use A\\B;"));
    assert!(out.contains("use C\\D;"));
    assert!(out.contains("function foo"));
    assert!(out.contains("function bar"));
    assert!(out.find("use C\\D").unwrap() < out.find("function foo").unwrap());
}

#[test]
fn merge_css_appends_missing_rules() {
    let out = run_merge("styles.css", ".a { color: red; }\n", ".b { color: blue; }\n");
    assert!(out.contains(".a { color: red; }"));
    assert!(out.contains(".b { color: blue; }"));
}

#[test]
fn merge_markdown_appends_missing_heading_sections() {
    let out = run_merge(
        "README.md",
        "# Title\n\nIntro.\n\n## Keep\n\nText.\n",
        "# Title\n\nIntro.\n\n## New Section\n\nStuff.\n",
    );
    assert!(out.contains("## Keep"));
    assert!(out.contains("## New Section"));
}

#[test]
fn merge_html_appends_missing_assets() {
    let out = run_merge(
        "index.html",
        "<!doctype html>\n<html><head>\n<script src=\"a.js\"></script>\n</head><body></body></html>\n",
        "<!doctype html>\n<html><head>\n<script src=\"b.js\"></script>\n<link rel=\"stylesheet\" href=\"x.css\">\n</head><body></body></html>\n",
    );

    assert!(out.contains("script src=\"a.js\""));
    assert!(out.contains("script src=\"b.js\""));
    assert!(out.contains("href=\"x.css\""));
}

#[test]
fn merge_bash_appends_missing_function() {
    let out = run_merge("script.sh", "foo() { echo foo; }\n", "bar() { echo bar; }\n");
    assert!(out.contains("foo()"));
    assert!(out.contains("bar()"));
}

#[test]
fn merge_zsh_appends_missing_function() {
    let out = run_merge("script.zsh", "foo() { echo foo; }\n", "bar() { echo bar; }\n");
    assert!(out.contains("foo()"));
    assert!(out.contains("bar()"));
}

#[test]
fn merge_lua_appends_missing_function() {
    let out = run_merge(
        "script.lua",
        "function foo()\n  return 1\nend\n",
        "function bar()\n  return 2\nend\n",
    );
    assert!(out.contains("function foo()"));
    assert!(out.contains("function bar()"));
}

#[test]
fn merge_ruby_appends_missing_class() {
    let out = run_merge("script.rb", "class Foo\nend\n", "class Bar\nend\n");
    assert!(out.contains("class Foo"));
    assert!(out.contains("class Bar"));
}

