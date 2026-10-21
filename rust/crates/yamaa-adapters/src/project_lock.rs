//! Bounded packaging-lock syntax at the host adapter boundary, never core planning.
//! R hosts can inspect renv JSON without adding an R JSON runtime dependency,
//! and Python hosts have their uv lock verified here without `packaging`.
use crate::python_packaging::{canonical_name, marker, version};
use crate::yaml_decode::{decode_yaml, DecodeFailure};
use std::collections::{BTreeMap, BTreeSet};
use yamaa_core::json::Value;

pub use yamaa_core::diagnostic::{LockFinding as Finding, LockReason as Reason};
#[derive(Debug)]
pub enum Error {
    Limit(yamaa_core::limits::LimitExceeded),
    Json(serde_json::Error),
    Document(DecodeFailure),
    Shape(&'static str),
}
#[derive(Debug, PartialEq, Eq)]
pub struct RenvLock {
    pub runtime_version: String,
    pub packages: BTreeMap<String, String>,
}
fn text(value: Option<&Value>, field: &'static str) -> Result<String, Error> {
    let Some(Value::String(text)) = value else {
        return Err(Error::Shape(field));
    };
    if text.is_empty() || text.contains('\0') {
        return Err(Error::Shape(field));
    }
    Ok(text.clone())
}
pub fn decode_renv(bytes: &[u8]) -> Result<RenvLock, Error> {
    yamaa_core::limits::Limits::spec_bytes(bytes.len()).map_err(Error::Limit)?;
    let root: Value = crate::json_wire::decode_slice(bytes).map_err(Error::Json)?;
    // JSON syntax is established first; the shared decoder then enforces
    // duplicate-key/node/text/depth bounds throughout this JSON subset of YAML.
    decode_yaml(bytes).map_err(Error::Document)?;
    let runtime = root
        .get("R")
        .and_then(|value| value.as_object())
        .ok_or(Error::Shape("R"))?;
    let runtime_version = text(runtime.get("Version"), "R.Version")?;
    let packages = root
        .get("Packages")
        .and_then(|value| value.as_object())
        .ok_or(Error::Shape("Packages"))?;
    let mut result = BTreeMap::new();
    for (name, record) in packages {
        if name.is_empty() || name.contains('\0') {
            return Err(Error::Shape("package name"));
        }
        let record = record.as_object().ok_or(Error::Shape("package record"))?;
        if let Some(package) = record.get("Package") {
            if text(Some(package), "Package")? != *name {
                return Err(Error::Shape("Package identity"));
            }
        }
        result.insert(name.clone(), text(record.get("Version"), "Version")?);
    }
    Ok(RenvLock {
        runtime_version,
        packages: result,
    })
}

/// Static lock-format classification from held bytes. Installed-version and
/// target-marker selection remain activation-time packaging responsibilities.
pub fn kind(bytes: &[u8]) -> Result<yamaa_core::project_environment::LockKind, Error> {
    use yamaa_core::project_environment::LockKind;
    yamaa_core::limits::Limits::spec_bytes(bytes.len()).map_err(Error::Limit)?;
    if bytes.iter().find(|byte| !byte.is_ascii_whitespace()) == Some(&b'{') {
        decode_renv(bytes)?;
        return Ok(LockKind::Renv);
    }
    let source = std::str::from_utf8(bytes).map_err(|_| Error::Shape("lock UTF-8"))?;
    let root = source
        .parse::<toml::Table>()
        .map_err(|_| Error::Shape("uv TOML"))?;
    if root.get("version").and_then(toml::Value::as_integer) != Some(1) {
        return Err(Error::Shape("uv version"));
    }
    let packages = root
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or(Error::Shape("uv package array"))?;
    for package in packages {
        let package = package
            .as_table()
            .ok_or(Error::Shape("uv package record"))?;
        for key in ["name", "version"] {
            // uv may omit an unresolved dynamic version. Activation checks
            // versions only for yamaa and packages selected by called functions.
            if key == "version" && !package.contains_key(key) {
                continue;
            }
            let value = package
                .get(key)
                .and_then(toml::Value::as_str)
                .ok_or(Error::Shape("uv package identity"))?;
            if value.is_empty() || value.contains('\0') {
                return Err(Error::Shape("uv package identity"));
            }
        }
    }
    Ok(LockKind::Uv)
}

/// What a Python host observes of its installation for uv lock verification.
/// The engine reads the lock and compares versions; the host only reports.
pub trait PythonInstallation {
    type Error;
    /// The installed distributions that provide each of the top-level
    /// `modules`, by module. A module that no distribution provides may be
    /// absent or have no distributions.
    fn providers(
        &mut self,
        modules: &[String],
    ) -> Result<BTreeMap<String, Vec<String>>, Self::Error>;
    /// Whether `module` belongs to the interpreter's standard library.
    fn standard_library(&mut self, module: &str) -> Result<bool, Self::Error>;
    /// The installed version of the distribution with the normalized name
    /// `distribution`.
    fn version(&mut self, distribution: &str) -> Result<Installed, Self::Error>;
    /// The marker environment: PEP 508's values, read from `sys`, `platform`
    /// and `os`.
    fn environment(&mut self) -> Result<BTreeMap<String, String>, Self::Error>;
}

/// An installed distribution's version, as its metadata records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Installed {
    /// No distribution of that name is installed.
    Missing,
    /// The version text that its metadata records.
    Version(String),
    /// Its metadata records no version text.
    Unreadable,
}

