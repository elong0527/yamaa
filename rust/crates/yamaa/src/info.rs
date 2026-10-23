//! Engine information (D41, P-INFO-1): what the engine is and where it runs.
//!
//! [`info`] gives the same six facts in every host, in this order: the engine
//! version, the `yamaa:` language versions the engine reads, the optional
//! features compiled into it, the host, the host's own version and the
//! platform. The command line prints them for `yamaa --version`. A host names
//! itself and its interpreter version; every other fact belongs to the
//! compiled engine, and none is a build time, commit or toolchain, so the same
//! source gives the same facts.

/// The hosts as written, one for each [`Host`] in declaration order: the
/// closed vocabulary of [`Info::host`].
pub const HOSTS: [&str; 3] = ["python", "r", "cli"];

/// The optional format features as written, sorted, by their Cargo names:
/// the closed vocabulary of [`Info::features`]. Every shipped package and
/// binary compiles all four. The `arrow` feature, the Arrow form of tables,
/// reads no format and is not one of them.
pub const FEATURES: [&str; 4] = ["odm", "parquet", "readstat", "yaml"];

/// The host a caller runs in, as [`Info::host`] names it.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    /// The Python package.
    Python,
    /// The R package.
    R,
    /// The `yamaa` command line.
    Cli,
}

impl Host {
    /// The host as written: `python`, `r` or `cli`, one of [`HOSTS`].
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::R => "r",
            Self::Cli => "cli",
        }
    }
}

/// What the engine is and where it runs: P-INFO-1's six fields, in order.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Info {
    /// The engine version, [`crate::VERSION`], the same in every host; a
    /// release candidate reads like `1.0.0-rc.1`.
    pub version: &'static str,
    /// Every `yamaa:` language version the engine reads, oldest first: `1.0`
    /// up to the newest it implements (SCHEMA-2).
    pub language_versions: Vec<String>,
    /// The optional format features compiled into the engine, sorted, by
    /// their Cargo names: those of [`FEATURES`] that this build compiles.
    pub features: Vec<&'static str>,
    /// The host that asked.
    pub host: Host,
    /// The Python or R version the host runs on; `None` for the command
    /// line.
    pub host_version: Option<String>,
    /// `<os>-<arch>` of the compiled engine, such as `linux-x86_64`,
    /// `linux-aarch64`, `windows-x86_64` or `macos-aarch64`. It names no C
    /// library, so a musl and a glibc build read alike.
    pub platform: String,
}

/// Whether this build compiles each of [`FEATURES`], in its order.
const COMPILED: [bool; 4] = [
    cfg!(feature = "odm"),
    cfg!(feature = "parquet"),
    cfg!(feature = "readstat"),
    cfg!(feature = "yaml"),
];

/// The engine information `host` reports, with `host_version` the Python or
/// R version it runs on, or `None` for the command line.
///
/// # Example
///
/// ```
/// let info = yamaa::info(yamaa::Host::Cli, None);
/// assert_eq!(info.version, yamaa::VERSION);
/// assert_eq!(info.host.as_str(), "cli");
/// assert_eq!(info.host_version, None);
/// assert!(info.platform.contains('-'));
/// ```
pub fn info(host: Host, host_version: Option<&str>) -> Info {
    use yamaa_core::language::{language_versions, LANGUAGE_VERSION};

    Info {
        version: crate::VERSION,
        language_versions: language_versions(Some(LANGUAGE_VERSION)),
        features: FEATURES
            .into_iter()
            .zip(COMPILED)
            .filter_map(|(name, compiled)| compiled.then_some(name))
            .collect(),
        host,
        host_version: host_version.map(str::to_owned),
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_are_the_hosts_in_order() {
        let hosts = [Host::Python, Host::R, Host::Cli];
        for host in hosts {
            // Exhaustive: a new host fails to compile here until it is listed
            // above, and so in HOSTS.
            match host {
                Host::Python | Host::R | Host::Cli => {}
            }
        }
        assert_eq!(hosts.map(Host::as_str), HOSTS);
    }

    #[test]
    fn features_are_sorted_and_name_what_a_build_may_compile() {
        assert!(FEATURES.windows(2).all(|pair| pair[0] < pair[1]));
        let compiled = info(Host::Cli, None).features;
        assert!(compiled.iter().all(|feature| FEATURES.contains(feature)));
    }
}
