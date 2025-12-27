#![forbid(unsafe_code)]

//! Configuration loading and resolution for pinit.
//!
//! Supports TOML and YAML configuration files discovered via `~/.config/pinit.*`
//! or a user-provided override path.

use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::{debug, instrument};
use yaml_rust2::{Yaml, YamlLoader, yaml::Hash};

/// Parsed configuration file contents.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub base_template: Option<String>,

    pub license: Option<LicenseDef>,

    #[serde(default)]
    pub hooks: HookSet,

    #[serde(default)]
    pub sources: Vec<Source>,

    #[serde(default)]
    pub templates: BTreeMap<String, TemplateDef>,

    #[serde(default)]
    pub targets: BTreeMap<String, TargetDef>,

    #[serde(default)]
    pub overrides: Vec<OverrideRule>,

    #[serde(default)]
    pub recipes: BTreeMap<String, RecipeDef>,
}

/// License configuration for optional SPDX rendering.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum LicenseDef {
    Spdx(String),
    Detailed(LicenseDetailed),
}

impl LicenseDef {
    pub fn spdx(&self) -> &str {
        match self {
            LicenseDef::Spdx(s) => s.as_str(),
            LicenseDef::Detailed(d) => d.spdx.as_str(),
        }
    }

    pub fn output_path(&self) -> PathBuf {
        match self {
            LicenseDef::Spdx(_) => PathBuf::from("LICENSE"),
            LicenseDef::Detailed(d) => d.output.clone().unwrap_or_else(|| PathBuf::from("LICENSE")),
        }
    }

    pub fn template_args(&self) -> BTreeMap<String, String> {
        match self {
            LicenseDef::Spdx(_) => BTreeMap::new(),
            LicenseDef::Detailed(d) => {
                let mut args = d.args.clone();
                if let Some(year) = d.year.as_deref() {
                    args.entry("year".to_string())
                        .or_insert_with(|| year.to_string());
                }
                if let Some(name) = d.name.as_deref() {
                    args.entry("fullname".to_string())
                        .or_insert_with(|| name.to_string());
                    args.entry("copyright holders".to_string())
                        .or_insert_with(|| name.to_string());
                }
                args
            }
        }
    }
}

/// Detailed SPDX license configuration and template arguments.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct LicenseDetailed {
    /// SPDX license identifier, e.g. `MIT`, `Apache-2.0`.
    pub spdx: String,

    /// Destination path relative to the project root. Default: `LICENSE`.
    pub output: Option<PathBuf>,

    /// Convenience: fills the SPDX `year` template variable.
    pub year: Option<String>,

    /// Convenience: fills the SPDX `fullname` template variable.
    pub name: Option<String>,

    /// SPDX template variables by name, e.g. `copyright holders`.
    #[serde(default)]
    pub args: BTreeMap<String, String>,
}

/// Global or recipe-scoped hook configuration.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct HookSet {
    #[serde(default)]
    pub after_dir_create: Vec<HookDef>,

    #[serde(default)]
    pub after_recipe: Vec<HookDef>,

    #[serde(default)]
    pub after_all: Vec<HookDef>,
}

/// Hook command definition.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct HookDef {
    pub command: Vec<String>,
    pub run_on: Vec<HookRunOn>,

    pub cwd: Option<PathBuf>,

    #[serde(default)]
    pub env: BTreeMap<String, String>,

    #[serde(default)]
    pub allow_failure: bool,
}

/// When a hook should run.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HookRunOn {
    Init,
    Update,
}

/// Template source definition (local path or git repository).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Source {
    pub name: String,

    pub path: Option<PathBuf>,
    pub repo: Option<String>,

    #[serde(rename = "ref")]
    pub git_ref: Option<String>,

    pub git_protocol: Option<GitProtocol>,

    pub subdir: Option<PathBuf>,
}

/// Git transport protocol for shorthand repository identifiers.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GitProtocol {
    Ssh,
    Https,
}

impl GitProtocol {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "ssh" => Some(Self::Ssh),
            "https" => Some(Self::Https),
            _ => None,
        }
    }
}

/// Template definition that resolves to a directory.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum TemplateDef {
    Path(PathBuf),
    Detailed {
        source: Option<String>,
        path: PathBuf,
    },
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

/// Action to take when an override rule matches.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OverrideAction {
    #[default]
    Overwrite,
    Merge,
    Skip,
}

