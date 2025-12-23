use std::path::PathBuf;

#[test]
fn covers_display_and_small_helpers() {
    assert_eq!(
        pinit_core::ExistingFileAction::Overwrite.as_str(),
        "overwrite"
    );
    assert_eq!(pinit_core::ExistingFileAction::Merge.as_str(), "merge");
    assert_eq!(pinit_core::ExistingFileAction::Skip.as_str(), "skip");

    let e = pinit_core::ApplyError::TemplateDirNotFound(PathBuf::from("x"));
    assert!(e.to_string().contains("template directory not found"));

    let e = pinit_core::ApplyError::TemplateDirNotDir(PathBuf::from("x"));
    assert!(e.to_string().contains("template path is not a directory"));

    let e = pinit_core::ApplyError::DestDirNotDir(PathBuf::from("x"));
    assert!(e.to_string().contains("destination is not a directory"));

    let e = pinit_core::ApplyError::SymlinkNotSupported(PathBuf::from("x"));
    assert!(e.to_string().contains("symlinks are not supported"));

    let e = pinit_core::ApplyError::GitIgnoreFailed {
        cmd: "git".into(),
        status: 128,
        stderr: "no".into(),
    };
    assert!(e.to_string().contains("git ignore check failed"));

    let io = std::io::Error::new(std::io::ErrorKind::Other, "boom");
    let e = pinit_core::ApplyError::Io {
        path: PathBuf::from("x"),
        source: io,
    };
    assert!(std::error::Error::source(&e).is_some());
    assert!(e.to_string().contains("boom"));

    let e = pinit_core::config::ConfigError::NotFound;
    assert!(e.to_string().contains("no config file found"));

    let e = pinit_core::config::ConfigError::ParseYaml {
        path: PathBuf::from("x"),
        message: "bad".into(),
    };
    assert!(e.to_string().contains("bad"));

    let e = pinit_core::resolve::ResolveError::UnknownTemplate("nope".into());
    assert!(e.to_string().contains("unknown template"));

    let e = pinit_core::resolve::ResolveError::SourcePathMissing { source: "s".into() };
    assert!(e.to_string().contains("missing 'path'"));
}
