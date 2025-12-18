#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub common: Option<String>,

    #[serde(default)]
    pub sources: Vec<Source>,

    #[serde(default)]
    pub templates: BTreeMap<String, TemplateDef>,

    #[serde(default)]
    pub targets: BTreeMap<String, Vec<String>>,

    #[serde(default)]
    pub recipes: BTreeMap<String, RecipeDef>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Source {
    pub name: String,

    pub path: Option<PathBuf>,
    pub repo: Option<String>,

    #[serde(rename = "ref")]
    pub git_ref: Option<String>,

    pub subdir: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum TemplateDef {
    Path(PathBuf),
    Detailed { source: Option<String>, path: PathBuf },
}

impl TemplateDef {
    pub fn path(&self) -> &Path {
        match self {
            TemplateDef::Path(path) => path.as_path(),
            TemplateDef::Detailed { path, .. } => path.as_path(),
        }
    }

    pub fn source(&self) -> Option<&str> {
        match self {
            TemplateDef::Path(_) => None,
            TemplateDef::Detailed { source, .. } => source.as_deref(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct RecipeDef {
    #[serde(default)]
    pub templates: Vec<String>,

    #[serde(default)]
    pub files: Vec<FileSetDef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FileSetDef {
    pub root: PathBuf,

    #[serde(default)]
    pub include: Vec<String>,

    pub dest_prefix: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedRecipe {
    pub name: String,
    pub templates: Vec<String>,
    pub files: Vec<FileSetDef>,
}

#[derive(Debug)]
pub enum ConfigError {
    NotFound,
    Io { path: PathBuf, source: io::Error },
    ParseToml { path: PathBuf, source: toml::de::Error },
    ParseYaml { path: PathBuf, message: String },
    YamlRootNotMapping { path: PathBuf },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::NotFound => write!(f, "no config file found"),
            ConfigError::Io { path, source } => write!(f, "{}: {}", path.display(), source),
            ConfigError::ParseToml { path, source } => write!(f, "{}: {}", path.display(), source),
            ConfigError::ParseYaml { path, message } => write!(f, "{}: {}", path.display(), message),
            ConfigError::YamlRootNotMapping { path } => write!(f, "{}: YAML root must be a mapping", path.display()),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::Io { source, .. } => Some(source),
            ConfigError::ParseToml { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub fn default_config_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        let xdg = PathBuf::from(xdg);
        out.push(xdg.join("pinit.toml"));
        out.push(xdg.join("pinit.yaml"));
        out.push(xdg.join("pinit.yml"));
        return out;
    }

    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        let config = home.join(".config");
        out.push(config.join("pinit.toml"));
        out.push(config.join("pinit.yaml"));
        out.push(config.join("pinit.yml"));
    }

    out
}

pub fn load_config(path_override: Option<&Path>) -> Result<(PathBuf, Config), ConfigError> {
    if let Some(path) = path_override {
        return load_config_at(path);
    }

    for path in default_config_paths() {
        if path.is_file() {
            return load_config_at(&path);
        }
    }

    Err(ConfigError::NotFound)
}

fn load_config_at(path: &Path) -> Result<(PathBuf, Config), ConfigError> {
    let content = fs::read_to_string(path).map_err(|e| ConfigError::Io { path: path.to_path_buf(), source: e })?;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();

    let config = match ext.as_str() {
        "toml" => parse_toml(path, &content)?,
        "yaml" | "yml" => parse_yaml(path, &content)?,
        _ => {
            if let Ok(cfg) = parse_toml(path, &content) {
                cfg
            } else {
                parse_yaml(path, &content)?
            }
        }
    };

    Ok((path.to_path_buf(), config))
}

fn parse_toml(path: &Path, s: &str) -> Result<Config, ConfigError> {
    toml::from_str::<Config>(s).map_err(|e| ConfigError::ParseToml { path: path.to_path_buf(), source: e })
}

fn parse_yaml(path: &Path, s: &str) -> Result<Config, ConfigError> {
    // rust-yaml is intentionally used instead of serde_yaml (deprecated).
    //
    // This is a minimal parser that supports the subset of YAML we need for config.
    let yaml = rust_yaml::Yaml::new();
    let doc = yaml
        .load_str(s)
        .map_err(|e| ConfigError::ParseYaml { path: path.to_path_buf(), message: e.to_string() })?;

    yaml_to_config(path, &doc)
}

fn yaml_to_config(path: &Path, root: &rust_yaml::Value) -> Result<Config, ConfigError> {
    let rust_yaml::Value::Mapping(map) = root else {
        return Err(ConfigError::YamlRootNotMapping { path: path.to_path_buf() });
    };

    let mut cfg = Config::default();

    cfg.common = yaml_get_string(map, "common");

    if let Some(sources) = yaml_get_seq(map, "sources") {
        for source in sources {
            let Some(source_map) = yaml_as_mapping(source) else { continue };
            let Some(name) = yaml_get_string(source_map, "name") else { continue };
            let path_val = yaml_get_string(source_map, "path").map(PathBuf::from);
            let repo = yaml_get_string(source_map, "repo");
            let git_ref = yaml_get_string(source_map, "ref");
            let subdir = yaml_get_string(source_map, "subdir").map(PathBuf::from);
            cfg.sources.push(Source { name, path: path_val, repo, git_ref, subdir });
        }
    }

    if let Some(templates_root) = yaml_get(map, "templates").and_then(yaml_as_mapping) {
        for (k, v) in templates_root {
            let Some(name) = yaml_as_string(k) else { continue };

            if let Some(path_str) = yaml_as_string(v) {
                cfg.templates.insert(name, TemplateDef::Path(PathBuf::from(path_str)));
                continue;
            }

            if let Some(d) = yaml_as_mapping(v) {
                let source = yaml_get_string(d, "source");
                let Some(path_str) = yaml_get_string(d, "path") else { continue };
                cfg.templates.insert(name, TemplateDef::Detailed { source, path: PathBuf::from(path_str) });
            }
        }
    }

    if let Some(targets_root) = yaml_get(map, "targets").and_then(yaml_as_mapping) {
        for (k, v) in targets_root {
            let Some(name) = yaml_as_string(k) else { continue };
            let Some(items) = yaml_as_vec_of_strings(v) else { continue };
            cfg.targets.insert(name, items);
        }
    }

    if let Some(recipes_root) = yaml_get(map, "recipes").and_then(yaml_as_mapping) {
        for (k, v) in recipes_root {
            let Some(name) = yaml_as_string(k) else { continue };
            let Some(recipe_map) = yaml_as_mapping(v) else { continue };

            let templates = yaml_get_vec_of_strings(recipe_map, "templates").unwrap_or_default();
            let mut files = Vec::new();
            if let Some(files_seq) = yaml_get_seq(recipe_map, "files") {
                for fs_item in files_seq {
                    let Some(fs_map) = yaml_as_mapping(fs_item) else { continue };
                    let Some(root) = yaml_get_string(fs_map, "root").map(PathBuf::from) else { continue };
                    let include = yaml_get_vec_of_strings(fs_map, "include").unwrap_or_default();
                    let dest_prefix = yaml_get_string(fs_map, "dest_prefix").map(PathBuf::from);
                    files.push(FileSetDef { root, include, dest_prefix });
                }
            }

            cfg.recipes.insert(name, RecipeDef { templates, files });
        }
    }

    // Validate that the YAML did not contain multiple documents, anchors, etc is out-of-scope for v1.
    let _ = path;
    Ok(cfg)
}

fn yaml_key(s: &str) -> rust_yaml::Value {
    rust_yaml::Value::String(s.to_string())
}

fn yaml_get<'a>(map: &'a IndexMap<rust_yaml::Value, rust_yaml::Value>, key: &str) -> Option<&'a rust_yaml::Value> {
    map.get(&yaml_key(key))
}

fn yaml_get_seq<'a>(map: &'a IndexMap<rust_yaml::Value, rust_yaml::Value>, key: &str) -> Option<&'a Vec<rust_yaml::Value>> {
    yaml_get(map, key).and_then(|v| match v {
        rust_yaml::Value::Sequence(seq) => Some(seq),
        _ => None,
    })
}

fn yaml_get_string(map: &IndexMap<rust_yaml::Value, rust_yaml::Value>, key: &str) -> Option<String> {
    yaml_get(map, key).and_then(yaml_as_string)
}

fn yaml_get_vec_of_strings(map: &IndexMap<rust_yaml::Value, rust_yaml::Value>, key: &str) -> Option<Vec<String>> {
    yaml_get(map, key).and_then(yaml_as_vec_of_strings)
}

fn yaml_as_mapping(y: &rust_yaml::Value) -> Option<&IndexMap<rust_yaml::Value, rust_yaml::Value>> {
    match y {
        rust_yaml::Value::Mapping(m) => Some(m),
        _ => None,
    }
}

fn yaml_as_string(y: &rust_yaml::Value) -> Option<String> {
    match y {
        rust_yaml::Value::String(s) => Some(s.to_string()),
        rust_yaml::Value::Int(i) => Some(i.to_string()),
        rust_yaml::Value::Float(f) => Some(f.to_string()),
        rust_yaml::Value::Bool(b) => Some(if *b { "true" } else { "false" }.to_string()),
        _ => None,
    }
}

fn yaml_as_vec_of_strings(y: &rust_yaml::Value) -> Option<Vec<String>> {
    match y {
        rust_yaml::Value::Sequence(seq) => {
            let mut out = Vec::new();
            for item in seq {
                let Some(s) = yaml_as_string(item) else { return None };
                out.push(s);
            }
            Some(out)
        }
        _ => None,
    }
}

impl Config {
    pub fn resolve_recipe(&self, name: &str) -> Option<ResolvedRecipe> {
        if let Some(def) = self.recipes.get(name) {
            return Some(ResolvedRecipe { name: name.to_string(), templates: def.templates.clone(), files: def.files.clone() });
        }

        if let Some(stack) = self.targets.get(name) {
            return Some(ResolvedRecipe { name: name.to_string(), templates: stack.clone(), files: Vec::new() });
        }

        if self.templates.contains_key(name) {
            let mut templates = Vec::new();
            if let Some(common) = self.common.as_deref() {
                if common != name {
                    templates.push(common.to_string());
                }
            }
            templates.push(name.to_string());
            return Some(ResolvedRecipe { name: name.to_string(), templates, files: Vec::new() });
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn make_temp_root() -> PathBuf {
        static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);
        let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("pinit-config-test-{}-{n}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn parses_toml_and_resolves_target() {
        let cfg: Config = toml::from_str(
            r#"
common = "common"

[templates]
common = "common"
rust = "rust"

[targets]
rust = ["common", "rust"]
"#,
        )
        .unwrap();

        let resolved = cfg.resolve_recipe("rust").unwrap();
        assert_eq!(resolved.templates, vec!["common".to_string(), "rust".to_string()]);
    }

    #[test]
    fn parses_yaml_and_resolves_target() {
        let yaml = r#"
common: common
templates:
  common: common
  rust: rust
targets:
  rust: [common, rust]
"#;

        let root = make_temp_root();
        let path = root.join("pinit.yaml");
        fs::write(&path, yaml).unwrap();

        let (_, cfg) = load_config(Some(&path)).unwrap();
        let resolved = cfg.resolve_recipe("rust").unwrap();
        assert_eq!(resolved.templates, vec!["common".to_string(), "rust".to_string()]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn recipe_can_be_defined_inline_with_filesets() {
        let cfg: Config = toml::from_str(
            r#"
[recipes.rust-lite]
templates = []

[[recipes.rust-lite.files]]
root = "/tmp"
include = ["README.md", ".github/workflows/*.yml"]
"#,
        )
        .unwrap();

        let resolved = cfg.resolve_recipe("rust-lite").unwrap();
        assert!(resolved.templates.is_empty());
        assert_eq!(resolved.files.len(), 1);
        assert_eq!(resolved.files[0].include.len(), 2);
    }
}