/// Why uv lock verification refused instead of reporting findings.
#[derive(Debug, PartialEq, Eq)]
pub enum UvFailure<E> {
    /// The lock is no uv lock that verification can read; the field says
    /// which part.
    Lock(&'static str),
    /// The host reported an installed distribution whose name is no valid
    /// distribution name.
    Distribution,
    /// A host observation failed.
    Host(E),
}

fn read_uv(bytes: &[u8]) -> Result<toml::Table, &'static str> {
    yamaa_core::limits::Limits::spec_bytes(bytes.len()).map_err(|_| "lock size")?;
    let source = std::str::from_utf8(bytes).map_err(|_| "lock UTF-8")?;
    let root = source.parse::<toml::Table>().map_err(|_| "uv TOML")?;
    if root.get("version").and_then(toml::Value::as_integer) != Some(1) {
        return Err("uv version");
    }
    if !root.get("package").is_some_and(toml::Value::is_array) {
        return Err("uv package array");
    }
    Ok(root)
}

/// Whether a package record applies to this interpreter: it has no
/// `resolution-markers`, or one of them holds. Every marker is evaluated, and
/// the host's marker environment is read once, when the first is.
fn selected<H: PythonInstallation>(
    record: &toml::Table,
    environment: &mut Option<marker::Environment>,
    host: &mut H,
) -> Result<bool, UvFailure<H::Error>> {
    const MARKERS: &str = "uv resolution-markers";
    let Some(markers) = record.get("resolution-markers") else {
        return Ok(true);
    };
    let markers = markers
        .as_array()
        .filter(|markers| !markers.is_empty())
        .ok_or(UvFailure::Lock(MARKERS))?;
    let mut any = false;
    for text in markers {
        let text = text.as_str().ok_or(UvFailure::Lock(MARKERS))?;
        let parsed = marker::parse(text).map_err(|_| UvFailure::Lock(MARKERS))?;
        if environment.is_none() {
            *environment = Some(marker::Environment::new(
                host.environment().map_err(UvFailure::Host)?,
            ));
        }
        let environment = environment.as_ref().expect("read above");
        any |= parsed
            .evaluate(environment)
            .map_err(|_| UvFailure::Lock(MARKERS))?;
    }
    Ok(any)
}

