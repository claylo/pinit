#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::path::Path;

use rust_yaml::Emitter;
use tracing::debug;

pub fn merge_file(rel_path: &Path, dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    let file_name = rel_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if file_name == ".envrc" {
        return merge_envrc(dest_bytes, src_bytes);
    }
    if file_name == ".env" || file_name.starts_with(".env.") {
        return merge_env(dest_bytes, src_bytes);
    }

    let ext = rel_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "toml" => merge_toml(dest_bytes, src_bytes),
        "yml" | "yaml" => merge_yaml(dest_bytes, src_bytes),
        "rs" => merge_rust(dest_bytes, src_bytes),
        "php" => merge_php(dest_bytes, src_bytes),
        "py" => merge_python(dest_bytes, src_bytes),
        "js" | "mjs" | "cjs" => merge_javascript(dest_bytes, src_bytes),
        "ts" => merge_typescript(dest_bytes, src_bytes),
        "tsx" => merge_tsx(dest_bytes, src_bytes),
        "css" => merge_css(dest_bytes, src_bytes),
        "md" | "markdown" => merge_markdown(dest_bytes, src_bytes),
        "lua" => merge_lua(dest_bytes, src_bytes),
        "sh" | "bash" => merge_bash(dest_bytes, src_bytes),
        "zsh" => merge_zsh(dest_bytes, src_bytes),
        "rb" => merge_ruby(dest_bytes, src_bytes),
        "html" | "htm" => merge_html(dest_bytes, src_bytes),
        _ => merge_lines(dest_bytes, src_bytes),
    }
}

fn merge_lines(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    let dest = std::str::from_utf8(dest_bytes).ok()?;
    let src = std::str::from_utf8(src_bytes).ok()?;

    let mut have: HashSet<&str> = HashSet::new();
    for line in dest.lines() {
        have.insert(line);
    }

    let mut out = String::new();
    out.push_str(dest);
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }

    let mut appended = 0usize;
    for line in src.lines() {
        if have.contains(line) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        appended += 1;
    }

    if appended == 0 {
        return Some(dest_bytes.to_vec());
    }
    Some(out.into_bytes())
}

fn merge_env(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    let dest = std::str::from_utf8(dest_bytes).ok()?;
    let src = std::str::from_utf8(src_bytes).ok()?;

    let mut have: HashSet<String> = HashSet::new();
    for line in dest.lines() {
        let key = env_key(line)?;
        have.insert(key);
    }

    let mut missing = Vec::new();
    for line in src.lines() {
        let key = match env_key(line) {
            Some(k) => k,
            None => continue,
        };
        if have.contains(&key) {
            continue;
        }
        missing.push(line);
    }

    if missing.is_empty() {
        return Some(dest_bytes.to_vec());
    }

    let mut out = String::new();
    out.push_str(dest);
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out.push('\n');
    for line in missing {
        out.push_str(line);
        out.push('\n');
    }
    Some(out.into_bytes())
}

fn env_key(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let mut s = trimmed;
    if let Some(rest) = s.strip_prefix("export ") {
        s = rest.trim_start();
    }
    let (key, _rest) = s.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    if !is_env_ident(key) {
        return None;
    }
    Some(key.to_string())
}

fn is_env_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn merge_envrc(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    let dest = std::str::from_utf8(dest_bytes).ok()?;
    let src = std::str::from_utf8(src_bytes).ok()?;

    let mut have_lines: HashSet<&str> = HashSet::new();
    let mut have_vars: HashSet<String> = HashSet::new();

    for line in dest.lines() {
        have_lines.insert(line);
        if let Some(var) = envrc_var(line) {
            have_vars.insert(var);
        }
    }

    let mut to_add = Vec::new();
    for line in src.lines() {
        if have_lines.contains(line) {
            continue;
        }
        if let Some(var) = envrc_var(line) {
            if have_vars.contains(&var) {
                continue;
            }
        }
        to_add.push(line);
    }

    if to_add.is_empty() {
        return Some(dest_bytes.to_vec());
    }

    let mut out = String::new();
    out.push_str(dest);
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out.push('\n');
    for line in to_add {
        out.push_str(line);
        out.push('\n');
    }
    Some(out.into_bytes())
}

fn envrc_var(line: &str) -> Option<String> {
    let mut s = line.trim_start();
    if s.starts_with('#') || s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_prefix("export") {
        s = rest.trim_start();
    }
    let (var, _rest) = s.split_once('=')?;
    let var = var.trim();
    if !is_env_ident(var) {
        return None;
    }
    Some(var.to_string())
}

