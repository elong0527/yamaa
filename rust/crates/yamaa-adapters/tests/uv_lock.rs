//! uv lock verification over a recorded Python installation (FUNC-3). The
//! comparison moved from the Python host into the engine (#3030 D11); each
//! case keeps one of the host's former `verify_versions` cases, and the
//! observation trace keeps the order in which the host is asked.
//!
//! The seam is `verify_uv` against a scripted `PythonInstallation`, whose
//! trace and integer limit the test sets, which neither the benchmark reader
//! nor the host fixtures reach: a real host answers from its own interpreter.
use std::collections::BTreeMap;
use yamaa_adapters::project_lock::{
    verify_uv, Finding, Installed, PythonInstallation, Reason, UvFailure, VERSION_DIGITS,
};

/// An installation that records each observation it is asked for.
struct Host {
    providers: BTreeMap<String, Vec<String>>,
    versions: BTreeMap<String, Installed>,
    environment: BTreeMap<String, String>,
    failure: Option<&'static str>,
    /// The interpreter's integer limit, `None` when it is turned off.
    digits: Option<usize>,
    /// The limit that each observation, by its name, sets as it runs.
    sets: BTreeMap<&'static str, Option<usize>>,
    trace: Vec<String>,
}

impl Host {
    /// Apply the limit that an observation sets, as a Python callback that
    /// calls `sys.set_int_max_str_digits` would.
    fn observe(&mut self, observation: &str) {
        if let Some(digits) = self.sets.get(observation) {
            self.digits = *digits;
        }
    }

    fn new(versions: &[(&str, &str)], providers: &[(&str, &[&str])]) -> Self {
        Self {
            providers: providers
                .iter()
                .map(|(module, names)| {
                    (
                        (*module).to_owned(),
                        names.iter().map(|name| (*name).to_owned()).collect(),
                    )
                })
                .collect(),
            versions: versions
                .iter()
                .map(|(name, version)| {
                    (
                        (*name).to_owned(),
                        Installed::Version((*version).to_owned()),
                    )
                })
                .collect(),
            environment: environment("linux"),
            failure: None,
            digits: Some(VERSION_DIGITS),
            sets: BTreeMap::new(),
            trace: Vec::new(),
        }
    }
}

fn environment(platform: &str) -> BTreeMap<String, String> {
    [
        ("implementation_name", "cpython"),
        ("implementation_version", "3.12.8"),
        ("os_name", "posix"),
        ("platform_machine", "x86_64"),
        ("platform_release", "6.8.0"),
        ("platform_system", "Linux"),
        ("platform_version", "#1 SMP"),
        ("python_full_version", "3.12.8"),
        ("platform_python_implementation", "CPython"),
        ("python_version", "3.12"),
        ("sys_platform", platform),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_owned(), value.to_owned()))
    .collect()
}

impl PythonInstallation for Host {
    type Error = &'static str;

    fn providers(
        &mut self,
        modules: &[String],
    ) -> Result<BTreeMap<String, Vec<String>>, Self::Error> {
        self.trace.push("providers".to_owned());
        self.observe("providers");
        Ok(self
            .providers
            .iter()
            .filter(|(module, _)| modules.contains(module))
            .map(|(module, names)| (module.clone(), names.clone()))
            .collect())
    }

    fn standard_library(&mut self, module: &str) -> Result<bool, Self::Error> {
        Ok(["math", "statistics"].contains(&module))
    }

    fn version(&mut self, distribution: &str) -> Result<Installed, Self::Error> {
        self.trace.push(format!("version {distribution}"));
        self.observe("version");
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        Ok(self
            .versions
            .get(distribution)
            .cloned()
            .unwrap_or(Installed::Missing))
    }

    fn environment(&mut self) -> Result<BTreeMap<String, String>, Self::Error> {
        self.trace.push("environment".to_owned());
        self.observe("environment");
        Ok(self.environment.clone())
    }

    fn integer_digits(&mut self) -> Result<Option<usize>, Self::Error> {
        self.trace.push("integer digits".to_owned());
        Ok(self.digits)
    }
}

