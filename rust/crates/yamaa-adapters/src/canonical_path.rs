//! One canonical spelling for the host paths the adapters record.
//!
//! `std::fs::canonicalize` returns a verbatim path on Windows: `\\?\C:\x` for
//! a local drive and `\\?\UNC\server\share\x` for a network share. Recorded
//! origins, roots and resource identities render with forward slashes, where
//! that prefix becomes `//?/`: a spelling no other host produces and one the
//! resource grammar refuses, because `//` reads as an empty segment
//! (FILE-5). Every adapter canonicalization goes through [`canonicalize`],
//! which drops the prefix when Windows can address the path without it and
//! keeps it for the paths that need it: those whose plain spelling reaches
//! the legacy `MAX_PATH` limit, and the device and volume namespaces, which
//! have no plain spelling at all.
//!
//! Resource identities are lexical and every open goes through a held
//! directory handle, so an identity never carries the prefix, whatever its
//! length: `without_verbatim_prefix` is the same rule without the length
//! guard. Both are string transformations, checked on every host with literal
//! `\\?\` spellings.
use std::{
    io,
    path::{Path, PathBuf},
};

/// The legacy Windows path limit, counting the terminating NUL: a plain
/// spelling of at most 259 UTF-16 units stays addressable without the prefix.
const MAX_PATH: usize = 260;

/// `std::fs::canonicalize` without a removable Windows verbatim prefix.
pub fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)?;
    Ok(match canonical.to_str().and_then(strip_verbatim_prefix) {
        Some(plain) => PathBuf::from(plain),
        None => canonical,
    })
}

/// The spelling a verbatim path has once Windows can address it without the
/// prefix: `\\?\C:\x` becomes `C:\x` and `\\?\UNC\server\share\x` becomes
/// `\\server\share\x`. `None` keeps the spelling: a path without the prefix,
/// a device or volume namespace (`\\?\Volume{...}\x`), and a plain spelling
/// that would reach `MAX_PATH`.
pub fn strip_verbatim_prefix(path: &str) -> Option<String> {
    without_verbatim_prefix(path).filter(|plain| plain.encode_utf16().count() < MAX_PATH)
}

/// The plain spelling of a verbatim path, when it has one: a local drive
/// (`\\?\C:\x` is `C:\x`) or a network share (`\\?\UNC\server\share\x` is
/// `\\server\share\x`). Other verbatim shapes and paths without the prefix
/// have none. Only `\` introduces the prefix: `//?/` is not a verbatim path
/// on Windows.
pub(crate) fn without_verbatim_prefix(path: &str) -> Option<String> {
    let rest = path.strip_prefix(r"\\?\")?;
    if let Some(unc) = rest.strip_prefix(r"UNC\") {
        let mut parts = unc.split('\\');
        let complete = parts.next().is_some_and(|server| !server.is_empty())
            && parts.next().is_some_and(|share| !share.is_empty());
        return complete.then(|| format!(r"\\{unc}"));
    }
    let bytes = rest.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    drive.then(|| rest.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_drive_and_network_share_paths_lose_the_prefix() {
        for (verbatim, plain) in [
            (r"\\?\C:\Users\study\spec.yaml", r"C:\Users\study\spec.yaml"),
            (r"\\?\C:\", r"C:\"),
            (r"\\?\d:\study", r"d:\study"),
            (
                r"\\?\UNC\server\share\study\spec.yaml",
                r"\\server\share\study\spec.yaml",
            ),
            (r"\\?\UNC\server\share", r"\\server\share"),
        ] {
            assert_eq!(strip_verbatim_prefix(verbatim).as_deref(), Some(plain));
            assert_eq!(without_verbatim_prefix(verbatim).as_deref(), Some(plain));
        }
    }

    #[test]
    fn other_shapes_keep_their_spelling() {
        for path in [
            r"\\?\Volume{b75e2c83-0000-0000-0000-602f00000000}\study\spec.yaml",
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\study",
            r"\\?\pipe\yamaa",
            r"\\?\C:",
            r"\\?\C:study",
            r"\\?\UNC\\share\study",
            r"\\?\UNC\server",
            r"\\?\UNC\server\\study",
            r"\\.\COM1",
            r"C:\Users\study\spec.yaml",
            r"\\server\share\study",
            "/tmp/study/spec.yaml",
            "//?/C:/Users/study/spec.yaml",
            "",
        ] {
            assert_eq!(strip_verbatim_prefix(path), None, "{path}");
            assert_eq!(without_verbatim_prefix(path), None, "{path}");
        }
    }

    #[test]
    fn paths_at_the_legacy_limit_keep_the_prefix_while_identities_never_do() {
        // `C:\` and the name together: 259 units fit, 260 do not.
        let fits = format!(r"\\?\C:\{}", "a".repeat(MAX_PATH - 4));
        let beyond = format!(r"\\?\C:\{}", "a".repeat(MAX_PATH - 3));
        assert_eq!(strip_verbatim_prefix(&fits).as_deref(), Some(&fits[4..]));
        assert_eq!(strip_verbatim_prefix(&beyond), None);
        assert_eq!(
            without_verbatim_prefix(&beyond).as_deref(),
            Some(&beyond[4..])
        );
        // The limit counts UTF-16 units, as the host does, not UTF-8 bytes.
        let accented = format!(r"\\?\C:\{}", "\u{e9}".repeat(MAX_PATH - 4));
        assert_eq!(
            strip_verbatim_prefix(&accented).as_deref(),
            Some(&accented[4..])
        );
        let share = format!(r"\\?\UNC\server\share\{}", "a".repeat(MAX_PATH));
        assert_eq!(strip_verbatim_prefix(&share), None);
        assert_eq!(
            without_verbatim_prefix(&share),
            Some(format!(r"\\server\share\{}", "a".repeat(MAX_PATH)))
        );
    }

    #[test]
    fn canonical_directories_are_addressable_without_the_prefix() {
        let directory =
            std::env::temp_dir().join(format!("yamaa-canonical-path-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let verbatim = std::fs::canonicalize(&directory).unwrap();
        let canonical = canonicalize(&directory).unwrap();
        let addressable = std::fs::metadata(&canonical).map(|m| m.is_dir());
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(addressable.unwrap());
        assert!(canonical.is_absolute());
        let text = canonical.to_str().unwrap();
        assert!(!text.starts_with(r"\\?\"), "{text}");
        #[cfg(windows)]
        {
            assert_eq!(verbatim.to_str().unwrap(), format!(r"\\?\{text}"));
            assert!(matches!(
                canonical.components().next(),
                Some(std::path::Component::Prefix(prefix))
                    if matches!(prefix.kind(), std::path::Prefix::Disk(_))
            ));
        }
        #[cfg(unix)]
        assert_eq!(canonical, verbatim);
        assert!(canonicalize(directory).is_err());
    }
}
