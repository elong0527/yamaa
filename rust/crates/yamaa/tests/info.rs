//! `info()` (D41, P-INFO-1): the six fields every host reports.

use yamaa::{info, Host};

#[test]
fn the_engine_reports_its_version_language_versions_and_platform() {
    let info = info(Host::Cli, None);
    assert_eq!(info.version, yamaa::VERSION);
    // A 1.0 engine reads the 1.0 language, oldest first (SCHEMA-2).
    assert_eq!(info.language_versions, ["1.0"]);
    assert_eq!(
        info.platform,
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
    );
}

/// The four platforms of #3030 D49 read as P-INFO-1 spells them, with no
/// target triple and no C library.
#[test]
fn a_platform_is_its_operating_system_and_architecture() {
    let platform = info(Host::Cli, None).platform;
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        assert_eq!(platform, "linux-x86_64");
    }
    if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        assert_eq!(platform, "linux-aarch64");
    }
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        assert_eq!(platform, "windows-x86_64");
    }
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert_eq!(platform, "macos-aarch64");
    }
}

#[test]
fn features_list_the_compiled_optional_features_sorted() {
    let features = info(Host::Cli, None).features;
    for (name, compiled) in [
        ("odm", cfg!(feature = "odm")),
        ("parquet", cfg!(feature = "parquet")),
        ("readstat", cfg!(feature = "readstat")),
        ("yaml", cfg!(feature = "yaml")),
    ] {
        assert_eq!(features.contains(&name), compiled, "{name}");
    }
    assert!(features.windows(2).all(|pair| pair[0] < pair[1]));
    // Every shipped package and binary compiles all four.
    if cfg!(all(
        feature = "odm",
        feature = "parquet",
        feature = "readstat",
        feature = "yaml"
    )) {
        assert_eq!(features, ["odm", "parquet", "readstat", "yaml"]);
    }
}

#[test]
fn the_host_names_itself_and_its_own_version() {
    let python = info(Host::Python, Some("3.12.4"));
    assert_eq!(python.host, Host::Python);
    assert_eq!(python.host_version.as_deref(), Some("3.12.4"));
    let r = info(Host::R, Some("4.6.1"));
    assert_eq!(r.host_version.as_deref(), Some("4.6.1"));
    // The command line runs on no interpreter, so it has no host version.
    assert_eq!(info(Host::Cli, None).host_version, None);
    assert_eq!(
        [Host::Python, Host::R, Host::Cli].map(Host::as_str),
        ["python", "r", "cli"]
    );
    // Only the host's own facts differ between hosts.
    let mut same = python;
    same.host = Host::Cli;
    same.host_version = None;
    assert_eq!(same, info(Host::Cli, None));
}