/// A uv lock of `(name, version, resolution-markers)` records.
fn lock(records: &[(&str, &str, Option<&[&str]>)]) -> Vec<u8> {
    let mut text = String::from("version = 1\n");
    for (name, version, markers) in records {
        text += &format!("\n[[package]]\nname = \"{name}\"\nversion = \"{version}\"\n");
        if let Some(markers) = markers {
            let markers = markers
                .iter()
                .map(|marker| format!("{marker:?}"))
                .collect::<Vec<_>>();
            text += &format!("resolution-markers = [{}]\n", markers.join(", "));
        }
    }
    text.into_bytes()
}

fn finding(package: &str, reason: Reason, expected: &[&str], actual: Option<&str>) -> Finding {
    Finding {
        package: package.to_owned(),
        reason,
        expected: expected
            .iter()
            .map(|version| (*version).to_owned())
            .collect(),
        actual: actual.map(str::to_owned),
    }
}

#[test]
fn empty_selection_has_no_parsing_or_metadata_effects() {
    let mut host = Host::new(&[], &[]);
    assert_eq!(verify_uv(b"[malformed", &[], &mut host), Ok(Vec::new()));
    assert!(host.trace.is_empty());
}

#[test]
fn stdlib_provider_mapping_precedes_exemption_and_yamaa_rechecks_each_activation() {
    let raw = lock(&[("yamaa", "0.2.0", None)]);
    let mut host = Host::new(&[("yamaa", "0.2.0")], &[]);
    for _ in 0..2 {
        assert_eq!(
            verify_uv(&raw, &["math.sqrt", "statistics.mean"], &mut host),
            Ok(Vec::new())
        );
    }
    assert_eq!(
        host.trace,
        ["providers", "version yamaa", "providers", "version yamaa"]
    );
}

#[test]
fn module_to_distribution_mapping_is_lossless_and_unrelated_packages_are_not_audited() {
    let raw = lock(&[
        ("yamaa", "0.2.0", None),
        ("project-bmi", "1.2.0", None),
        ("unused", "9", None),
    ]);
    let mut host = Host::new(
        &[("yamaa", "0.2.0"), ("project-bmi", "1.2")],
        &[("project_bmi", &["Project_BMI"])],
    );
    assert_eq!(
        verify_uv(
            &raw,
            &["project_bmi.calculate", "project_bmi.other"],
            &mut host
        ),
        Ok(Vec::new())
    );
    assert_eq!(
        host.trace,
        ["providers", "version project-bmi", "version yamaa"]
    );
}

#[test]
fn platform_forks_select_the_locked_version_independently_of_installed_version() {
    let raw = lock(&[
        ("yamaa", "0.2.0", None),
        ("project", "1", Some(&["sys_platform == 'linux'"])),
        ("project", "2", Some(&["sys_platform == 'win32'"])),
    ]);
    let mut host = Host::new(
        &[("yamaa", "0.2.0"), ("project", "2")],
        &[("project", &["project"])],
    );
    assert_eq!(
        verify_uv(&raw, &["project.run"], &mut host),
        Ok(vec![finding(
            "project",
            Reason::VersionMismatch,
            &["1"],
            Some("2")
        )])
    );
    // The marker environment is read once, when the first marker is.
    assert_eq!(
        host.trace,
        [
            "providers",
            "environment",
            "version project",
            "version yamaa"
        ]
    );
    host.environment = environment("win32");
    assert_eq!(verify_uv(&raw, &["project.run"], &mut host), Ok(Vec::new()));
}

#[test]
fn all_independent_package_failures_are_reported_and_namespace_providers_all_checked() {
    let raw = lock(&[("yamaa", "0.2.0", None), ("a", "1", None), ("b", "2", None)]);
    let mut host = Host::new(
        &[("yamaa", "0.1.0"), ("a", "3")],
        &[("shared", &["a", "b"])],
    );
    assert_eq!(
        verify_uv(&raw, &["shared.function", "unknown.call"], &mut host),
        Ok(vec![
            finding("unknown", Reason::DistributionNotIdentified, &[], None),
            finding("a", Reason::VersionMismatch, &["1"], Some("3")),
            finding("b", Reason::PackageNotInstalled, &["2"], None),
            finding("yamaa", Reason::VersionMismatch, &["0.2.0"], Some("0.1.0")),
        ])
    );
    assert_eq!(
        host.trace,
        ["providers", "version a", "version b", "version yamaa"]
    );
}

