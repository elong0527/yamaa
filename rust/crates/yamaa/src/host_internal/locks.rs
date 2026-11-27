//! The packaging-lock helpers the host bindings and the command line's
//! worker verify a project's lock with (FUNC-3): an renv lock decoded for R,
//! and a uv lock verified against what a Python host observes of its
//! installation. Part of the host-internal seam ([`crate::host_internal`]).

use std::collections::BTreeMap;

use crate::host::functions::{FunctionError, FunctionIdentity, LockFinding, LockSyntaxError};

/// A decoded renv lock: the runtime version plus package versions by name.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenvLock {
    pub runtime_version: String,
    pub packages: BTreeMap<String, String>,
}

/// Decode an renv lock without adding a JSON runtime dependency to the host.
/// Shape, limit, JSON and document failures surface as [`LockSyntaxError`].
pub fn decode_renv_lock(bytes: &[u8]) -> Result<RenvLock, LockSyntaxError> {
    yamaa_adapters::project_lock::decode_renv(bytes)
        .map(|locked| RenvLock {
            runtime_version: locked.runtime_version,
            packages: locked.packages,
        })
        .map_err(|error| LockSyntaxError::from_adapter(&error))
}

/// What a Python host reports of its installation for [`verify_uv_lock`].
/// The engine reads the lock and compares versions; the host only observes.
/// Every method runs on the calling thread of the operation, and a failure
/// is the verification's failure, unchanged.
pub trait InstalledDistributions {
    /// The installed distributions that provide each of the top-level
    /// `modules`, by module. A module that no distribution provides may be
    /// absent or have no distributions.
    fn providers(&self, modules: &[String])
        -> Result<BTreeMap<String, Vec<String>>, FunctionError>;

    /// Whether `module` belongs to the interpreter's standard library, whose
    /// functions verification exempts.
    fn standard_library(&self, module: &str) -> Result<bool, FunctionError>;

    /// The installed version of the distribution with the normalized name
    /// `distribution`.
    fn version(&self, distribution: &str) -> Result<InstalledVersion, FunctionError>;

    /// The marker environment that a lock's `resolution-markers` select by:
    /// the values PEP 508 names, read from `sys`, `platform` and `os`.
    fn environment(&self) -> Result<BTreeMap<String, String>, FunctionError>;

    /// The interpreter's limit on the digits of an integer converted from
    /// text (`sys.get_int_max_str_digits()`), `None` when it is turned off.
    /// Verification asks only when a version it reads has a number of more
    /// than 640 digits, below which CPython converts whatever its limit.
    /// The provided method reports CPython's default, 4,300 digits.
    fn integer_digits(&self) -> Result<Option<usize>, FunctionError> {
        Ok(Some(yamaa_adapters::project_lock::VERSION_DIGITS))
    }
}

/// An installed distribution's version, as its metadata records it.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstalledVersion {
    /// No distribution of that name is installed.
    Missing,
    /// The version text that its metadata records.
    Version(String),
    /// Its metadata records no version text.
    Unreadable,
}