/// Override rule for a specific path or glob.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct OverrideRule {
    #[serde(alias = "path", alias = "pattern")]
    pub pattern: String,

    #[serde(default)]
    pub action: OverrideAction,
}

/// Target definition that can be a simple template list or a detailed object.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum TargetDef {
    Templates(Vec<String>),
    Detailed(TargetDetailed),
}

impl TargetDef {
    pub fn templates(&self) -> &[String] {
        match self {
            TargetDef::Templates(items) => items.as_slice(),
            TargetDef::Detailed(def) => def.templates.as_slice(),
        }
    }

    pub fn overrides(&self) -> &[OverrideRule] {
        match self {
            TargetDef::Templates(_) => &[],
            TargetDef::Detailed(def) => def.overrides.as_slice(),
        }
    }
}

/// Detailed target definition with template list and overrides.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct TargetDetailed {
    #[serde(default)]
    pub templates: Vec<String>,

    #[serde(default)]
    pub overrides: Vec<OverrideRule>,
}

/// Recipe definition made of template names and/or file sets.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct RecipeDef {
    #[serde(default)]
    pub templates: Vec<String>,

    #[serde(default)]
    pub files: Vec<FileSetDef>,

    #[serde(default)]
    pub overrides: Vec<OverrideRule>,

    #[serde(default)]
    pub hooks: HookSet,
}

/// File set definition for inline recipes.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct FileSetDef {
    pub root: PathBuf,

    #[serde(default)]
    pub include: Vec<String>,

    pub dest_prefix: Option<PathBuf>,
}

/// Recipe resolved to concrete template names and file sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedRecipe {
    pub name: String,
    pub templates: Vec<String>,
    pub files: Vec<FileSetDef>,
    pub overrides: Vec<OverrideRule>,
    pub hooks: HookSet,
    pub kind: ResolvedKind,
}

/// What kind of config entry resolved to a template stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedKind {
    Recipe,
    Target,
    Template,
}

/// Errors encountered while loading configuration.
#[derive(Debug)]
pub enum ConfigError {
    NotFound,
    Io {
        path: PathBuf,
        source: io::Error,
    },
    ParseToml {
        path: PathBuf,
        source: toml::de::Error,
    },
    ParseYaml {
        path: PathBuf,
        message: String,
    },
    YamlRootNotMapping {
        path: PathBuf,
    },
    InvalidConfig {
        path: PathBuf,
        message: String,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::NotFound => write!(f, "no config file found"),
            ConfigError::Io { path, source } => write!(f, "{}: {}", path.display(), source),
            ConfigError::ParseToml { path, source } => write!(f, "{}: {}", path.display(), source),
            ConfigError::ParseYaml { path, message } => {
                write!(f, "{}: {}", path.display(), message)
            }
            ConfigError::YamlRootNotMapping { path } => {
                write!(f, "{}: YAML root must be a mapping", path.display())
            }
            ConfigError::InvalidConfig { path, message } => {
                write!(f, "{}: {}", path.display(), message)
            }
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

/// Default configuration search paths in priority order.
pub fn default_config_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        let xdg = PathBuf::from(xdg);
        let root = xdg.join("pinit");
        out.push(root.join("pinit.toml"));
        out.push(root.join("pinit.yaml"));
        out.push(root.join("pinit.yml"));
        return out;
    }

    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        let config = home.join(".config").join("pinit");
        out.push(config.join("pinit.toml"));
        out.push(config.join("pinit.yaml"));
        out.push(config.join("pinit.yml"));
    }

    out
}

/// Load configuration from disk, optionally overriding the discovery path.
pub fn load_config(path_override: Option<&Path>) -> Result<(PathBuf, Config), ConfigError> {
    if let Some(path) = path_override {
        debug!(path = %path.display(), "config: load override");
        return load_config_at(path);
    }

    for path in default_config_paths() {
        if path.is_file() {
            debug!(path = %path.display(), "config: load");
            return load_config_at(&path);
        }
    }

    Err(ConfigError::NotFound)
}