#[test]
fn ambiguous_forks_are_not_chosen_by_the_installed_version() {
    let raw = lock(&[
        ("yamaa", "0.2.0", None),
        ("project", "2", None),
        ("project", "1", None),
    ]);
    let mut host = Host::new(
        &[("yamaa", "0.2.0"), ("project", "2")],
        &[("project", &["project"])],
    );
    assert_eq!(
        verify_uv(&raw, &["project.run"], &mut host),
        Ok(vec![finding(
            "project",
            Reason::AmbiguousLockVersion,
            &["1", "2"],
            None
        )])
    );
    assert!(!host.trace.contains(&"version project".to_owned()));
}

#[test]
fn invalid_lock_syntax_and_shape_precede_metadata() {
    for raw in [
        &b"\xff"[..],
        b"version = true\npackage = []",
        b"version = 2\npackage = []",
        b"version = 1\npackage = {}",
        b"version = 1\n[[package]\n",
    ] {
        let mut host = Host::new(&[], &[]);
        assert!(
            matches!(
                verify_uv(raw, &["project.run"], &mut host),
                Err(UvFailure::Lock(_))
            ),
            "{raw:?}"
        );
        assert!(host.trace.is_empty());
    }
}

#[test]
fn deeply_nested_lock_syntax_is_an_invalid_lock() {
    let raw = format!(
        "version = 1\nx = {}{}\npackage = []\n",
        "[".repeat(2000),
        "]".repeat(2000)
    );
    let mut host = Host::new(&[], &[]);
    assert_eq!(
        verify_uv(raw.as_bytes(), &["project.run"], &mut host),
        Err(UvFailure::Lock("uv TOML"))
    );
    assert!(host.trace.is_empty());
}

#[test]
fn invalid_markers_and_missing_versions_do_not_become_installed_version_choices() {
    for raw in [
        lock(&[("yamaa", "0.2.0", Some(&["not a marker"]))]),
        lock(&[(
            "yamaa",
            "0.2.0",
            Some(&["sys_platform == 'linux'", "extra == 'cpu'"]),
        )]),
        b"version = 1\n[[package]]\nname = \"yamaa\"\nresolution-markers = []\n".to_vec(),
        b"version = 1\n[[package]]\nname = \"yamaa\"\nresolution-markers = [1]\n".to_vec(),
        b"version = 1\n[[package]]\nname = \"yamaa\"\n".to_vec(),
        b"version = 1\n[[package]]\nname = \"yamaa\"\nversion = \"not a version\"\n".to_vec(),
    ] {
        let mut host = Host::new(&[("yamaa", "0.2.0")], &[]);
        assert!(
            matches!(
                verify_uv(&raw, &["math.sqrt"], &mut host),
                Err(UvFailure::Lock(_))
            ),
            "{}",
            String::from_utf8_lossy(&raw)
        );
        assert!(!host.trace.contains(&"version yamaa".to_owned()));
    }
}

#[test]
fn uncalled_dynamic_package_can_omit_version_but_called_package_cannot() {
    let mut raw = lock(&[("yamaa", "0.2.0", None)]);
    raw.extend_from_slice(
        b"\n[[package]]\nname = \"program\"\nsource = {editable = \"program\"}\n",
    );
    let mut host = Host::new(&[("yamaa", "0.2.0")], &[("program", &["program"])]);
    assert_eq!(verify_uv(&raw, &["math.sqrt"], &mut host), Ok(Vec::new()));
    assert_eq!(
        verify_uv(&raw, &["program.value"], &mut host),
        Err(UvFailure::Lock("uv package version"))
    );
}

#[test]
fn original_metadata_failure_is_the_verification_failure() {
    let mut host = Host::new(&[], &[]);
    host.failure = Some("opaque metadata error");
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "0.2.0", None)]),
            &["math.sqrt"],
            &mut host
        ),
        Err(UvFailure::Host("opaque metadata error"))
    );
}

#[test]
fn missing_installed_version_is_a_finding_without_guessing_or_stopping_other_checks() {
    let mut host = Host::new(&[("yamaa", "0.2.0")], &[("project", &["project"])]);
    host.versions
        .insert("project".to_owned(), Installed::Unreadable);
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "0.2.0", None), ("project", "1", None)]),
            &["project.run"],
            &mut host
        ),
        Ok(vec![finding(
            "project",
            Reason::InvalidInstalledVersion,
            &["1"],
            None
        )])
    );
}

