//! Bounded packaging-lock syntax at the host adapter boundary, never core planning.
//! R hosts can inspect renv JSON without adding an R JSON runtime dependency.
use crate::yaml_decode::{decode_yaml, DecodeFailure, DecodeLimits};
use serde_json::Value;
use std::collections::BTreeMap;
const MAX_BYTES: usize = 16_777_216;
const MAX_RECORDS: usize = 65_536;
const MAX_TEXT: usize = 2_048;
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