#[instrument(skip_all, fields(path = %path.display()))]
fn load_config_at(path: &Path) -> Result<(PathBuf, Config), ConfigError> {
    let content = fs::read_to_string(path).map_err(|e| ConfigError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

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
    validate_config(path, &config)?;
    Ok((path.to_path_buf(), config))
}

fn parse_toml(path: &Path, s: &str) -> Result<Config, ConfigError> {
    toml::from_str::<Config>(s).map_err(|e| ConfigError::ParseToml {
        path: path.to_path_buf(),
        source: e,
    })
}

fn parse_yaml(path: &Path, s: &str) -> Result<Config, ConfigError> {
    // yaml-rust2 is intentionally used instead of serde_yaml (deprecated).
    //
    // This is a minimal parser that supports the subset of YAML we need for config.
    let docs = YamlLoader::load_from_str(s).map_err(|e| ConfigError::ParseYaml {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    let Some(doc) = docs.first() else {
        return Err(ConfigError::ParseYaml {
            path: path.to_path_buf(),
            message: "empty YAML document".to_string(),
        });
    };

    yaml_to_config(path, doc)
}

fn yaml_to_config(path: &Path, root: &Yaml) -> Result<Config, ConfigError> {
    let Yaml::Hash(map) = root else {
        return Err(ConfigError::YamlRootNotMapping {
            path: path.to_path_buf(),
        });
    };

    let mut cfg = Config {
        base_template: yaml_get_string(map, "base_template"),
        license: yaml_get(map, "license").and_then(yaml_to_license),
        ..Config::default()
    };

    if let Some(sources) = yaml_get_seq(map, "sources") {
        for source in sources {
            let Some(source_map) = yaml_as_mapping(source) else {
                continue;
            };
            let Some(name) = yaml_get_string(source_map, "name") else {
                continue;
            };
            let path_val = yaml_get_string(source_map, "path").map(PathBuf::from);
            let repo = yaml_get_string(source_map, "repo");
            let git_ref = yaml_get_string(source_map, "ref");
            let git_protocol =
                yaml_get_string(source_map, "git_protocol").and_then(|s| GitProtocol::parse(&s));
            let subdir = yaml_get_string(source_map, "subdir").map(PathBuf::from);
            cfg.sources.push(Source {
                name,
                path: path_val,
                repo,
                git_ref,
                git_protocol,
                subdir,
            });
        }
    }

    if let Some(templates_root) = yaml_get(map, "templates").and_then(yaml_as_mapping) {
        for (k, v) in templates_root {
            let Some(name) = yaml_as_string(k) else {
                continue;
            };

            if let Some(path_str) = yaml_as_string(v) {
                cfg.templates
                    .insert(name, TemplateDef::Path(PathBuf::from(path_str)));
                continue;
            }

            if let Some(d) = yaml_as_mapping(v) {
                let source = yaml_get_string(d, "source");
                let Some(path_str) = yaml_get_string(d, "path") else {
                    continue;
                };
                cfg.templates.insert(
                    name,
                    TemplateDef::Detailed {
                        source,
                        path: PathBuf::from(path_str),
                    },
                );
            }
        }
    }

    if let Some(targets_root) = yaml_get(map, "targets").and_then(yaml_as_mapping) {
        for (k, v) in targets_root {
            let Some(name) = yaml_as_string(k) else {
                continue;
            };
            if let Some(items) = yaml_as_vec_of_strings(v) {
                cfg.targets.insert(name, TargetDef::Templates(items));
                continue;
            }
            let Some(detail_map) = yaml_as_mapping(v) else {
                continue;
            };
            let templates = yaml_get_vec_of_strings(detail_map, "templates").unwrap_or_default();
            let overrides = yaml_get(detail_map, "overrides")
                .and_then(yaml_to_override_rules)
                .unwrap_or_default();
            cfg.targets.insert(
                name,
                TargetDef::Detailed(TargetDetailed {
                    templates,
                    overrides,
                }),
            );
        }
    }

    if let Some(overrides) = yaml_get(map, "overrides").and_then(yaml_to_override_rules) {
        cfg.overrides = overrides;
    }

    if let Some(hooks_root) = yaml_get(map, "hooks").and_then(yaml_as_mapping) {
        cfg.hooks = yaml_to_hook_set(path, hooks_root)?;
    }

    if let Some(recipes_root) = yaml_get(map, "recipes").and_then(yaml_as_mapping) {
        for (k, v) in recipes_root {
            let Some(name) = yaml_as_string(k) else {
                continue;
            };
            let Some(recipe_map) = yaml_as_mapping(v) else {
                continue;
            };

            let templates = yaml_get_vec_of_strings(recipe_map, "templates").unwrap_or_default();
            let overrides = yaml_get(recipe_map, "overrides")
                .and_then(yaml_to_override_rules)
                .unwrap_or_default();
            let hooks = match yaml_get(recipe_map, "hooks").and_then(yaml_as_mapping) {
                Some(hooks_map) => yaml_to_hook_set(path, hooks_map)?,
                None => HookSet::default(),
            };
            let mut files = Vec::new();
            if let Some(files_seq) = yaml_get_seq(recipe_map, "files") {
                for fs_item in files_seq {
                    let Some(fs_map) = yaml_as_mapping(fs_item) else {
                        continue;
                    };
                    let Some(root) = yaml_get_string(fs_map, "root").map(PathBuf::from) else {
                        continue;
                    };
                    let include = yaml_get_vec_of_strings(fs_map, "include").unwrap_or_default();
                    let dest_prefix = yaml_get_string(fs_map, "dest_prefix").map(PathBuf::from);
                    files.push(FileSetDef {
                        root,
                        include,
                        dest_prefix,
                    });
                }
            }

            cfg.recipes.insert(
                name,
                RecipeDef {
                    templates,
                    files,
                    overrides,
                    hooks,
                },
            );
        }
    }

    // Validate that the YAML did not contain multiple documents, anchors, etc is out-of-scope for v1.
    let _ = path;
    Ok(cfg)
}

fn yaml_key(s: &str) -> Yaml {
    Yaml::String(s.to_string())
}

fn yaml_get<'a>(map: &'a Hash, key: &str) -> Option<&'a Yaml> {
    map.get(&yaml_key(key))
}

fn yaml_get_seq<'a>(map: &'a Hash, key: &str) -> Option<&'a Vec<Yaml>> {
    yaml_get(map, key).and_then(|v| match v {
        Yaml::Array(seq) => Some(seq),
        _ => None,
    })
}