#[test]
fn an_installed_version_that_is_no_version_is_a_finding_with_its_text() {
    let mut host = Host::new(
        &[("yamaa", "0.2.0"), ("project", "1.0-local build")],
        &[("project", &["project"])],
    );
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "0.2.0", None), ("project", "1", None)]),
            &["project.run"],
            &mut host
        ),
        Ok(vec![finding(
            "project",
            Reason::InvalidInstalledVersion,
            &["1"],
            Some("1.0-local build")
        )])
    );
}

#[test]
fn existing_repository_uv_lock_identifies_yamaa_and_a_called_distribution() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../python/uv.lock");
    let raw = std::fs::read(path).unwrap();
    let document = std::str::from_utf8(&raw)
        .unwrap()
        .parse::<toml::Table>()
        .unwrap();
    let mut versions = Vec::new();
    for record in document["package"].as_array().unwrap() {
        let name = record["name"].as_str().unwrap();
        if name == "yamaa" || name == "pytest" {
            versions.push((name, record["version"].as_str().unwrap()));
        }
    }
    assert_eq!(versions.len(), 2);
    let mut host = Host::new(&versions, &[("pytest", &["pytest"])]);
    assert_eq!(verify_uv(&raw, &["pytest.main"], &mut host), Ok(Vec::new()));
    assert_eq!(host.trace, ["providers", "version pytest", "version yamaa"]);
}

#[test]
fn editable_distribution_names_normalize_and_versions_compare_by_value() {
    // The Python host maps an editable's module to its distribution's
    // metadata name; the lock spells the name normalized.
    let mut host = Host::new(
        &[("yamaa", "0.2.0"), ("clinical-programs", "1.2.0")],
        &[("project_bmi", &["Clinical-Programs"])],
    );
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "0.2.0", None), ("clinical-programs", "1.2", None)]),
            &["project_bmi.calculate"],
            &mut host
        ),
        Ok(Vec::new())
    );
    assert_eq!(
        host.trace,
        ["providers", "version clinical-programs", "version yamaa"]
    );
}

#[test]
fn a_distribution_reported_twice_is_one_needed_distribution() {
    // A site directory that two path entries reach, as `lib` and `lib64` are
    // on RHEL-family systems, lists a distribution twice; it is one package,
    // observed and reported once.
    let mut host = Host::new(
        &[("yamaa", "0.2.0"), ("a", "1")],
        &[("shared", &["a", "a", "A"])],
    );
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "0.2.0", None), ("a", "2", None)]),
            &["shared.run"],
            &mut host
        ),
        Ok(vec![finding(
            "a",
            Reason::VersionMismatch,
            &["2"],
            Some("1")
        )])
    );
    assert_eq!(host.trace, ["providers", "version a", "version yamaa"]);
}

#[test]
fn distribution_with_stdlib_root_name_is_version_checked_before_exemption() {
    let mut host = Host::new(
        &[("yamaa", "0.2.0"), ("shadow-statistics", "2")],
        &[("statistics", &["shadow-statistics"])],
    );
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "0.2.0", None), ("shadow-statistics", "1", None)]),
            &["statistics.mean"],
            &mut host
        ),
        Ok(vec![finding(
            "shadow-statistics",
            Reason::VersionMismatch,
            &["1"],
            Some("2")
        )])
    );
    assert_eq!(
        host.trace,
        ["providers", "version shadow-statistics", "version yamaa"]
    );
}

#[test]
fn locked_versions_count_once_by_value_and_print_normalized_in_order() {
    // `1.0` and `1.0.0` are one version, spelled as the first record has it.
    let raw = lock(&[
        ("yamaa", "0.2.0", None),
        ("project", "1.0", None),
        ("project", "1.0.0", None),
    ]);
    let mut host = Host::new(
        &[("yamaa", "0.2.0"), ("project", "1.0.0")],
        &[("project", &["project"])],
    );
    assert_eq!(verify_uv(&raw, &["project.run"], &mut host), Ok(Vec::new()));
    let raw = lock(&[
        ("yamaa", "0.2.0", None),
        ("project", "2.0-RC1", None),
        ("project", "v1.10", None),
        ("project", "1.9.post", None),
    ]);
    assert_eq!(
        verify_uv(&raw, &["project.run"], &mut host),
        Ok(vec![finding(
            "project",
            Reason::AmbiguousLockVersion,
            &["1.9.post0", "1.10", "2.0rc1"],
            None
        )])
    );
}