fn merge_toml(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    let dest_str = std::str::from_utf8(dest_bytes).ok()?;
    let src_str = std::str::from_utf8(src_bytes).ok()?;

    let mut dest_doc: toml_edit::DocumentMut = dest_str.parse().ok()?;
    let src_doc: toml_edit::DocumentMut = src_str.parse().ok()?;

    merge_toml_table(dest_doc.as_table_mut(), src_doc.as_table());
    Some(dest_doc.to_string().into_bytes())
}

fn merge_toml_table(dest: &mut toml_edit::Table, src: &toml_edit::Table) {
    for (key, src_item) in src.iter() {
        if !dest.contains_key(key) {
            dest.insert(key, src_item.clone());
            continue;
        }
        let Some(dest_item) = dest.get_mut(key) else {
            continue;
        };

        match (dest_item, src_item) {
            (toml_edit::Item::Table(dest_table), toml_edit::Item::Table(src_table)) => {
                merge_toml_table(dest_table, src_table);
            }
            (toml_edit::Item::Value(_), toml_edit::Item::Value(_)) => {}
            (toml_edit::Item::ArrayOfTables(_), toml_edit::Item::ArrayOfTables(_)) => {}
            _ => {}
        }
    }
}

fn merge_yaml(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    let dest_str = std::str::from_utf8(dest_bytes).ok()?;
    let src_str = std::str::from_utf8(src_bytes).ok()?;

    let yaml = rust_yaml::Yaml::new();
    let mut dest_val = yaml.load_str(dest_str).ok()?;
    let src_val = yaml.load_str(src_str).ok()?;

    merge_yaml_value(&mut dest_val, &src_val);

    let mut out = Vec::new();
    let mut emitter = rust_yaml::BasicEmitter::new();
    emitter.emit(&dest_val, &mut out).ok()?;
    Some(out)
}

fn merge_yaml_value(dest: &mut rust_yaml::Value, src: &rust_yaml::Value) {
    match (dest, src) {
        (rust_yaml::Value::Mapping(dest_map), rust_yaml::Value::Mapping(src_map)) => {
            for (k, v) in src_map.iter() {
                if !dest_map.contains_key(k) {
                    dest_map.insert(k.clone(), v.clone());
                    continue;
                }
                if let Some(dest_v) = dest_map.get_mut(k) {
                    merge_yaml_value(dest_v, v);
                }
            }
        }
        _ => {}
    }
}

fn merge_rust(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_named_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_rust::LANGUAGE.into(),
        LangMergeRules {
            import_like: &["use"],
            named_like: &[
                "function", "struct", "enum", "trait", "type", "const", "static", "mod",
            ],
            skip_if_dest_has_namespace: false,
        },
        "rust",
    )
}

fn merge_php(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_named_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_php::LANGUAGE_PHP.into(),
        LangMergeRules {
            import_like: &["use", "namespace"],
            named_like: &["function", "class", "interface", "trait", "enum"],
            skip_if_dest_has_namespace: true,
        },
        "php",
    )
}

fn merge_python(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_named_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_python::LANGUAGE.into(),
        LangMergeRules {
            import_like: &["import"],
            named_like: &["function", "class"],
            skip_if_dest_has_namespace: false,
        },
        "python",
    )
}

fn merge_javascript(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_named_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_javascript::LANGUAGE.into(),
        LangMergeRules {
            import_like: &["import"],
            named_like: &["export", "function", "class"],
            skip_if_dest_has_namespace: false,
        },
        "javascript",
    )
}

fn merge_typescript(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_named_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        LangMergeRules {
            import_like: &["import"],
            named_like: &["export", "function", "class", "interface", "type", "enum"],
            skip_if_dest_has_namespace: false,
        },
        "typescript",
    )
}

fn merge_tsx(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_named_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_typescript::LANGUAGE_TSX.into(),
        LangMergeRules {
            import_like: &["import"],
            named_like: &["export", "function", "class", "interface", "type", "enum"],
            skip_if_dest_has_namespace: false,
        },
        "tsx",
    )
}

fn merge_lua(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_named_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_lua::LANGUAGE.into(),
        LangMergeRules {
            import_like: &[],
            named_like: &["function"],
            skip_if_dest_has_namespace: false,
        },
        "lua",
    )
}

