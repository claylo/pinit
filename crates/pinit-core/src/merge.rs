#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::path::Path;

use tracing::debug;
use rust_yaml::Emitter;

pub fn merge_file(rel_path: &Path, dest_bytes: &[u8], src_bytes: &[u8]) -> Option<Vec<u8>> {
    let file_name = rel_path.file_name().and_then(|s| s.to_str()).unwrap_or_default();
    if file_name == ".envrc" {
        return merge_envrc(dest_bytes, src_bytes);
    }
    if file_name == ".env" || file_name.starts_with(".env.") {
        return merge_env(dest_bytes, src_bytes);
    }

    let ext = rel_path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
    match ext.as_str() {
        "toml" => merge_toml(dest_bytes, src_bytes),
        "yml" | "yaml" => merge_yaml(dest_bytes, src_bytes),
        "rs" => merge_rust(dest_bytes, src_bytes),
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
    let Some(first) = chars.next() else { return false };
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
        let Some(dest_item) = dest.get_mut(key) else { continue };

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
    let dest_str = std::str::from_utf8(dest_bytes).ok()?;
    let src_str = std::str::from_utf8(src_bytes).ok()?;

    let lang = tree_sitter_rust::LANGUAGE;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&lang.into()).ok()?;

    let dest_tree = parser.parse(dest_str, None)?;
    let src_tree = parser.parse(src_str, None)?;

    let dest_root = dest_tree.root_node();
    let src_root = src_tree.root_node();

    let dest_items = rust_top_level_index(dest_root, dest_str.as_bytes());
    let src_items = rust_top_level_items(src_root, src_str.as_bytes());

    let mut additions = Vec::new();
    for item in src_items {
        match item.kind.as_str() {
            "use_declaration" => {
                if dest_items.use_texts.contains(&item.text) {
                    continue;
                }
                additions.push(item.text);
            }
            _ => {
                let Some(name) = item.name else { continue };
                if dest_items.named.contains(&(item.kind, name)) {
                    continue;
                }
                additions.push(item.text);
            }
        }
    }

    if additions.is_empty() {
        return Some(dest_bytes.to_vec());
    }

    debug!(added = additions.len(), "rust merge: append missing top-level items");
    let mut out = String::new();
    out.push_str(dest_str);
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

struct RustIndex {
    use_texts: HashSet<String>,
    named: HashSet<(String, String)>,
}

fn rust_top_level_index(root: tree_sitter::Node<'_>, src: &[u8]) -> RustIndex {
    let mut idx = RustIndex { use_texts: HashSet::new(), named: HashSet::new() };
    for item in rust_top_level_items(root, src) {
        if item.kind == "use_declaration" {
            idx.use_texts.insert(item.text);
            continue;
        }
        if let Some(name) = item.name {
            idx.named.insert((item.kind, name));
        }
    }
    idx
}

#[derive(Clone)]
struct RustTopLevelItem {
    kind: String,
    name: Option<String>,
    text: String,
}

fn rust_top_level_items(root: tree_sitter::Node<'_>, src: &[u8]) -> Vec<RustTopLevelItem> {
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        let kind = child.kind().to_string();
        if !is_rust_top_level_kind(&kind) {
            continue;
        }
        let text = child.utf8_text(src).unwrap_or_default().to_string();
        let name = rust_item_name(&child, src);
        out.push(RustTopLevelItem { kind, name, text });
    }
    out
}

fn is_rust_top_level_kind(kind: &str) -> bool {
    matches!(
        kind,
        "use_declaration"
            | "mod_item"
            | "function_item"
            | "struct_item"
            | "enum_item"
            | "type_item"
            | "const_item"
            | "static_item"
            | "trait_item"
    )
}

fn rust_item_name(node: &tree_sitter::Node<'_>, src: &[u8]) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        return Some(name.utf8_text(src).ok()?.to_string());
    }

    // Fallback: scan for an identifier child.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some(child.utf8_text(src).ok()?.to_string());
        }
    }
    None
}
