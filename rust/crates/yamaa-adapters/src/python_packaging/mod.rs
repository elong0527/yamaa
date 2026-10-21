//! The subset of Python packaging standards that uv lock verification needs:
//! PEP 503 distribution names, PEP 440 versions and specifiers, and PEP 508
//! markers. Each follows Python's `packaging` 26, which the Python host used
//! for this comparison before it moved into the engine (#3030 D11), so a lock
//! verifies with the same findings.
pub(crate) mod marker;
pub(crate) mod version;

/// The PEP 503 normalized form of a distribution name: lowercase, with each
/// run of `-`, `_` and `.` written as one `-`. `None` for a name that is no
/// valid distribution name, which begins and ends with an ASCII letter or
/// digit and holds only those, `-`, `_` and `.`.
pub(crate) fn canonical_name(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let valid = bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !valid {
        return None;
    }
    let mut normalized = String::with_capacity(name.len());
    let mut separated = false;
    for byte in bytes {
        if matches!(byte, b'-' | b'_' | b'.') {
            separated = true;
            continue;
        }
        if separated {
            normalized.push('-');
            separated = false;
        }
        normalized.push(char::from(byte.to_ascii_lowercase()));
    }
    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::canonical_name;

    #[test]
    fn names_normalize_as_pep_503_says() {
        for (name, expected) in [
            ("yamaa", Some("yamaa")),
            ("Project_BMI", Some("project-bmi")),
            ("oslo.concurrency", Some("oslo-concurrency")),
            ("a-_.-b", Some("a-b")),
            ("A", Some("a")),
            ("0", Some("0")),
            ("", None),
            ("-a", None),
            ("a-", None),
            ("_a", None),
            ("a b", None),
            ("caf\u{e9}", None),
            ("a+b", None),
        ] {
            assert_eq!(canonical_name(name).as_deref(), expected, "{name:?}");
        }
    }
}