/// Verify a uv lock for the Python functions about to run (FUNC-3): yamaa
/// and each distribution that provides a called function's top-level module
/// must be installed at the one version that the lock selects for this
/// interpreter. A module that no distribution provides is exempt when it
/// belongs to the standard library. The engine reads the lock and compares
/// under PEP 440, 503 and 508, as Python's `packaging` 26 does, so a Python
/// host needs no packaging library; `installed` reports the installation.
///
/// Findings fail with [`FunctionError::Lock`]; a lock that cannot be read as
/// a uv lock with [`FunctionError::LockSyntax`]; an installed distribution
/// whose name is invalid with [`FunctionError::Protocol`]; and a failure of
/// `installed` propagates unchanged. No functions means no lock read and no
/// observation.
///
/// A version that the comparison reads with a number longer than the
/// interpreter's integer limit allows (`installed.integer_digits()`, 4,300
/// digits by default) fails with [`LockSyntaxError::Limit`], wherever it is
/// written: in the lock, in a resolution marker or in the installed
/// metadata, as `packaging` refused to convert it.
pub fn verify_uv_lock(
    lock: &[u8],
    functions: &[FunctionIdentity],
    installed: &dyn InstalledDistributions,
) -> Result<(), FunctionError> {
    use yamaa_adapters::project_lock::{verify_uv, UvFailure};
    /// The installation behind the adapters' observation port.
    struct Installation<'a>(&'a dyn InstalledDistributions);

    impl yamaa_adapters::project_lock::PythonInstallation for Installation<'_> {
        type Error = FunctionError;

        fn providers(
            &mut self,
            modules: &[String],
        ) -> Result<BTreeMap<String, Vec<String>>, FunctionError> {
            self.0.providers(modules)
        }

        fn standard_library(&mut self, module: &str) -> Result<bool, FunctionError> {
            self.0.standard_library(module)
        }

        fn version(
            &mut self,
            distribution: &str,
        ) -> Result<yamaa_adapters::project_lock::Installed, FunctionError> {
            use yamaa_adapters::project_lock::Installed;
            Ok(match self.0.version(distribution)? {
                InstalledVersion::Missing => Installed::Missing,
                InstalledVersion::Version(text) => Installed::Version(text),
                InstalledVersion::Unreadable => Installed::Unreadable,
            })
        }

        fn environment(&mut self) -> Result<BTreeMap<String, String>, FunctionError> {
            self.0.environment()
        }

        fn integer_digits(&mut self) -> Result<Option<usize>, FunctionError> {
            self.0.integer_digits()
        }
    }

    let calls = functions
        .iter()
        .map(|function| function.call.as_str())
        .collect::<Vec<_>>();
    match verify_uv(lock, &calls, &mut Installation(installed)) {
        Ok(findings) if findings.is_empty() => Ok(()),
        Ok(findings) => Err(FunctionError::Lock(
            findings.into_iter().map(LockFinding::from_core).collect(),
        )),
        Err(UvFailure::Lock(field)) => {
            Err(FunctionError::LockSyntax(LockSyntaxError::Shape { field }))
        }
        Err(UvFailure::Distribution) => Err(FunctionError::Protocol),
        Err(UvFailure::Digits {
            origin,
            digits,
            limit,
        }) => Err(FunctionError::LockSyntax(LockSyntaxError::Limit {
            resource: origin,
            limit,
            actual: Some(digits),
        })),
        Err(UvFailure::Host(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::functions::LockReason;

    /// An installation with one installed distribution per module, named
    /// after it, which records the observations it is asked for.
    struct Installation {
        installed: &'static [(&'static str, &'static str)],
        interrupt: bool,
        observed: std::sync::Mutex<Vec<String>>,
    }

    impl Installation {
        fn new(installed: &'static [(&'static str, &'static str)]) -> Self {
            Self {
                installed,
                interrupt: false,
                observed: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn observe(&self, observation: String) {
            self.observed.lock().unwrap().push(observation);
        }
    }

    impl InstalledDistributions for Installation {
        fn providers(
            &self,
            modules: &[String],
        ) -> Result<BTreeMap<String, Vec<String>>, FunctionError> {
            self.observe("providers".to_owned());
            Ok(modules
                .iter()
                .map(|module| (module.clone(), vec![module.replace('_', " ")]))
                .collect())
        }

        fn standard_library(&self, _module: &str) -> Result<bool, FunctionError> {
            Ok(false)
        }

        fn version(&self, distribution: &str) -> Result<InstalledVersion, FunctionError> {
            self.observe(format!("version {distribution}"));
            if self.interrupt {
                return Err(FunctionError::Interrupted { payload: None });
            }
            Ok(self
                .installed
                .iter()
                .find(|(name, _)| *name == distribution)
                .map_or(InstalledVersion::Missing, |(_, version)| {
                    InstalledVersion::Version((*version).to_owned())
                }))
        }

        fn environment(&self) -> Result<BTreeMap<String, String>, FunctionError> {
            self.observe("environment".to_owned());
            Ok(BTreeMap::new())
        }
    }

    /// An installation whose interpreter has its integer limit turned off.
    struct Unlimited(Installation);

    impl InstalledDistributions for Unlimited {
        fn providers(
            &self,
            modules: &[String],
        ) -> Result<BTreeMap<String, Vec<String>>, FunctionError> {
            self.0.providers(modules)
        }

        fn standard_library(&self, module: &str) -> Result<bool, FunctionError> {
            self.0.standard_library(module)
        }

        fn version(&self, distribution: &str) -> Result<InstalledVersion, FunctionError> {
            self.0.version(distribution)
        }

        fn environment(&self) -> Result<BTreeMap<String, String>, FunctionError> {
            self.0.environment()
        }

        fn integer_digits(&self) -> Result<Option<usize>, FunctionError> {
            Ok(None)
        }
    }

    #[test]
    fn uv_locks_verify_against_the_host_installation() {
        let lock = b"version = 1\n[[package]]\nname = \"yamaa\"\nversion = \"1.0\"\n";
        let calls = [FunctionIdentity {
            name: "f".into(),
            call: "yamaa.spec".into(),
        }];
        let installed = Installation::new(&[("yamaa", "1.0.0")]);
        assert!(verify_uv_lock(lock, &calls, &installed).is_ok());
        assert_eq!(
            *installed.observed.lock().unwrap(),
            ["providers", "version yamaa"]
        );
        let installed = Installation::new(&[("yamaa", "2.0")]);
        match verify_uv_lock(lock, &calls, &installed) {
            Err(FunctionError::Lock(findings)) => assert_eq!(
                findings,
                [LockFinding::new(
                    "yamaa".into(),
                    LockReason::VersionMismatch,
                    vec!["1.0".into()],
                    Some("2.0".into()),
                )]
            ),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            verify_uv_lock(b"version = 2\npackage = []\n", &calls, &installed),
            Err(FunctionError::LockSyntax(LockSyntaxError::Shape {
                field: "uv version"
            }))
        ));
        // The installation names `not_a_name`'s provider `not a name`.
        let invalid = [FunctionIdentity {
            name: "g".into(),
            call: "not_a_name.f".into(),
        }];
        assert!(matches!(
            verify_uv_lock(lock, &invalid, &installed),
            Err(FunctionError::Protocol)
        ));
        let mut interrupted = Installation::new(&[]);
        interrupted.interrupt = true;
        assert!(matches!(
            verify_uv_lock(lock, &calls, &interrupted),
            Err(FunctionError::Interrupted { payload: None })
        ));
        let untouched = Installation::new(&[]);
        assert!(verify_uv_lock(b"not a lock", &[], &untouched).is_ok());
        assert!(untouched.observed.lock().unwrap().is_empty());
        // Under CPython's default limit, which the provided method reports, a
        // number of 4,301 digits refuses with its length and the limit.
        let past: &'static str = Box::leak("1".repeat(4301).into_boxed_str());
        let huge: &'static [(&'static str, &'static str)] = Box::leak(Box::new([("yamaa", past)]));
        assert!(matches!(
            verify_uv_lock(lock, &calls, &Installation::new(huge)),
            Err(FunctionError::LockSyntax(LockSyntaxError::Limit {
                resource: "installed version",
                limit: 4300,
                actual: Some(4301)
            }))
        ));
        // An interpreter that converts any length reads the version, which
        // the lock does not hold.
        match verify_uv_lock(lock, &calls, &Unlimited(Installation::new(huge))) {
            Err(FunctionError::Lock(findings)) => assert_eq!(
                findings,
                [LockFinding::new(
                    "yamaa".into(),
                    LockReason::VersionMismatch,
                    vec!["1.0".into()],
                    Some(past.into()),
                )]
            ),
            other => panic!("{other:?}"),
        }
    }

    #[cfg(feature = "yaml")]
    #[test]
    fn renv_locks_decode_without_a_host_json_runtime() {
        let lock =
            br#"{"R":{"Version":"4.3.1"},"Packages":{"pkgA":{"Package":"pkgA","Version":"1.0"}}}"#;
        let decoded = decode_renv_lock(lock).unwrap();
        assert_eq!(decoded.runtime_version, "4.3.1");
        assert_eq!(
            decoded.packages.get("pkgA").map(String::as_str),
            Some("1.0")
        );
        let error = decode_renv_lock(b"not json").unwrap_err();
        assert!(matches!(error, LockSyntaxError::Json { .. }));
        let error = decode_renv_lock(br#"{"R":{}}"#).unwrap_err();
        assert!(matches!(error, LockSyntaxError::Shape { .. }));
    }
}