fn merge_ruby(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_named_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_ruby::LANGUAGE.into(),
        LangMergeRules {
            import_like: &["require"],
            named_like: &["class", "module", "method", "def"],
            skip_if_dest_has_namespace: false,
        },
        "ruby",
    )
}

fn merge_bash(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_named_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_bash::LANGUAGE.into(),
        LangMergeRules {
            import_like: &[],
            named_like: &["function"],
            skip_if_dest_has_namespace: false,
        },
        "bash",
    )
}

fn merge_zsh(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_named_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_zsh::LANGUAGE.into(),
        LangMergeRules {
            import_like: &[],
            named_like: &["function"],
            skip_if_dest_has_namespace: false,
        },
        "zsh",
    )
}

fn merge_css(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_tree_sitter_text_top_level(
        dest_bytes,
        src_bytes,
        tree_sitter_css::LANGUAGE.into(),
        &["rule", "at_rule"],
        "css",
    )
}

fn merge_markdown(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_markdown_sections(dest_bytes, src_bytes)
}

fn merge_html(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    merge_html_assets(dest_bytes, src_bytes)
}

#[derive(Clone, Copy)]
struct LangMergeRules {
    import_like: &'static [&'static str],
    named_like: &'static [&'static str],
    skip_if_dest_has_namespace: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TsKey {
    Text(String),
    Named { kind: String, name: String },
}

fn merge_tree_sitter_named_top_level(
    dest_bytes: &[u8],
    src_bytes: &[u8],
    language: tree_sitter::Language,
    rules: LangMergeRules,
    label: &'static str,
) -> Option<Vec<u8>> {
    let dest_str = std::str::from_utf8(dest_bytes).ok()?;
    let src_str = std::str::from_utf8(src_bytes).ok()?;

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;

    let dest_tree = parser.parse(dest_str, None)?;
    let src_tree = parser.parse(src_str, None)?;

    let dest_root = dest_tree.root_node();
    let src_root = src_tree.root_node();

    let dest_items = ts_top_level_items(dest_root, dest_str.as_bytes(), rules, false);
    let dest_has_namespace = dest_items.iter().any(|i| i.is_namespace);
    if rules.skip_if_dest_has_namespace && dest_has_namespace {
        debug!(
            lang = label,
            "namespace present in dest; skip namespace merges"
        );
    }

    let insertion_byte = ts_import_insertion_byte(&dest_items);

    let mut dest_keys: HashSet<TsKey> = HashSet::new();
    for item in &dest_items {
        if item.is_import {
            dest_keys.insert(TsKey::Text(normalize_ws(&item.text)));
        }
        if item.is_named {
            if let Some(name) = &item.name {
                dest_keys.insert(TsKey::Named {
                    kind: item.kind.clone(),
                    name: name.clone(),
                });
            }
        }
    }

    let src_items = ts_top_level_items(src_root, src_str.as_bytes(), rules, dest_has_namespace);

    let mut missing_imports: Vec<String> = Vec::new();
    let mut missing_named: Vec<String> = Vec::new();
    for item in src_items {
        if item.is_import {
            let key = TsKey::Text(normalize_ws(&item.text));
            if dest_keys.contains(&key) {
                continue;
            }
            dest_keys.insert(key);
            missing_imports.push(item.text);
            continue;
        }

        if item.is_named {
            let Some(name) = item.name else { continue };
            let key = TsKey::Named {
                kind: item.kind,
                name,
            };
            if dest_keys.contains(&key) {
                continue;
            }
            dest_keys.insert(key);
            missing_named.push(item.text);
        }
    }

    if missing_imports.is_empty() && missing_named.is_empty() {
        return Some(dest_bytes.to_vec());
    }

    let mut out = dest_bytes.to_vec();

    if !missing_imports.is_empty() {
        debug!(
            lang = label,
            added = missing_imports.len(),
            "insert missing imports"
        );
        let at = insertion_byte.min(out.len());
        let mut merged = Vec::with_capacity(out.len() + 256);
        merged.extend_from_slice(&out[..at]);

        if !merged.is_empty() && *merged.last().unwrap() != b'\n' {
            merged.push(b'\n');
        }
        for text in &missing_imports {
            merged.extend_from_slice(text.trim_end().as_bytes());
            merged.push(b'\n');
        }

        merged.extend_from_slice(&out[at..]);
        out = merged;
    }

    if !missing_named.is_empty() {
        debug!(
            lang = label,
            added = missing_named.len(),
            "append missing named items"
        );
        if !out.is_empty() && *out.last().unwrap() != b'\n' {
            out.push(b'\n');
        }
        for text in &missing_named {
            out.push(b'\n');
            out.extend_from_slice(text.trim_end().as_bytes());
            out.push(b'\n');
        }
    }

    Some(out)
}