#[test]
fn a_record_selected_by_none_of_its_markers_leaves_the_package_unlocked() {
    let raw = lock(&[(
        "yamaa",
        "0.2.0",
        Some(&["sys_platform == 'win32'", "sys_platform == 'darwin'"]),
    )]);
    let mut host = Host::new(&[("yamaa", "0.2.0")], &[]);
    assert_eq!(
        verify_uv(&raw, &["math.sqrt"], &mut host),
        Ok(vec![finding("yamaa", Reason::VersionNotLocked, &[], None)])
    );
    assert_eq!(host.trace, ["providers", "environment"]);
}

#[test]
fn a_later_package_refuses_after_the_earlier_ones_were_observed() {
    let raw = lock(&[
        ("yamaa", "0.2.0", Some(&["sys_platform =="])),
        ("a", "1", None),
    ]);
    let mut host = Host::new(&[("a", "1")], &[("a", &["a"])]);
    assert_eq!(
        verify_uv(&raw, &["a.run"], &mut host),
        Err(UvFailure::Lock("uv resolution-markers"))
    );
    assert_eq!(host.trace, ["providers", "version a"]);
}

#[test]
fn every_record_names_a_package_and_every_provider_is_a_distribution_name() {
    for raw in [
        &b"version = 1\npackage = [1]\n"[..],
        b"version = 1\n[[package]]\nversion = \"1\"\n",
        b"version = 1\n[[package]]\nname = \"-invalid\"\nversion = \"1\"\n",
    ] {
        let mut host = Host::new(&[], &[]);
        assert_eq!(
            verify_uv(raw, &["math.sqrt"], &mut host),
            Err(UvFailure::Lock("uv package identity"))
        );
    }
    let mut host = Host::new(&[], &[("project", &["not a name"])]);
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "0.2.0", None)]),
            &["project.run"],
            &mut host
        ),
        Err(UvFailure::Distribution)
    );
}

/// CPython converts text of more than 640 digits to an integer only within
/// the interpreter's limit, 4,300 by default, so `packaging` refused a
/// version with a longer number, at each place where verification reads one.
/// The host is asked for the limit only when a number of more than 640
/// digits needs it.
fn numbers() -> (String, String) {
    ("1".repeat(VERSION_DIGITS), "1".repeat(VERSION_DIGITS + 1))
}

