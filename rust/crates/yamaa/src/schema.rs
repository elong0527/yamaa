//! The JSON Schema of each file type (#3030 section 2.12, D149): the
//! machine-readable contract that editors validate and complete a yamaa file
//! against as it is typed.
//!
//! [`schema`] gives the JSON Schema of `spec`, `project`, `codelists` or
//! `function` as canonical JSON text. It is the document that
//! `tools/packaging/generate_json_schema.py` generates from the type's schema
//! files and commits in `yaml/json/`, indented there for review. The
//! engine embeds the committed file when it is built, so nothing is read at
//! run time, and every host returns the same text, which the command line
//! prints.

use crate::api::Failure;
use crate::file_type::FileType;
use crate::text::Action;

/// The JSON Schema of the file type `file_type`, which is compared exactly
/// with the types a file writes in `type`: `spec`, `project`, `codelists` or
/// `function`, the [`crate::FILE_TYPES`].
///
/// The text is canonical JSON (RFC 8785), as every JSON text the engine
/// writes: one line without a final line feed, members in key order, numbers
/// as written. A specification file may be one layer of an inheritance
/// chain, so the `spec` schema checks what one layer may write (LAYER-4), and
/// its `$defs/root_class` checks a complete specification.
///
/// Any other text is refused as `unexpected_file_type` (SCHEMA-2), with the
/// text as given as `type` and the four types as `expected`, at no spec path,
/// since no file is read. The refusal names the call as `cannot give the
/// schema of <type>` (D39).
///
/// # Example
///
/// ```
/// let schema = yamaa::schema("spec").unwrap();
/// assert!(schema.starts_with(r#"{"$defs":{"#));
/// assert!(schema.ends_with(r#""title":"yamaa 1.0 spec file"}"#));
///
/// let refused = yamaa::schema("specs").unwrap_err();
/// assert_eq!(
///     refused.message,
///     "cannot give the schema of specs: a specs file is given where a spec, \
///      project, codelists, function file is expected [unexpected_file_type, SCHEMA-2]"
/// );
/// ```
pub fn schema(file_type: &str) -> Result<String, Failure> {
    match FileType::parse(file_type) {
        Some(parsed) => Ok(yamaa_adapters::shipped_schema::json_schema(
            parsed.into_core(),
        )),
        None => Err(Failure::from_finding(
            Action::Schema(file_type),
            yamaa_core::language::unknown_file_type(file_type),
        )),
    }
}