#[derive(Clone)]
struct TsTopLevelItem {
    kind: String,
    kind_lower: String,
    end_byte: usize,
    text: String,
    name: Option<String>,
    is_namespace: bool,
    is_import: bool,
    is_named: bool,
}

fn ts_top_level_items(
    root: tree_sitter::Node<'_>,
    bytes: &[u8],
    rules: LangMergeRules,
    dest_has_namespace: bool,
) -> Vec<TsTopLevelItem> {
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        let kind = child.kind();
        let kind_lower = kind.to_ascii_lowercase();
        let text = child.utf8_text(bytes).unwrap_or_default().to_string();
        let is_namespace = kind_lower.contains("namespace") && !kind_lower.contains("use");

        if rules.skip_if_dest_has_namespace && dest_has_namespace && is_namespace {
            continue;
        }

        let is_import = ts_is_import_like(&child, &kind_lower, rules, bytes);
        let is_named = !is_import && contains_any(&kind_lower, rules.named_like);

        let name = if is_named {
            ts_item_name(&child, bytes)
        } else {
            None
        };

        out.push(TsTopLevelItem {
            kind: kind.to_string(),
            kind_lower,
            end_byte: child.end_byte(),
            text,
            name,
            is_namespace,
            is_import,
            is_named,
        });
    }
    out
}

fn ts_import_insertion_byte(items: &[TsTopLevelItem]) -> usize {
    let mut insert_after = 0usize;
    let mut in_preamble = true;
    for item in items {
        if !in_preamble {
            break;
        }
        let is_comment_like = item.kind_lower.contains("comment");
        let is_shebang_like =
            item.kind_lower.contains("shebang") || item.kind_lower.contains("hash_bang");
        if item.is_namespace || item.is_import || is_comment_like || is_shebang_like {
            insert_after = item.end_byte;
            continue;
        }
        in_preamble = false;
    }
    insert_after
}

fn contains_any(haystack: &str, needles: &'static [&'static str]) -> bool {
    needles
        .iter()
        .any(|n| !n.is_empty() && haystack.contains(n))
}

fn ts_is_import_like(
    node: &tree_sitter::Node<'_>,
    kind_lower: &str,
    rules: LangMergeRules,
    src: &[u8],
) -> bool {
    if contains_any(kind_lower, rules.import_like) {
        return true;
    }

    // ruby: `require "x"` often parses as a call/command rather than a `require` node kind.
    if rules.import_like.iter().any(|s| *s == "require")
        && (kind_lower == "call" || kind_lower == "command")
    {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "identifier" {
                if let Ok(name) = child.utf8_text(src) {
                    let name = name.trim();
                    if name == "require" || name == "require_relative" {
                        return true;
                    }
                }
                break;
            }
        }
    }

    false
}

fn ts_item_name(node: &tree_sitter::Node<'_>, src: &[u8]) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        return Some(name.utf8_text(src).ok()?.to_string());
    }
    if let Some(decl) = node.child_by_field_name("declaration") {
        if let Some(name) = ts_item_name(&decl, src) {
            return Some(name);
        }
    }
    // Many grammars use "identifier" nodes.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some(child.utf8_text(src).ok()?.to_string());
        }
    }
    None
}

fn merge_tree_sitter_text_top_level(
    dest_bytes: &[u8],
    src_bytes: &[u8],
    language: tree_sitter::Language,
    kind_substrings: &'static [&'static str],
    label: &'static str,
) -> Option<Vec<u8>> {
    let dest_str = std::str::from_utf8(dest_bytes).ok()?;
    let src_str = std::str::from_utf8(src_bytes).ok()?;

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;

    let dest_tree = parser.parse(dest_str, None)?;
    let src_tree = parser.parse(src_str, None)?;

    let dest_keys = ts_text_keys(dest_tree.root_node(), dest_str.as_bytes(), kind_substrings);
    let src_items = ts_text_items(src_tree.root_node(), src_str.as_bytes(), kind_substrings);

    let mut out_items = Vec::new();
    for (key, text) in src_items {
        if dest_keys.contains(&key) {
            continue;
        }
        out_items.push(text);
    }

    if out_items.is_empty() {
        return Some(dest_bytes.to_vec());
    }

    debug!(
        lang = label,
        added = out_items.len(),
        "append missing top-level blocks"
    );
    let mut out = String::new();
    out.push_str(dest_str);
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    for text in out_items {
        out.push('\n');
        out.push_str(text.trim_end());
        out.push('\n');
    }
    Some(out.into_bytes())
}

