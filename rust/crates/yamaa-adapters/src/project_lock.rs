//! Bounded packaging-lock syntax at the host adapter boundary, never core planning.
//! R hosts can inspect renv JSON without adding an R JSON runtime dependency.
use crate::yaml_decode::{decode_yaml, DecodeFailure, DecodeLimits};
use serde_json::Value;
use std::collections::BTreeMap;
const MAX_BYTES: usize = 16_777_216;
const MAX_RECORDS: usize = 65_536;
const MAX_TEXT: usize = 2_048;

/// Closed facts from installed host metadata. Version comparison belongs to the
/// host packaging tool; shared projection never reparses or compares versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    DistributionNotIdentified,
    VersionNotLocked,
    AmbiguousLockVersion,
    InvalidLockedVersion,
    PackageNotInstalled,
    InvalidInstalledVersion,
    VersionMismatch,
}
impl Reason {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "distribution_not_identified" => Self::DistributionNotIdentified,
            "version_not_locked" => Self::VersionNotLocked,
            "ambiguous_lock_version" => Self::AmbiguousLockVersion,
            "invalid_locked_version" => Self::InvalidLockedVersion,
            "package_not_installed" => Self::PackageNotInstalled,
            "invalid_installed_version" => Self::InvalidInstalledVersion,
            "version_mismatch" => Self::VersionMismatch,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DistributionNotIdentified => "distribution_not_identified",
            Self::VersionNotLocked => "version_not_locked",
            Self::AmbiguousLockVersion => "ambiguous_lock_version",
            Self::InvalidLockedVersion => "invalid_locked_version",
            Self::PackageNotInstalled => "package_not_installed",
            Self::InvalidInstalledVersion => "invalid_installed_version",
            Self::VersionMismatch => "version_mismatch",
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct Finding {
    pub package: String,
    pub reason: Reason,
    pub expected: Vec<String>,
    pub actual: Option<String>,
}
#[derive(Debug)]
pub enum Error {
    Limit(&'static str),
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
    if text.len() > MAX_TEXT {
        return Err(Error::Limit("text"));
    }
    Ok(text.clone())
}
pub fn decode_renv(bytes: &[u8]) -> Result<RenvLock, Error> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit("bytes"));
    }
    let root: Value = serde_json::from_slice(bytes).map_err(Error::Json)?;
    // JSON syntax is established first; the shared decoder then enforces
    // duplicate-key/node/text/depth bounds throughout this JSON subset of YAML.
    decode_yaml(bytes, DecodeLimits::default()).map_err(Error::Document)?;
    let runtime = root
        .get("R")
        .and_then(|value| value.as_object())
        .ok_or(Error::Shape("R"))?;
    let runtime_version = text(runtime.get("Version"), "R.Version")?;
    let packages = root
        .get("Packages")
        .and_then(|value| value.as_object())
        .ok_or(Error::Shape("Packages"))?;
    if packages.len() > MAX_RECORDS {
        return Err(Error::Limit("packages"));
    }
    let mut result = BTreeMap::new();
    for (name, record) in packages {
        if name.is_empty() || name.contains('\0') {
            return Err(Error::Shape("package name"));
        }
        if name.len() > MAX_TEXT {
            return Err(Error::Limit("text"));
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
    if bytes.len() > MAX_BYTES {
        return Err(Error::Limit("bytes"));
    }
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
    if packages.len() > MAX_RECORDS {
        return Err(Error::Limit("packages"));
    }
    for package in packages {
        let package = package
            .as_table()
            .ok_or(Error::Shape("uv package record"))?;
        for key in ["name", "version"] {
            let value = package
                .get(key)
                .and_then(toml::Value::as_str)
                .ok_or(Error::Shape("uv package identity"))?;
            if value.is_empty() || value.contains('\0') || value.len() > MAX_TEXT {
                return Err(Error::Shape("uv package identity"));
            }
        }
    }
    Ok(LockKind::Uv)
}