fn yaml_get_string(map: &Hash, key: &str) -> Option<String> {
    yaml_get(map, key).and_then(yaml_as_string)
}

fn yaml_get_vec_of_strings(map: &Hash, key: &str) -> Option<Vec<String>> {
    yaml_get(map, key).and_then(yaml_as_vec_of_strings)
}

fn yaml_as_mapping(y: &Yaml) -> Option<&Hash> {
    match y {
        Yaml::Hash(m) => Some(m),
        _ => None,
    }
}

fn yaml_as_string(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) => Some(s.to_string()),
        Yaml::Integer(i) => Some(i.to_string()),
        Yaml::Real(f) => Some(f.to_string()),
        Yaml::Boolean(b) => Some(if *b { "true" } else { "false" }.to_string()),
        _ => None,
    }
}

fn yaml_as_vec_of_strings(y: &Yaml) -> Option<Vec<String>> {
    match y {
        Yaml::Array(seq) => {
            let mut out = Vec::new();
            for item in seq {
                let s = yaml_as_string(item)?;
                out.push(s);
            }
            Some(out)
        }
        _ => None,
    }
}

fn yaml_as_bool(y: &Yaml) -> Option<bool> {
    match y {
        Yaml::Boolean(b) => Some(*b),
        _ => None,
    }
}

fn yaml_to_override_rules(y: &Yaml) -> Option<Vec<OverrideRule>> {
    let Yaml::Array(seq) = y else {
        return None;
    };
    let mut out = Vec::new();
    for item in seq {
        let Some(map) = yaml_as_mapping(item) else {
            continue;
        };
        let pattern = yaml_get_string(map, "path").or_else(|| yaml_get_string(map, "pattern"));
        let Some(pattern) = pattern else {
            continue;
        };
        let action = match yaml_get_string(map, "action")
            .as_deref()
            .map(|s| s.to_ascii_lowercase())
            .as_deref()
        {
            None => OverrideAction::Overwrite,
            Some("overwrite") => OverrideAction::Overwrite,
            Some("merge") => OverrideAction::Merge,
            Some("skip") => OverrideAction::Skip,
            Some(_) => continue,
        };
        out.push(OverrideRule { pattern, action });
    }
    Some(out)
}