fn ts_text_keys(
    root: tree_sitter::Node<'_>,
    bytes: &[u8],
    substrings: &'static [&'static str],
) -> HashSet<String> {
    let mut keys = HashSet::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        let kind_lower = child.kind().to_ascii_lowercase();
        if !contains_any(&kind_lower, substrings) {
            continue;
        }
        let text = child.utf8_text(bytes).unwrap_or_default();
        keys.insert(normalize_ws(text));
    }
    keys
}

fn ts_text_items(
    root: tree_sitter::Node<'_>,
    bytes: &[u8],
    substrings: &'static [&'static str],
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        let kind_lower = child.kind().to_ascii_lowercase();
        if !contains_any(&kind_lower, substrings) {
            continue;
        }
        let text = child.utf8_text(bytes).unwrap_or_default().to_string();
        out.push((normalize_ws(&text), text));
    }
    out
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn merge_markdown_sections(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    let dest = std::str::from_utf8(dest_bytes).ok()?;
    let src = std::str::from_utf8(src_bytes).ok()?;

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_md::LANGUAGE.into()).ok()?;

    let dest_tree = parser.parse(dest, None)?;
    let src_tree = parser.parse(src, None)?;

    let mut dest_headings = markdown_heading_set(dest_tree.root_node(), dest.as_bytes());
    let src_sections = markdown_sections(src_tree.root_node(), src.as_bytes());

    let mut additions = Vec::new();
    for section in src_sections {
        if dest_headings.contains(&section.heading_key) {
            continue;
        }
        for hk in &section.heading_keys_in_section {
            dest_headings.insert(hk.clone());
        }
        additions.push(section.text);
    }

    if additions.is_empty() {
        return Some(dest_bytes.to_vec());
    }

    debug!(
        lang = "markdown",
        added = additions.len(),
        "append missing heading sections"
    );
    let mut out = String::new();
    out.push_str(dest);
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    for section in additions {
        out.push('\n');
        out.push_str(section.trim_end());
        out.push('\n');
    }
    Some(out.into_bytes())
}

#[derive(Clone)]
struct MdSection {
    heading_key: String,
    heading_keys_in_section: Vec<String>,
    text: String,
}

