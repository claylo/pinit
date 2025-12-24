use std::fs;

#[test]
fn toml_license_string_form_is_supported() {
    let cfg: pinit_core::config::Config = toml::from_str(
        r#"
license = "MIT"

[templates]
rust = "/tmp/rust"
"#,
    )
    .unwrap();

    assert_eq!(cfg.license.unwrap().spdx(), "MIT");
}

#[test]
fn yaml_license_alias_keys_are_supported() {
    let root = std::env::temp_dir().join(format!("pinit-config-shapes-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let path = root.join("pinit.yaml");
    fs::write(
        &path,
        r#"
license:
  id: MIT
  path: LICENSES/MIT.txt
  year: 2025
  name: Clay
templates:
  rust: rust
"#,
    )
    .unwrap();

    let (_, cfg) = pinit_core::config::load_config(Some(&path)).unwrap();
    let lic = cfg.license.unwrap();
    assert_eq!(lic.spdx(), "MIT");
    assert_eq!(lic.output_path().to_string_lossy(), "LICENSES/MIT.txt");
    let args = lic.template_args();
    assert_eq!(args.get("year").unwrap(), "2025");
    assert_eq!(args.get("fullname").unwrap(), "Clay");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn load_config_unknown_extension_falls_back_to_toml_then_yaml() {
    let root = std::env::temp_dir().join(format!("pinit-config-shapes-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let path = root.join("pinit.conf");
    fs::write(
        &path,
        r#"
base_template = "common"

[templates]
common = "/tmp/common"
"#,
    )
    .unwrap();

    let (_, cfg) = pinit_core::config::load_config(Some(&path)).unwrap();
    assert_eq!(cfg.base_template.as_deref(), Some("common"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn load_config_missing_path_returns_io_error() {
    let path = std::env::temp_dir().join(format!(
        "pinit-config-missing-{}-nope.toml",
        std::process::id()
    ));
    let err = pinit_core::config::load_config(Some(&path)).unwrap_err();
    assert!(matches!(err, pinit_core::config::ConfigError::Io { .. }));
}