/// Verify a uv lock for the called Python functions (FUNC-3): yamaa and each
/// distribution that provides a call's top-level module must be installed at
/// the one version the lock selects for this interpreter. A module that no
/// distribution provides is exempt when it belongs to the standard library.
///
/// Findings come first for each call's module that no distribution provides,
/// then for each needed distribution in the order of its normalized name. The
/// host is asked in that order too: for the providers once, after the lock's
/// syntax is read; for the marker environment at most once, when the first
/// marker is evaluated; and for a distribution's version only when the lock
/// selects exactly one version of it. A refusal about one distribution comes
/// after the earlier ones were asked about. No calls means neither a lock
/// read nor an observation.
pub fn verify_uv<H: PythonInstallation>(
    bytes: &[u8],
    calls: &[&str],
    host: &mut H,
) -> Result<Vec<Finding>, UvFailure<H::Error>> {
    const IDENTITY: &str = "uv package identity";
    const VERSION: &str = "uv package version";
    if calls.is_empty() {
        return Ok(Vec::new());
    }
    let root = read_uv(bytes).map_err(UvFailure::Lock)?;
    let mut modules: Vec<String> = Vec::new();
    for call in calls {
        let module = call.split('.').next().unwrap_or(call);
        if !modules.iter().any(|known| known == module) {
            modules.push(module.to_owned());
        }
    }
    let providers = host.providers(&modules).map_err(UvFailure::Host)?;
    let mut findings = Vec::new();
    let mut needed = BTreeSet::from(["yamaa".to_owned()]);
    for module in &modules {
        match providers.get(module).filter(|names| !names.is_empty()) {
            Some(names) => {
                for name in names {
                    needed.insert(canonical_name(name).ok_or(UvFailure::Distribution)?);
                }
            }
            None => {
                if !host.standard_library(module).map_err(UvFailure::Host)? {
                    findings.push(Finding {
                        package: module.clone(),
                        reason: Reason::DistributionNotIdentified,
                        expected: Vec::new(),
                        actual: None,
                    });
                }
            }
        }
    }
    let mut records: BTreeMap<String, Vec<&toml::Table>> = BTreeMap::new();
    for record in root
        .get("package")
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
    {
        let record = record.as_table().ok_or(UvFailure::Lock(IDENTITY))?;
        let name = record
            .get("name")
            .and_then(toml::Value::as_str)
            .and_then(canonical_name)
            .ok_or(UvFailure::Lock(IDENTITY))?;
        if needed.contains(&name) {
            records.entry(name).or_default().push(record);
        }
    }
    let mut environment = None;
    for name in needed {
        // Versions that denote the same version count once, as the first
        // record spells it.
        let mut expected: Vec<version::Version> = Vec::new();
        for record in records.get(&name).into_iter().flatten() {
            if !selected(record, &mut environment, host)? {
                continue;
            }
            let locked = record
                .get("version")
                .and_then(toml::Value::as_str)
                .and_then(version::version)
                .ok_or(UvFailure::Lock(VERSION))?;
            if !expected.contains(&locked) {
                expected.push(locked);
            }
        }
        expected.sort();
        let choices = expected.iter().map(ToString::to_string).collect();
        if expected.len() != 1 {
            findings.push(Finding {
                package: name,
                reason: if expected.is_empty() {
                    Reason::VersionNotLocked
                } else {
                    Reason::AmbiguousLockVersion
                },
                expected: choices,
                actual: None,
            });
            continue;
        }
        let (reason, actual) = match host.version(&name).map_err(UvFailure::Host)? {
            Installed::Missing => (Reason::PackageNotInstalled, None),
            Installed::Unreadable => (Reason::InvalidInstalledVersion, None),
            Installed::Version(text) => match version::version(&text) {
                None => (Reason::InvalidInstalledVersion, Some(text)),
                Some(installed) if installed != expected[0] => {
                    (Reason::VersionMismatch, Some(text))
                }
                Some(_) => continue,
            },
        };
        findings.push(Finding {
            package: name,
            reason,
            expected: choices,
            actual,
        });
    }
    Ok(findings)
}