fn markdown_heading_set(root: tree_sitter::Node<'_>, bytes: &[u8]) -> HashSet<String> {
    let mut set = HashSet::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind().to_ascii_lowercase().contains("heading") {
            if let Some(key) = markdown_heading_key(node, bytes) {
                set.insert(key);
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    set
}

fn markdown_sections(root: tree_sitter::Node<'_>, bytes: &[u8]) -> Vec<MdSection> {
    let mut headings = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind().to_ascii_lowercase().contains("heading") {
            if let Some((key, level)) = markdown_heading_key_and_level(node, bytes) {
                headings.push((node.start_byte(), key, level));
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    headings.sort_by_key(|(start, _, _)| *start);

    let mut sections = Vec::new();
    for (idx, (start, key, level)) in headings.iter().enumerate() {
        let next_start = headings
            .get(idx + 1)
            .map(|(s, _, _)| *s)
            .unwrap_or(bytes.len());
        let text = String::from_utf8_lossy(&bytes[*start..next_start]).to_string();

        // Collect headings inside this section so if we add it, we don't re-add nested headings later.
        let mut inner = Vec::new();
        for (s, k, l) in headings.iter().skip(idx + 1) {
            if *s >= next_start {
                break;
            }
            if *l >= *level {
                inner.push(k.clone());
            }
        }
        inner.insert(0, key.clone());
        sections.push(MdSection {
            heading_key: key.clone(),
            heading_keys_in_section: inner,
            text,
        });
    }
    sections
}

fn markdown_heading_key(node: tree_sitter::Node<'_>, bytes: &[u8]) -> Option<String> {
    markdown_heading_key_and_level(node, bytes).map(|(k, _)| k)
}

fn markdown_heading_key_and_level(
    node: tree_sitter::Node<'_>,
    bytes: &[u8],
) -> Option<(String, usize)> {
    let text = node.utf8_text(bytes).ok()?;
    let first_line = text.lines().next().unwrap_or("").trim();
    if first_line.starts_with('#') {
        let hashes = first_line.chars().take_while(|c| *c == '#').count();
        let title = first_line[hashes..].trim().trim_end_matches('#').trim();
        if title.is_empty() {
            return None;
        }
        return Some((normalize_ws(title), hashes));
    }

    // setext: take the first line as title, level from underline char
    let mut lines = text.lines();
    let title = lines.next().unwrap_or("").trim();
    let underline = lines.next().unwrap_or("").trim();
    if underline.chars().all(|c| c == '=') {
        return Some((normalize_ws(title), 1));
    }
    if underline.chars().all(|c| c == '-') {
        return Some((normalize_ws(title), 2));
    }
    None
}

fn merge_html_assets(dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    let dest = std::str::from_utf8(dest_bytes).ok()?;
    let src = std::str::from_utf8(src_bytes).ok()?;

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_html::LANGUAGE.into())
        .ok()?;

    let dest_tree = parser.parse(dest, None)?;
    let src_tree = parser.parse(src, None)?;

    let dest_assets = html_asset_keys(dest_tree.root_node(), dest.as_bytes());
    let src_assets = html_assets(src_tree.root_node(), src.as_bytes());

    let mut additions = Vec::new();
    for asset in src_assets {
        if dest_assets.contains(&asset.key) {
            continue;
        }
        additions.push(asset.text);
    }

    if additions.is_empty() {
        return Some(dest_bytes.to_vec());
    }

    debug!(
        lang = "html",
        added = additions.len(),
        "append missing html assets"
    );
    let mut out = String::new();
    out.push_str(dest);
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    for text in additions {
        out.push('\n');
        out.push_str(text.trim_end());
        out.push('\n');
    }
    Some(out.into_bytes())
}

#[derive(Clone)]
struct HtmlAsset {
    key: String,
    text: String,
}

fn html_asset_keys(root: tree_sitter::Node<'_>, bytes: &[u8]) -> HashSet<String> {
    let mut set = HashSet::new();
    for asset in html_assets(root, bytes) {
        set.insert(asset.key);
    }
    set
}

fn html_assets(root: tree_sitter::Node<'_>, bytes: &[u8]) -> Vec<HtmlAsset> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let kind = node.kind().to_ascii_lowercase();
        if kind.contains("element") {
            if let Some(asset) = html_asset_from_element(node, bytes) {
                out.push(asset);
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            stack.push(child);
        }
    }
    out
}

fn html_asset_from_element(node: tree_sitter::Node<'_>, bytes: &[u8]) -> Option<HtmlAsset> {
    let start_tag = find_html_start_tag(node)?;
    let tag_name = html_tag_name(start_tag, bytes)?;

    match tag_name.as_str() {
        "script" => {
            let src = html_attr_value(start_tag, "src", bytes)?;
            let text = node.utf8_text(bytes).ok()?.to_string();
            Some(HtmlAsset {
                key: format!("script:{}", normalize_ws(&src)),
                text,
            })
        }
        "link" => {
            let href = html_attr_value(start_tag, "href", bytes)?;
            let text = node.utf8_text(bytes).ok()?.to_string();
            Some(HtmlAsset {
                key: format!("link:{}", normalize_ws(&href)),
                text,
            })
        }
        _ => None,
    }
}

fn find_html_start_tag(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let kind = child.kind();
        if kind == "start_tag" || kind == "self_closing_tag" {
            return Some(child);
        }
    }
    None
}

fn html_tag_name(node: tree_sitter::Node<'_>, bytes: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "tag_name" {
            return Some(
                child
                    .utf8_text(bytes)
                    .ok()?
                    .to_string()
                    .to_ascii_lowercase(),
            );
        }
    }
    None
}

fn html_attr_value(node: tree_sitter::Node<'_>, attr_name: &str, bytes: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "attribute" {
            continue;
        }
        let mut attr_cursor = child.walk();
        let mut name_text = None;
        let mut value_text = None;
        for grand in child.named_children(&mut attr_cursor) {
            match grand.kind() {
                "attribute_name" => {
                    name_text = grand.utf8_text(bytes).ok().map(|s| s.to_ascii_lowercase());
                }
                "attribute_value" | "quoted_attribute_value" => {
                    value_text = grand.utf8_text(bytes).ok().map(|s| s.trim().to_string());
                }
                _ => {}
            }
        }
        if name_text.as_deref()? != attr_name {
            continue;
        }
        let raw = value_text?;
        return Some(raw.trim_matches(&['"', '\''][..]).to_string());
    }
    None
}