fn yaml_to_hook_set(path: &Path, map: &Hash) -> Result<HookSet, ConfigError> {
    let after_dir_create = match yaml_get(map, "after_dir_create") {
        Some(v) => yaml_to_hooks(path, "hooks.after_dir_create", v)?,
        None => Vec::new(),
    };
    let after_recipe = match yaml_get(map, "after_recipe") {
        Some(v) => yaml_to_hooks(path, "hooks.after_recipe", v)?,
        None => Vec::new(),
    };
    let after_all = match yaml_get(map, "after_all") {
        Some(v) => yaml_to_hooks(path, "hooks.after_all", v)?,
        None => Vec::new(),
    };

    Ok(HookSet {
        after_dir_create,
        after_recipe,
        after_all,
    })
}

fn yaml_to_hooks(path: &Path, label: &str, y: &Yaml) -> Result<Vec<HookDef>, ConfigError> {
    let Yaml::Array(seq) = y else {
        return Err(ConfigError::InvalidConfig {
            path: path.to_path_buf(),
            message: format!("{label} must be a list"),
        });
    };

    let mut out = Vec::new();
    for (idx, item) in seq.iter().enumerate() {
        let Some(map) = yaml_as_mapping(item) else {
            return Err(ConfigError::InvalidConfig {
                path: path.to_path_buf(),
                message: format!("{label}[{idx}] must be a mapping"),
            });
        };

        let command = yaml_get(map, "command")
            .and_then(yaml_as_vec_of_strings)
            .ok_or_else(|| ConfigError::InvalidConfig {
                path: path.to_path_buf(),
                message: format!("{label}[{idx}].command must be a non-empty list"),
            })?;
        if command.is_empty() {
            return Err(ConfigError::InvalidConfig {
                path: path.to_path_buf(),
                message: format!("{label}[{idx}].command must be a non-empty list"),
            });
        }

        let run_on_strings = yaml_get(map, "run_on")
            .and_then(yaml_as_vec_of_strings)
            .ok_or_else(|| ConfigError::InvalidConfig {
                path: path.to_path_buf(),
                message: format!("{label}[{idx}].run_on must be a non-empty list"),
            })?;
        if run_on_strings.is_empty() {
            return Err(ConfigError::InvalidConfig {
                path: path.to_path_buf(),
                message: format!("{label}[{idx}].run_on must be a non-empty list"),
            });
        }
        let mut run_on = Vec::new();
        for item in run_on_strings {
            let parsed = match item.to_ascii_lowercase().as_str() {
                "init" => HookRunOn::Init,
                "update" => HookRunOn::Update,
                other => {
                    return Err(ConfigError::InvalidConfig {
                        path: path.to_path_buf(),
                        message: format!("{label}[{idx}].run_on has invalid value '{other}'"),
                    });
                }
            };
            run_on.push(parsed);
        }

        let cwd = yaml_get_string(map, "cwd").map(PathBuf::from);
        let allow_failure = match yaml_get(map, "allow_failure") {
            Some(v) => yaml_as_bool(v).ok_or_else(|| ConfigError::InvalidConfig {
                path: path.to_path_buf(),
                message: format!("{label}[{idx}].allow_failure must be a boolean"),
            })?,
            None => false,
        };

        let mut env = BTreeMap::new();
        if let Some(env_map) = yaml_get(map, "env").and_then(yaml_as_mapping) {
            for (k, v) in env_map {
                let Some(key) = yaml_as_string(k) else {
                    continue;
                };
                let Some(val) = yaml_as_string(v) else {
                    continue;
                };
                env.insert(key, val);
            }
        }

        out.push(HookDef {
            command,
            run_on,
            cwd,
            env,
            allow_failure,
        });
    }

    Ok(out)
}

fn yaml_to_license(y: &Yaml) -> Option<LicenseDef> {
    if let Some(s) = yaml_as_string(y) {
        return Some(LicenseDef::Spdx(s));
    }

    let map = yaml_as_mapping(y)?;
    let spdx = yaml_get_string(map, "spdx")
        .or_else(|| yaml_get_string(map, "id"))
        .or_else(|| yaml_get_string(map, "license"))?;

    let output = yaml_get_string(map, "output")
        .or_else(|| yaml_get_string(map, "path"))
        .map(PathBuf::from);
    let year = yaml_get_string(map, "year");
    let name = yaml_get_string(map, "name");

    let mut args = BTreeMap::new();
    if let Some(args_map) = yaml_get(map, "args").and_then(yaml_as_mapping) {
        for (k, v) in args_map {
            let Some(key) = yaml_as_string(k) else {
                continue;
            };
            let Some(val) = yaml_as_string(v) else {
                continue;
            };
            args.insert(key, val);
        }
    }

    Some(LicenseDef::Detailed(LicenseDetailed {
        spdx,
        output,
        year,
        name,
        args,
    }))
}