/// The refusal of a number of 4,301 digits at `origin` under the default.
fn refused(origin: &'static str) -> Result<Vec<Finding>, UvFailure<&'static str>> {
    Err(UvFailure::Digits {
        origin,
        digits: VERSION_DIGITS + 1,
        limit: VERSION_DIGITS,
    })
}

#[test]
fn a_locked_version_past_the_integer_limit_refuses_before_its_installed_version_is_asked() {
    let (at, past) = numbers();
    let mut host = Host::new(&[("yamaa", at.as_str())], &[]);
    assert_eq!(
        verify_uv(&lock(&[("yamaa", &at, None)]), &["math.sqrt"], &mut host),
        Ok(Vec::new())
    );
    // The limit is read for each conversion that needs it: reading the locked
    // version, printing it for the findings, then reading the installed one.
    assert_eq!(
        host.trace,
        [
            "providers",
            "integer digits",
            "integer digits",
            "version yamaa",
            "integer digits"
        ]
    );
    let mut host = Host::new(&[("yamaa", past.as_str())], &[]);
    assert_eq!(
        verify_uv(&lock(&[("yamaa", &past, None)]), &["math.sqrt"], &mut host),
        refused("uv package version")
    );
    assert_eq!(host.trace, ["providers", "integer digits"]);
}

#[test]
fn an_installed_version_past_the_integer_limit_refuses_after_it_is_asked() {
    let (at, past) = numbers();
    let raw = lock(&[("yamaa", "1.0", None)]);
    let mut host = Host::new(&[("yamaa", at.as_str())], &[]);
    assert_eq!(
        verify_uv(&raw, &["math.sqrt"], &mut host),
        Ok(vec![finding(
            "yamaa",
            Reason::VersionMismatch,
            &["1.0"],
            Some(&at)
        )])
    );
    assert_eq!(host.trace, ["providers", "version yamaa", "integer digits"]);
    let mut host = Host::new(&[("yamaa", past.as_str())], &[]);
    assert_eq!(
        verify_uv(&raw, &["math.sqrt"], &mut host),
        refused("installed version")
    );
    assert_eq!(host.trace, ["providers", "version yamaa", "integer digits"]);
}

#[test]
fn a_marker_bound_past_the_integer_limit_refuses_when_its_comparison_reads_it() {
    let (at, past) = numbers();
    let within = format!("python_version == '{at}'");
    let beyond = format!("python_version == '{past}'");
    let unread = format!("sys_platform == '{past}'");
    for (marker, trace) in [
        (&within, &["providers", "environment", "integer digits"][..]),
        (&unread, &["providers", "environment"][..]),
    ] {
        let mut host = Host::new(&[("yamaa", "0.2.0")], &[]);
        assert_eq!(
            verify_uv(
                &lock(&[("yamaa", "0.2.0", Some(&[marker.as_str()]))]),
                &["math.sqrt"],
                &mut host
            ),
            Ok(vec![finding("yamaa", Reason::VersionNotLocked, &[], None)])
        );
        assert_eq!(host.trace, trace);
    }
    let mut host = Host::new(&[("yamaa", "0.2.0")], &[]);
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "0.2.0", Some(&[beyond.as_str()]))]),
            &["math.sqrt"],
            &mut host
        ),
        refused("uv resolution-markers")
    );
    assert_eq!(host.trace, ["providers", "environment", "integer digits"]);
}

#[test]
fn the_interpreter_s_own_limit_decides_at_every_origin() {
    let past = "1".repeat(VERSION_DIGITS + 1);
    let beyond = "1".repeat(641);
    let marker = |number: &str| format!("python_version == '{number}'");
    // Raised or turned off, a limit lets packaging read 4,301 digits: the
    // locked version verifies, the installed one mismatches and the marker
    // comparison is false.
    for digits in [None, Some(10_000)] {
        let mut host = Host::new(&[("yamaa", past.as_str())], &[]);
        host.digits = digits;
        assert_eq!(
            verify_uv(&lock(&[("yamaa", &past, None)]), &["math.sqrt"], &mut host),
            Ok(Vec::new()),
            "{digits:?}"
        );
        assert_eq!(
            verify_uv(&lock(&[("yamaa", "1.0", None)]), &["math.sqrt"], &mut host),
            Ok(vec![finding(
                "yamaa",
                Reason::VersionMismatch,
                &["1.0"],
                Some(&past)
            )])
        );
        let bound = marker(&past);
        assert_eq!(
            verify_uv(
                &lock(&[("yamaa", "1.0", Some(&[bound.as_str()]))]),
                &["math.sqrt"],
                &mut host
            ),
            Ok(vec![finding("yamaa", Reason::VersionNotLocked, &[], None)])
        );
    }
    // Lowered to 640, the smallest limit CPython takes, it refuses 641
    // digits at each origin.
    let lowered = |origin| {
        Err(UvFailure::Digits {
            origin,
            digits: 641,
            limit: 640,
        })
    };
    let mut host = Host::new(&[("yamaa", beyond.as_str())], &[]);
    host.digits = Some(640);
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", &beyond, None)]),
            &["math.sqrt"],
            &mut host
        ),
        lowered("uv package version")
    );
    assert_eq!(
        verify_uv(&lock(&[("yamaa", "1.0", None)]), &["math.sqrt"], &mut host),
        lowered("installed version")
    );
    let bound = marker(&beyond);
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "1.0", Some(&[bound.as_str()]))]),
            &["math.sqrt"],
            &mut host
        ),
        lowered("uv resolution-markers")
    );
}

