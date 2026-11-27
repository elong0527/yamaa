//! The packaging locks a project names: the format classification every
//! project preparation runs, the renv reader an R host's activation calls
//! (RFC 8259 JSON), and uv lock verification (FUNC-3) with its PEP 440
//! version and PEP 508 marker readers. Each input is a lock, then optionally
//! a NUL byte and the version text an installation reports for every
//! distribution (a lock is text, which holds no NUL). A lock classified as
//! renv must read as one, since the classification reads it so.
#![no_main]

use std::collections::BTreeMap;

use libfuzzer_sys::fuzz_target;
use yamaa_adapters::project_lock::{
    decode_renv, kind, verify_uv, Installed, PythonInstallation, VERSION_DIGITS,
};
use yamaa_core::project_environment::LockKind;

/// The calls a project's functions make: a module a distribution of the
/// same name provides, one no distribution provides that the standard
/// library holds, and one no distribution provides at all.
const CALLS: [&str; 5] = [
    "numpy.mean",
    "pandas",
    "polars.col",
    "os.path.join",
    "unprovided",
];

/// A CPython 3.12 installation on Linux, whose every distribution reports
/// the input's version text and whose integer limit is CPython's default.
struct Installation<'a> {
    version: &'a str,
}

impl PythonInstallation for Installation<'_> {
    type Error = std::convert::Infallible;

    fn providers(
        &mut self,
        modules: &[String],
    ) -> Result<BTreeMap<String, Vec<String>>, Self::Error> {
        Ok(modules
            .iter()
            .filter(|module| !["os", "unprovided"].contains(&module.as_str()))
            .map(|module| (module.clone(), vec![module.clone()]))
            .collect())
    }

    fn standard_library(&mut self, module: &str) -> Result<bool, Self::Error> {
        Ok(module == "os")
    }

    fn version(&mut self, _distribution: &str) -> Result<Installed, Self::Error> {
        Ok(Installed::Version(self.version.into()))
    }

    fn environment(&mut self) -> Result<BTreeMap<String, String>, Self::Error> {
        Ok([
            ("implementation_name", "cpython"),
            ("implementation_version", "3.12.8"),
            ("os_name", "posix"),
            ("platform_machine", "x86_64"),
            ("platform_python_implementation", "CPython"),
            ("platform_release", "6.8.0"),
            ("platform_system", "Linux"),
            ("platform_version", "#1 SMP"),
            ("python_full_version", "3.12.8"),
            ("python_version", "3.12"),
            ("sys_platform", "linux"),
        ]
        .into_iter()
        .map(|(name, value)| (name.into(), value.into()))
        .collect())
    }

    fn integer_digits(&mut self) -> Result<Option<usize>, Self::Error> {
        Ok(Some(VERSION_DIGITS))
    }
}

fuzz_target!(|data: &[u8]| {
    let (lock, version) = match data.iter().position(|&byte| byte == 0) {
        Some(at) => (&data[..at], String::from_utf8_lossy(&data[at + 1..])),
        None => (data, "1.0".into()),
    };
    let renv = decode_renv(lock);
    if let Ok(LockKind::Renv) = kind(lock) {
        assert!(renv.is_ok(), "a lock classified as renv reads as one");
    }
    let _ = verify_uv(lock, &CALLS, &mut Installation { version: &version });
});