fn validate_config(path: &Path, cfg: &Config) -> Result<(), ConfigError> {
    validate_hook_set(path, "hooks", &cfg.hooks)?;
    for (name, recipe) in &cfg.recipes {
        let label = format!("recipes.{name}.hooks");
        validate_hook_set(path, &label, &recipe.hooks)?;
    }
    Ok(())
}

fn validate_hook_set(path: &Path, label: &str, hooks: &HookSet) -> Result<(), ConfigError> {
    validate_hooks_list(
        path,
        &format!("{label}.after_dir_create"),
        &hooks.after_dir_create,
    )?;
    validate_hooks_list(path, &format!("{label}.after_recipe"), &hooks.after_recipe)?;
    validate_hooks_list(path, &format!("{label}.after_all"), &hooks.after_all)?;
    Ok(())
}

fn validate_hooks_list(path: &Path, label: &str, hooks: &[HookDef]) -> Result<(), ConfigError> {
    for (idx, hook) in hooks.iter().enumerate() {
        if hook.command.is_empty() {
            return Err(ConfigError::InvalidConfig {
                path: path.to_path_buf(),
                message: format!("{label}[{idx}].command must be a non-empty list"),
            });
        }
        if hook.run_on.is_empty() {
            return Err(ConfigError::InvalidConfig {
                path: path.to_path_buf(),
                message: format!("{label}[{idx}].run_on must be a non-empty list"),
            });
        }
    }
    Ok(())
}

impl Config {
    /// Resolve a recipe/target/template name into concrete templates and file sets.
    pub fn resolve_recipe(&self, name: &str) -> Option<ResolvedRecipe> {
        if let Some(def) = self.recipes.get(name) {
            let mut overrides = self.overrides.clone();
            overrides.extend(def.overrides.clone());
            return Some(ResolvedRecipe {
                name: name.to_string(),
                templates: def.templates.clone(),
                files: def.files.clone(),
                overrides,
                hooks: def.hooks.clone(),
                kind: ResolvedKind::Recipe,
            });
        }

        if let Some(stack) = self.targets.get(name) {
            let mut overrides = self.overrides.clone();
            overrides.extend(stack.overrides().iter().cloned());
            return Some(ResolvedRecipe {
                name: name.to_string(),
                templates: stack.templates().to_vec(),
                files: Vec::new(),
                overrides,
                hooks: HookSet::default(),
                kind: ResolvedKind::Target,
            });
        }

        if self.templates.contains_key(name) {
            let mut templates = Vec::new();
            if let Some(base_template) = self.base_template.as_deref()
                && base_template != name
            {
                templates.push(base_template.to_string());
            }
            templates.push(name.to_string());
            let overrides = self.overrides.clone();
            return Some(ResolvedRecipe {
                name: name.to_string(),
                templates,
                files: Vec::new(),
                overrides,
                hooks: HookSet::default(),
                kind: ResolvedKind::Template,
            });
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
base_template = "common"

[templates]
common = "common"
rust = "rust"

[targets]
rust = ["common", "rust"]
"#,
        )
        .unwrap();

        let resolved = cfg.resolve_recipe("rust").unwrap();
        assert_eq!(
            resolved.templates,
            vec!["common".to_string(), "rust".to_string()]
        );
        assert!(resolved.overrides.is_empty());
    }