#[test]
fn the_limit_is_read_for_each_conversion_after_the_observations_before_it() {
    // packaging converted each number under the limit of that moment, which a
    // host observation made before it may have changed.
    let past = "1".repeat(VERSION_DIGITS + 1);
    let beyond = "1".repeat(641);
    let host = |installed: &str, sets: &[(&'static str, Option<usize>)]| {
        let mut host = Host::new(&[("yamaa", installed)], &[]);
        host.sets = sets.iter().copied().collect();
        host
    };
    // Turned off by the providers observation, then lowered to 640 by the
    // version lookup: the locked number converts, the installed one does not.
    let mut both = host(&past, &[("providers", None), ("version", Some(640))]);
    assert_eq!(
        verify_uv(&lock(&[("yamaa", &past, None)]), &["math.sqrt"], &mut both),
        Err(UvFailure::Digits {
            origin: "installed version",
            digits: VERSION_DIGITS + 1,
            limit: 640
        })
    );
    assert_eq!(
        both.trace,
        [
            "providers",
            "integer digits",
            "integer digits",
            "version yamaa",
            "integer digits"
        ]
    );
    let mut raised = host(&past, &[("providers", None)]);
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", &past, None)]),
            &["math.sqrt"],
            &mut raised
        ),
        Ok(Vec::new())
    );
    let mut lowered = host("0.2.0", &[("providers", Some(640))]);
    lowered.digits = None;
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", &beyond, None)]),
            &["math.sqrt"],
            &mut lowered
        ),
        Err(UvFailure::Digits {
            origin: "uv package version",
            digits: 641,
            limit: 640
        })
    );
    assert_eq!(lowered.trace, ["providers", "integer digits"]);
    let mut after_lookup = host(&past, &[("version", None)]);
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "1.0", None)]),
            &["math.sqrt"],
            &mut after_lookup
        ),
        Ok(vec![finding(
            "yamaa",
            Reason::VersionMismatch,
            &["1.0"],
            Some(&past)
        )])
    );
    // The marker environment read comes before the comparison it serves.
    let marker = format!("python_version == '{beyond}'");
    let mut marked = host("1.0", &[("environment", Some(640))]);
    marked.digits = None;
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "1.0", Some(&[marker.as_str()]))]),
            &["math.sqrt"],
            &mut marked
        ),
        Err(UvFailure::Digits {
            origin: "uv resolution-markers",
            digits: 641,
            limit: 640
        })
    );
    assert_eq!(marked.trace, ["providers", "environment", "integer digits"]);
    let marker = format!("python_version == '{past}'");
    let mut unmarked = host("1.0", &[("environment", None)]);
    assert_eq!(
        verify_uv(
            &lock(&[("yamaa", "1.0", Some(&[marker.as_str()]))]),
            &["math.sqrt"],
            &mut unmarked
        ),
        Ok(vec![finding("yamaa", Reason::VersionNotLocked, &[], None)])
    );
}

#[test]
fn a_locked_version_prints_under_the_limit_of_that_moment() {
    // packaging printed the locked versions it found, converting each number
    // back to text: a limit lowered after the version was read refuses it
    // there, by its significant digits.
    let beyond = format!("1.0+{}", "1".repeat(641));
    let mut host = Host::new(&[("yamaa", "1.0")], &[]);
    host.digits = None;
    host.sets.insert("environment", Some(640));
    let raw = lock(&[
        ("yamaa", &beyond, None),
        ("yamaa", "1.0", Some(&["sys_platform == 'linux'"])),
    ]);
    assert_eq!(
        verify_uv(&raw, &["math.sqrt"], &mut host),
        Err(UvFailure::Digits {
            origin: "uv package version",
            digits: 641,
            limit: 640
        })
    );
    assert_eq!(
        host.trace,
        [
            "providers",
            "integer digits",
            "environment",
            "integer digits"
        ]
    );
    // Leading zeros are no digits of the number printed.
    let zeros = format!("1.{}1", "0".repeat(700));
    let mut host = Host::new(&[("yamaa", "1.1")], &[]);
    host.digits = None;
    host.sets.insert("environment", Some(640));
    let raw = lock(&[
        ("yamaa", &zeros, None),
        ("yamaa", "1.1", Some(&["sys_platform == 'linux'"])),
    ]);
    assert_eq!(verify_uv(&raw, &["math.sqrt"], &mut host), Ok(Vec::new()));
}
