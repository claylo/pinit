use std::fs;
use std::sync::Mutex;

#[test]
fn load_config_not_found_when_xdg_empty() {
    static ENV_LOCK: Mutex<()> = Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap();

    let root = std::env::temp_dir().join(format!("pinit-config-env-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();

    let prev = std::env::var_os("XDG_CONFIG_HOME");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &root) };
    let res = pinit_core::config::load_config(None);
    match prev {
        Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
        None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
    }

    let _ = fs::remove_dir_all(&root);

    match res {
        Err(pinit_core::config::ConfigError::NotFound) => {}
        other => panic!("unexpected: {other:?}"),
    }
}

#[test]
fn load_config_finds_config_in_xdg_config_home() {
    static ENV_LOCK: Mutex<()> = Mutex::new(());
    let _guard = ENV_LOCK.lock().unwrap();

    let root = std::env::temp_dir().join(format!("pinit-config-env-found-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let cfg_dir = root.join("pinit");
    fs::create_dir_all(&cfg_dir).unwrap();
    let cfg_path = cfg_dir.join("pinit.toml");
    fs::write(&cfg_path, "[templates]\nrust = \"/tmp/rust\"\n").unwrap();

    let prev = std::env::var_os("XDG_CONFIG_HOME");
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &root) };
    let (found_path, cfg) = pinit_core::config::load_config(None).unwrap();
    match prev {
        Some(v) => unsafe { std::env::set_var("XDG_CONFIG_HOME", v) },
        None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
    }

    assert_eq!(found_path, cfg_path);
    assert!(cfg.templates.contains_key("rust"));

    let _ = fs::remove_dir_all(&root);
}