    #[test]
    fn parses_yaml_and_resolves_target() {
        let yaml = r#"
base_template: common
license: MIT
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
        assert_eq!(
            resolved.templates,
            vec!["common".to_string(), "rust".to_string()]
        );
        assert_eq!(cfg.license.as_ref().unwrap().spdx(), "MIT");
        assert!(resolved.overrides.is_empty());

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

    #[test]
    fn parses_toml_hooks() {
        let cfg: Config = toml::from_str(
            r#"
[hooks]

[[hooks.after_all]]
command = ["echo", "done"]
run_on = ["update"]

[recipes.rust]
templates = ["rust"]

[[recipes.rust.hooks.after_recipe]]
command = ["cargo", "fmt"]
run_on = ["init"]
"#,
        )
        .unwrap();

        assert_eq!(cfg.hooks.after_all.len(), 1);
        assert_eq!(cfg.hooks.after_all[0].command, vec!["echo", "done"]);
        assert_eq!(cfg.recipes["rust"].hooks.after_recipe.len(), 1);
    }

    #[test]
    fn parses_yaml_hooks() {
        let yaml = r#"
templates:
  rust: rust
hooks:
  after_all:
    - command: [echo, done]
      run_on: [update]
recipes:
  rust:
    templates: [rust]
    hooks:
      after_recipe:
        - command: [cargo, fmt]
          run_on: [init]
"#;

        let root = make_temp_root();
        let path = root.join("pinit.yaml");
        fs::write(&path, yaml).unwrap();

        let (_, cfg) = load_config(Some(&path)).unwrap();
        assert_eq!(cfg.hooks.after_all.len(), 1);
        assert_eq!(cfg.recipes["rust"].hooks.after_recipe.len(), 1);
    }

    #[test]
    fn invalid_hook_run_on_errors() {
        let yaml = r#"
hooks:
  after_all:
    - command: [echo, done]
      run_on: [sometimes]
"#;

        let root = make_temp_root();
        let path = root.join("pinit.yaml");
        fs::write(&path, yaml).unwrap();

        let err = load_config(Some(&path)).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidConfig { .. }));
    }

    #[test]
    fn empty_hook_command_errors() {
        let root = make_temp_root();
        let path = root.join("pinit.toml");
        fs::write(
            &path,
            r#"
[[hooks.after_all]]
command = []
run_on = ["init"]
"#,
        )
        .unwrap();

        let err = load_config(Some(&path)).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidConfig { .. }));
    }

    #[test]
    fn parses_toml_license_detailed_with_defaults() {
        let cfg: Config = toml::from_str(
            r#"
[license]
spdx = "MIT"
year = "2025"
name = "Clay"

[templates]
rust = "/tmp"
"#,
        )
        .unwrap();

        let lic = cfg.license.unwrap();
        assert_eq!(lic.spdx(), "MIT");
        assert_eq!(lic.output_path(), PathBuf::from("LICENSE"));
        let args = lic.template_args();
        assert_eq!(args.get("year").unwrap(), "2025");
        assert_eq!(args.get("fullname").unwrap(), "Clay");
        assert_eq!(args.get("copyright holders").unwrap(), "Clay");
    }

    #[test]
    fn parses_yaml_license_detailed() {
        let yaml = r#"
license:
  spdx: MIT
  year: "2025"
  name: Clay
  output: LICENSES/MIT.txt
  args:
    files: "this software"
templates:
  rust: rust
"#;

        let root = make_temp_root();
        let path = root.join("pinit.yaml");
        fs::write(&path, yaml).unwrap();

        let (_, cfg) = load_config(Some(&path)).unwrap();
        let lic = cfg.license.unwrap();
        assert_eq!(lic.spdx(), "MIT");
        assert_eq!(lic.output_path(), PathBuf::from("LICENSES/MIT.txt"));
        let args = lic.template_args();
        assert_eq!(args.get("year").unwrap(), "2025");
        assert_eq!(args.get("fullname").unwrap(), "Clay");
        assert_eq!(args.get("files").unwrap(), "this software");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_config_invalid_toml_errors() {
        let root = make_temp_root();
        let path = root.join("pinit.toml");
        fs::write(&path, "this is not toml = ").unwrap();
        let err = load_config(Some(&path)).unwrap_err();
        assert!(matches!(err, ConfigError::ParseToml { .. }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn load_config_yaml_root_not_mapping_errors() {
        let root = make_temp_root();
        let path = root.join("pinit.yaml");
        fs::write(&path, "- a\n- b\n").unwrap();
        let err = load_config(Some(&path)).unwrap_err();
        assert!(matches!(err, ConfigError::YamlRootNotMapping { .. }));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn yaml_value_string_coercions() {
        assert_eq!(
            yaml_as_string(&Yaml::Boolean(true)).as_deref(),
            Some("true")
        );
        assert_eq!(
            yaml_as_string(&Yaml::Boolean(false)).as_deref(),
            Some("false")
        );
        assert_eq!(yaml_as_string(&Yaml::Integer(42)).as_deref(), Some("42"));
        assert!(
            yaml_as_string(&Yaml::Real(std::f64::consts::PI.to_string()))
                .unwrap()
                .starts_with("3.14")
        );

        let seq = Yaml::Array(vec![Yaml::Integer(1), Yaml::Boolean(false)]);
        assert_eq!(
            yaml_as_vec_of_strings(&seq).unwrap(),
            vec!["1".to_string(), "false".to_string()]
        );

        let bad = Yaml::Array(vec![Yaml::Hash(Hash::new())]);
        assert!(yaml_as_vec_of_strings(&bad).is_none());
    }

    #[test]
    fn yaml_to_config_handles_sources_templates_targets_and_recipes() {
        let mut root = Hash::new();
        root.insert(
            yaml_key("base_template"),
            Yaml::String("common".to_string()),
        );

        let mut lic_map = Hash::new();
        lic_map.insert(yaml_key("id"), Yaml::String("MIT".to_string()));
        lic_map.insert(
            yaml_key("path"),
            Yaml::String("LICENSES/MIT.txt".to_string()),
        );
        root.insert(yaml_key("license"), Yaml::Hash(lic_map));

        let mut src_ok = Hash::new();
        src_ok.insert(yaml_key("name"), Yaml::String("local".to_string()));
        src_ok.insert(yaml_key("path"), Yaml::String("/tmp/templates".to_string()));
        src_ok.insert(yaml_key("git_protocol"), Yaml::String("https".to_string()));
        let src_missing_name = Yaml::Hash(Hash::new());
        root.insert(
            yaml_key("sources"),
            Yaml::Array(vec![
                Yaml::Hash(src_ok),
                Yaml::String("not-a-map".to_string()),
                src_missing_name,
            ]),
        );

        let mut templates = Hash::new();
        templates.insert(yaml_key("rust"), Yaml::String("rust".to_string()));
        let mut detailed = Hash::new();
        detailed.insert(yaml_key("source"), Yaml::String("local".to_string()));
        detailed.insert(yaml_key("path"), Yaml::String("common".to_string()));
        templates.insert(yaml_key("common"), Yaml::Hash(detailed));
        templates.insert(yaml_key("bad"), Yaml::Hash(Hash::new()));
        root.insert(yaml_key("templates"), Yaml::Hash(templates));

        let mut targets = Hash::new();
        targets.insert(
            yaml_key("rust"),
            Yaml::Array(vec![
                Yaml::String("common".to_string()),
                Yaml::String("rust".to_string()),
            ]),
        );
        targets.insert(yaml_key("bad"), Yaml::String("no".to_string()));
        root.insert(yaml_key("targets"), Yaml::Hash(targets));

        let mut recipes = Hash::new();
        let mut recipe_map = Hash::new();
        recipe_map.insert(
            yaml_key("templates"),
            Yaml::Array(vec![Yaml::String("rust".to_string())]),
        );
        let mut fs_ok = Hash::new();
        fs_ok.insert(yaml_key("root"), Yaml::String("/tmp".to_string()));
        fs_ok.insert(
            yaml_key("include"),
            Yaml::Array(vec![Yaml::String("README.md".to_string())]),
        );
        recipe_map.insert(
            yaml_key("files"),
            Yaml::Array(vec![Yaml::Hash(fs_ok), Yaml::String("bad".to_string())]),
        );
        recipes.insert(yaml_key("r1"), Yaml::Hash(recipe_map));
        recipes.insert(yaml_key("bad"), Yaml::String("no".to_string()));
        root.insert(yaml_key("recipes"), Yaml::Hash(recipes));

        let cfg = yaml_to_config(Path::new("x"), &Yaml::Hash(root)).unwrap();
        assert_eq!(cfg.base_template.as_deref(), Some("common"));
        assert_eq!(cfg.license.as_ref().unwrap().spdx(), "MIT");
        assert_eq!(
            cfg.license
                .as_ref()
                .unwrap()
                .output_path()
                .to_string_lossy(),
            "LICENSES/MIT.txt"
        );
        assert_eq!(cfg.sources.len(), 1);
        assert_eq!(cfg.sources[0].git_protocol, Some(GitProtocol::Https));
        assert!(cfg.templates.contains_key("rust"));
        assert!(cfg.templates.contains_key("common"));
        assert!(!cfg.templates.contains_key("bad"));
        assert_eq!(
            cfg.targets.get("rust").unwrap().templates(),
            &["common".to_string(), "rust".to_string()]
        );
        assert!(!cfg.targets.contains_key("bad"));
        assert!(cfg.recipes.contains_key("r1"));
        assert!(!cfg.recipes.contains_key("bad"));
        assert_eq!(cfg.recipes["r1"].files.len(), 1);
    }
}
