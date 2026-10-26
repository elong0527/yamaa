#![cfg(feature = "odm")]
//! ODM XML clinical-item contracts, from the retained Python reader suite and
//! refusals that suite did not cover. The reader's reads and refusals of text
//! files are cases of the application suite's
//! `tests/fixtures/diagnostics/odm_reads_xml.json` (#3798). The tests left here prove seams the replay cannot reach, each
//! named in its first comment: archives and gzip streams built in the test,
//! bytes that are not UTF-8 text, generated bounds and batches, resolver
//! doubles, links and Unix file names, the upstream tokenizer's messages, the
//! table's Arrow types and the engine's own table.
use crate::support;

use support::odm_files::{cell, column, gzip, ordinals, Header, Tar};
use support::{edit, scratch::Scratch};
use yamaa_adapters::{
    column_table::ColumnTable,
    odm_xml::{self, Error},
    public_table::PublicTable,
};
use yamaa_core::{
    odm_items::{self, Options},
    table::{TableAccess, ValueRef},
};

const ODM_13: &str = include_str!("fixtures/odm/odm13.xml");
const ODM_20: &str = include_str!("fixtures/odm/odm20.xml");

fn read_with(path: &std::path::Path, options: &Options) -> Result<PublicTable, odm_items::Error> {
    match odm_xml::read(path, options) {
        Ok(table) => Ok(table),
        Err(Error::Read(error)) => Err(error),
        Err(error) => panic!("unexpected adapter failure: {error:?}"),
    }
}

/// The condition a refusal of the source reports (ODM-1).
fn condition(error: &odm_items::Error) -> &'static str {
    yamaa_core::diagnostic::OdmReadFinding {
        refusal: error,
        path: "source.xml",
        dataset: None,
    }
    .diagnostic()
    .condition()
}

/// The one issue a refusal of `source`, named as given, reports.
#[track_caller]
fn issue(result: Result<PublicTable, Error>, source: &str) -> yamaa_core::diagnostic::Diagnostic {
    let error = result.expect_err("the source must be refused");
    error
        .issue(source)
        .unwrap_or_else(|| panic!("{error:?} reports no issue"))
}

fn read_document(document: &str) -> Result<PublicTable, odm_items::Error> {
    read_bytes("document.xml", document.as_bytes(), &Options::default())
}

fn read_bytes(
    name: &str,
    bytes: &[u8],
    options: &Options,
) -> Result<PublicTable, odm_items::Error> {
    let scratch = Scratch::new("read");
    let path = scratch.write(name, bytes);
    read_with(&path, options)
}

fn rows(document: &str) -> PublicTable {
    read_document(document).unwrap()
}

/// Assert the condition a refusal reports and that its reason contains
/// `message`.
#[track_caller]
fn refused(result: Result<PublicTable, odm_items::Error>, expected: &str, message: &str) {
    refused_case("the input", result, expected, message);
}

/// `case` must be refused with condition `expected` and a message holding
/// `message`.
#[track_caller]
fn refused_case(
    case: &str,
    result: Result<PublicTable, odm_items::Error>,
    expected: &str,
    message: &str,
) {
    let error = match result {
        Ok(_) => panic!("{case}: the input must be refused"),
        Err(error) => error,
    };
    assert_eq!(condition(&error), expected, "{case}: {error}");
    let text = error.to_string();
    assert!(
        text.contains(message),
        "{case}: {text:?} does not contain {message:?}"
    );
}

fn some(values: &[&str]) -> Vec<Option<String>> {
    values
        .iter()
        .map(|value| Some((*value).to_owned()))
        .collect()
}

mod python_reader_suite {
    use super::*;

    /// Seam: the Arrow type and nullability of each field, which a case's cells
    /// do not carry.
    #[test]
    fn read_odm_returns_the_fixed_schema_table() {
        let table = rows(ODM_20);
        let names = table
            .columns()
            .iter()
            .map(|field| field.name().as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, odm_items::COLUMNS);
        for field in table.columns() {
            let expected = if field.name() == "SourceOrdinal" {
                arrow_schema::DataType::Int64
            } else {
                arrow_schema::DataType::Utf8
            };
            assert_eq!(field.data_type(), &expected, "{}", field.name());
            assert!(field.is_nullable());
        }
        assert_eq!(table.rows(), 2);
        assert_eq!(ordinals(&table), [1, 2]);
    }

    /// Seam: a gzip TAR archive built in the test, which a case's text files
    /// cannot hold.
    #[test]
    fn archive_with_one_xml_member_is_selected_automatically() {
        let archive = gzip(
            &Tar::new()
                .file("study/odm.xml", ODM_20.as_bytes())
                .file("study/index.html", b"<html/>")
                .file("study/._odm.xml", b"apple-double")
                .finish(),
        );
        let table = read_bytes("odm.tar.gz", &archive, &Options::default()).unwrap();
        assert_eq!(column(&table, "ItemOID"), some(&["I.TEST", "I.RESULT"]));
    }

    /// Seam: a gzip TAR archive built in the test, which a case's text files
    /// cannot hold.
    #[test]
    fn archive_ignores_macosx_metadata_member() {
        let archive = gzip(
            &Tar::new()
                .file("study/odm.xml", ODM_20.as_bytes())
                .file("__MACOSX/study/odm.xml", b"not xml")
                .finish(),
        );
        let table = read_bytes("odm-macosx.tar.gz", &archive, &Options::default()).unwrap();
        assert_eq!(column(&table, "ItemOID"), some(&["I.TEST", "I.RESULT"]));
    }

    /// Seam: a gzip TAR archive built in the test, which a case's text files
    /// cannot hold.
    #[test]
    fn archive_member_selects_one_of_multiple_xml_files() {
        let archive = gzip(
            &Tar::new()
                .file("first.xml", ODM_20.as_bytes())
                .file("nested/second.xml", ODM_20.as_bytes())
                .finish(),
        );
        refused(
            read_bytes("multiple.tar.gz", &archive, &Options::default()),
            "source_odm_archive_member",
            "exactly one XML member",
        );
        let options = Options {
            archive_member: Some("nested/second.xml".into()),
            ..Options::default()
        };
        assert_eq!(
            read_bytes("multiple.tar.gz", &archive, &options)
                .unwrap()
                .rows(),
            2
        );
    }

    /// Seam: a gzip TAR archive built in the test, which a case's text files
    /// cannot hold.
    #[test]
    fn archive_member_count_at_bound_is_read() {
        let archive = gzip(
            &Tar::new()
                .file("study/odm.xml", ODM_20.as_bytes())
                .file("study/index.html", b"<html/>")
                .file("study/notes.txt", b"notes")
                .finish(),
        );
        let options = Options {
            max_archive_members: 3,
            ..Options::default()
        };
        assert_eq!(
            read_bytes("bounded.tar.gz", &archive, &options)
                .unwrap()
                .rows(),
            2
        );
    }

    /// Seam: a TAR archive of twenty generated members built in the test.
    #[test]
    fn archive_member_count_above_bound_is_rejected() {
        let tar = (0..20).fold(Tar::new(), |tar, index| {
            tar.file(&format!("member-{index}.txt"), index.to_string().as_bytes())
        });
        let options = Options {
            max_archive_members: 3,
            ..Options::default()
        };
        refused(
            read_bytes("too-many-members.tar", &tar.finish(), &options),
            "source_odm_limit_exceeded",
            "more than 3 members",
        );
    }

    /// Seam: a gzip TAR archive built in the test, which a case's text files
    /// cannot hold.
    #[test]
    fn archive_rejects_non_regular_xml_member() {
        let archive = gzip(
            &Tar::new()
                .special("study/odm.xml", b'2', "elsewhere.xml")
                .finish(),
        );
        refused(
            read_bytes("unsafe.tar.gz", &archive, &Options::default()),
            "source_odm_archive_member",
            "not a regular file",
        );
    }

    /// Seam: a gzip TAR archive built in the test and cut at byte offsets.
    #[test]
    fn truncated_compressed_archive_has_helper_error() {
        let archive = gzip(&Tar::new().file("study/odm.xml", ODM_20.as_bytes()).finish());
        for cut in [30, 250] {
            refused(
                read_bytes("truncated.tar.gz", &archive[..cut], &Options::default()),
                "source_odm_archive_invalid",
                "cannot read ODM archive",
            );
        }
    }
}

mod table {
    use super::*;

    /// Seam: the table's record batches, which a case's rows do not show.
    #[test]
    fn documents_without_items_produce_the_complete_schema() {
        let document = edit(
            ODM_13,
            &ODM_13[ODM_13.find("    <SubjectData").unwrap()
                ..ODM_13.find("  </ClinicalData>").unwrap()],
            "",
        );
        let table = rows(&document);
        assert_eq!(table.rows(), 0);
        assert_eq!(table.columns().len(), 30);
        assert_eq!(table.batches().len(), 1);
    }

    /// Seam: the engine's own table and schema, read by column index.
    #[test]
    fn the_table_is_an_ordinary_engine_input() {
        let table = rows(ODM_13);
        let input = ColumnTable::from_public(&table).unwrap();
        assert_eq!(input.schema(), &odm_items::schema());
        assert_eq!(input.row_count(), 3);
        assert_eq!(input.cell(0, 26).unwrap(), ValueRef::Str("2026-01-02"));
        assert_eq!(input.cell(2, 26).unwrap(), ValueRef::Missing);
        assert_eq!(input.cell(2, 29).unwrap(), ValueRef::Int(3));
        let fields = yamaa_core::odm_source::FIELDS;
        for field in fields {
            assert!(odm_items::COLUMNS.contains(&field), "{field}");
        }
    }

    /// Seam: 65,537 generated items and the record batches they split into.
    #[test]
    fn large_documents_split_into_engine_batches_in_source_order() {
        let items = (0..65_537)
            .map(|index| format!(r#"<ItemData ItemOID="I.{index}" Value="{index}"/>"#))
            .collect::<String>();
        let table = rows(&edit(
            ODM_13,
            concat!(
                r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                "\n            ",
                r#"<ItemData ItemOID="I.EMPTY" Value=""/>"#,
                "\n            ",
                r#"<ItemData ItemOID="I.NULL" IsNull="Yes"/>"#,
            ),
            &items,
        ));
        assert_eq!(table.rows(), 65_537);
        assert_eq!(
            table
                .batches()
                .iter()
                .map(|batch| batch.num_rows())
                .collect::<Vec<_>>(),
            [65_536, 1]
        );
        let ordinals = ordinals(&table);
        assert!(ordinals.iter().copied().eq(1..=65_537));
        assert_eq!(cell(&table, "Value", 65_536).as_deref(), Some("65536"));
    }
}

mod xml_strictness {
    use super::*;
    use crate::support::Edit as _;

    /// A 1.3 document holding `body`, then an empty `ClinicalData`, so a body
    /// that is well formed reads as an empty table.
    fn wrap(body: &str) -> String {
        format!(
            r#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">{body}<ClinicalData StudyOID="S" MetaDataVersionOID="M"/></ODM>"#
        )
    }

    /// Seam: messages the upstream XML tokenizer writes, which the repository
    /// does not author, so each is checked by a part of its text, where a case
    /// compares a reason whole. Every other well-formedness refusal is a case
    /// of `odm_reads_xml.json`.
    #[test]
    fn well_formedness_follows_xml_1_0() {
        for (document, message) in [
            (wrap("<a></b>"), "expected `</a>`"),
            (wrap("& "), "reference"),
            (wrap("<!-- a -- b -->"), "`--`"),
        ] {
            refused(
                read_document(&document),
                "source_odm_xml_malformed",
                message,
            );
        }
    }

    /// Seam: 4,095 and 4,096 generated namespace declarations.
    #[test]
    fn namespace_bindings_in_scope_are_bounded() {
        let declarations = |count: usize| {
            (0..count)
                .map(|index| format!(" xmlns:p{index}=\"urn:p{index}\""))
                .collect::<String>()
        };
        assert!(read_document(&wrap(&format!("<x{}/>", declarations(4095)))).is_ok());
        refused(
            read_document(&wrap(&format!("<x{}/>", declarations(4096)))),
            "source_odm_limit_exceeded",
            "XML namespace declarations in scope exceed 4096",
        );
    }

    /// Seam: bytes that are not UTF-8 text (UTF-16, an invalid UTF-8 byte),
    /// which a case's text files cannot hold, beside the characters around
    /// them.
    #[test]
    fn characters_and_encodings_are_utf8_xml_only() {
        let utf16 = wrap("")
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        for (bytes, code, message) in [
            (
                wrap("\u{1}").into_bytes(),
                "source_odm_xml_malformed",
                "invalid XML character U+0001",
            ),
            (
                wrap("\u{fffe}").into_bytes(),
                "source_odm_xml_malformed",
                "invalid XML character U+FFFE",
            ),
            (
                wrap("\u{ffff}").into_bytes(),
                "source_odm_xml_malformed",
                "invalid XML character U+FFFF",
            ),
            (
                wrap("\0").into_bytes(),
                "source_odm_xml_malformed",
                "invalid XML character U+0000",
            ),
            (
                format!(
                    "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>{}",
                    wrap("")
                )
                .into_bytes(),
                "source_odm_xml_unsupported",
                "must be encoded as UTF-8, not ISO-8859-1",
            ),
            (
                [&[0xFF, 0xFE][..], &utf16].concat(),
                "source_odm_xml_unsupported",
                "not UTF-16",
            ),
            (utf16.clone(), "source_odm_xml_unsupported", "not UTF-16"),
        ] {
            refused(
                read_bytes("chars.xml", &bytes, &Options::default()),
                code,
                message,
            );
        }
        let mut invalid_utf8 = wrap("x").into_bytes();
        let position = invalid_utf8.iter().rposition(|&byte| byte == b'x').unwrap();
        invalid_utf8[position] = 0xFF;
        refused(
            read_bytes("utf8.xml", &invalid_utf8, &Options::default()),
            "source_odm_xml_malformed",
            "invalid UTF-8",
        );
        // An invalid byte in the first token is reported at its source
        // offset, past a byte order mark too.
        for (prefix, at) in [(&[][..], 4), (&[0xEF, 0xBB, 0xBF][..], 7)] {
            let bytes = [prefix, b"<ODM\xff>"].concat();
            refused(
                read_bytes("first.xml", &bytes, &Options::default()),
                "source_odm_xml_malformed",
                &format!("invalid UTF-8 byte sequence at byte {at}"),
            );
        }
        let bom = [&[0xEF, 0xBB, 0xBF][..], ODM_13.as_bytes()].concat();
        assert_eq!(
            read_bytes("bom.xml", &bom, &Options::default())
                .unwrap()
                .rows(),
            3
        );
        let lower = ODM_13.edit(r#"encoding="UTF-8""#, r#"encoding="utf8""#);
        assert_eq!(rows(&lower).rows(), 3);
        let controls = rows(&edit(
            ODM_13,
            r#"Value="2026-01-02""#,
            "Value=\"\u{7f}\u{85}\"",
        ));
        assert_eq!(cell(&controls, "Value", 0).as_deref(), Some("\u{7f}\u{85}"));
    }

    /// Seam: generated tokens at and over the ten-megabyte and 50,000-byte
    /// bounds.
    #[test]
    fn text_attribute_and_name_sizes_stay_within_libxml2_bounds() {
        let at_bound = "x".repeat(9_999_999);
        let table = rows(&edit(
            ODM_20,
            "<Value>PD-L1</Value>",
            &format!("<Value>{at_bound}</Value>"),
        ));
        assert_eq!(
            cell(&table, "Value", 0).map(|value| value.len()),
            Some(9_999_999)
        );
        let over = "x".repeat(10_000_000);
        refused(
            read_document(&edit(
                ODM_20,
                "<Value>PD-L1</Value>",
                &format!("<Value>{over}</Value>"),
            )),
            "source_odm_limit_exceeded",
            "XML text node exceeds 9999999 bytes",
        );
        refused(
            read_document(&edit(
                ODM_20,
                "<Value>PD-L1</Value>",
                &format!(
                    "<Value>{}&amp;{}</Value>",
                    &over[..5_000_000],
                    &over[..5_000_000]
                ),
            )),
            "source_odm_limit_exceeded",
            "XML text node exceeds",
        );
        refused(
            read_document(&edit(
                ODM_13,
                r#"Value="2026-01-02""#,
                &format!(r#"Value="{over}""#),
            )),
            "source_odm_limit_exceeded",
            "XML attribute value exceeds 9999999 bytes",
        );
        refused(
            read_document(&wrap(&format!("<!--{over}-->"))),
            "source_odm_limit_exceeded",
            "XML comment exceeds",
        );
        refused(
            read_document(&wrap(&format!("<{}/>", "a".repeat(50_001)))),
            "source_odm_limit_exceeded",
            "XML name exceeds 50000 bytes",
        );
        assert!(read_document(&wrap(&format!("<{}/>", "a".repeat(50_000)))).is_ok());
    }

    /// Seam: generated nesting at and over the option's and the reader's depth
    /// bounds.
    #[test]
    fn depth_counts_every_element_within_the_option_and_libxml2_bound() {
        let nested = |depth: usize| {
            wrap(&format!(
                "{}{}",
                "<x:e xmlns:x=\"urn:x\">".repeat(depth - 1),
                "</x:e>".repeat(depth - 1)
            ))
        };
        let options = |depth| Options {
            max_xml_depth: depth,
            ..Options::default()
        };
        assert!(read_bytes("deep.xml", nested(8).as_bytes(), &options(8)).is_ok());
        refused(
            read_bytes("deep.xml", nested(9).as_bytes(), &options(8)),
            "source_odm_limit_exceeded",
            "XML depth exceeds 8",
        );
        assert!(read_bytes("deep.xml", nested(256).as_bytes(), &options(1000)).is_ok());
        refused(
            read_bytes("deep.xml", nested(257).as_bytes(), &options(1000)),
            "source_odm_limit_exceeded",
            "XML depth exceeds 256",
        );
        refused(
            read_bytes("deep.xml", ODM_13.as_bytes(), &options(7)),
            "value_not_permitted",
            "max_xml_depth must be at least 8",
        );
    }
}

mod inputs {
    use super::*;

    /// Seam: a symbolic link, which a case does not stage.
    ///
    /// The file is a caller path read through the local file resolver: a
    /// link in its place is refused as a symbolic link (FILE-8).
    #[cfg(unix)]
    #[test]
    fn a_linked_source_fails_as_a_symbolic_link() {
        let scratch = Scratch::new("linked");
        let target = scratch.write("odm13.xml", ODM_13);
        let link = scratch.path().join("linked.xml");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let shown = link.display().to_string();
        let refusal = issue(odm_xml::read(&link, &Options::default()), &shown);
        assert_eq!(
            (refusal.condition(), refusal.rule()),
            ("resource_path_symlink", Some("FILE-8"))
        );
        assert_eq!(
            refusal.context["path"],
            yamaa_core::json::Value::String(shown)
        );
        assert_eq!(read_with(&target, &Options::default()).unwrap().rows(), 3);
    }

    /// Seam: a caller's resolver double.
    ///
    /// A caller's resolver serves the source under its own name; what it
    /// reports instead of a regular file fails as FILE-7 says.
    #[test]
    fn a_resolver_serves_the_source_once_under_its_name() {
        use std::sync::Arc;
        use yamaa_adapters::host_resources::{MemoryResolver, ResourceKind};
        let resolver = Arc::new(MemoryResolver::default());
        resolver.insert("/", "edc/odm.xml", "odm:1", Arc::from(ODM_13.as_bytes()));
        resolver.insert_kind("/", "edc/folder.xml", ResourceKind::Directory);
        let read = |source: &str| {
            odm_xml::read_with_resolver(resolver.clone(), source, &Options::default())
        };
        assert_eq!(read("edc/odm.xml").unwrap().rows(), 3);
        for (source, expected) in [
            ("edc/folder.xml", "resource_path_not_regular_file"),
            ("edc/absent.xml", "resource_path_missing"),
        ] {
            let refusal = issue(read(source), source);
            assert_eq!(refusal.condition(), expected, "{source}");
            assert_eq!(
                refusal.context["path"],
                yamaa_core::json::Value::String(source.into())
            );
        }
    }

    /// Seam: a resolver double that counts its requests, and a sparse file over
    /// the source bound.
    ///
    /// Options are checked once the source is found and before any of it is
    /// read: a file whose content would be refused, or a resolver asked for
    /// nothing, reports the option.
    #[test]
    fn options_are_checked_before_the_source_is_read() {
        use std::sync::{Arc, Mutex};
        use yamaa_adapters::host_resources::{Failure, Reply, Request, Resolver};
        let invalid = Options {
            max_xml_depth: 1,
            ..Options::default()
        };
        refused(
            read_bytes("odm13.xml", b"<not xml", &invalid),
            "value_not_permitted",
            "max_xml_depth must be at least 8",
        );
        // A sparse file over the ten-gigabyte source bound is never read.
        #[cfg(unix)]
        {
            let scratch = Scratch::new("sparse-options");
            let path = scratch.write("odm.xml", ODM_13);
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(yamaa_core::limits::Limits::SOURCE_BYTES as u64 + 1)
                .unwrap();
            refused(
                read_with(&path, &invalid),
                "value_not_permitted",
                "max_xml_depth must be at least 8",
            );
        }
        #[derive(Default)]
        struct Counting(Mutex<usize>);
        impl Resolver for Counting {
            fn request(&self, _: Request<'_>) -> Result<Reply, Failure> {
                *self.0.lock().unwrap() += 1;
                Err(Failure::Missing)
            }
        }
        let resolver = Arc::new(Counting::default());
        let result = odm_xml::read_with_resolver(resolver.clone(), "edc/odm.xml", &invalid);
        let Err(Error::Read(error)) = result else {
            panic!("invalid options were not refused: {result:?}");
        };
        assert_eq!(condition(&error), "value_not_permitted");
        assert_eq!(*resolver.0.lock().unwrap(), 0);
    }

    /// Seam: a resolver double that reports an oversized capture, and a sparse
    /// file over the source bound.
    ///
    /// A source over the capture's bound is over the read's limit, not an
    /// I/O failure, whether the local file resolver or a caller's resolver
    /// refuses it. A caller's resolver holds a source whole under the
    /// per-file source bound, whose limit names the source, XML or archive,
    /// and that bound; a local plain XML file is refused at
    /// `max_expanded_bytes`.
    #[test]
    fn a_capture_over_its_bound_reports_the_limit() {
        use std::sync::Arc;
        use yamaa_adapters::host_resources::{Failure, Reply, Request, Resolver};
        struct Oversized;
        impl Resolver for Oversized {
            fn request(&self, request: Request<'_>) -> Result<Reply, Failure> {
                assert_eq!(
                    request.limit,
                    Some(yamaa_core::limits::Limits::SOURCE_BYTES)
                );
                Err(Failure::Limit(Some(u64::MAX)))
            }
        }
        for source in ["edc/odm.xml", "edc/odm.tar.gz"] {
            let result =
                odm_xml::read_with_resolver(Arc::new(Oversized), source, &Options::default());
            let Err(Error::Read(error)) = result else {
                panic!("an oversized source was not refused: {result:?}");
            };
            assert_eq!(condition(&error), "source_odm_limit_exceeded", "{error}");
            assert_eq!(
                error.to_string(),
                format!(
                    "ODM source exceeds {} bytes",
                    yamaa_core::limits::Limits::SOURCE_BYTES
                ),
                "{source}"
            );
        }
        // A sparse plain XML file over the ten-gigabyte source bound is
        // refused at its own limit before any of it is read.
        #[cfg(unix)]
        {
            let scratch = Scratch::new("sparse-xml");
            let path = scratch.write("odm.xml", ODM_13);
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(yamaa_core::limits::Limits::SOURCE_BYTES as u64 + 1)
                .unwrap();
            refused(
                read_with(&path, &Options::default()),
                "source_odm_limit_exceeded",
                &format!(
                    "ODM XML exceeds {} bytes",
                    Options::DEFAULT_MAX_EXPANDED_BYTES
                ),
            );
        }
    }

    /// Seam: Unix file names holding `:` or `\`, which not every host can
    /// stage.
    ///
    /// A caller's own file name is a host name, not a written path: a `:`, or
    /// a backslash on Unix, reads as any other name.
    #[cfg(unix)]
    #[test]
    fn caller_file_names_with_colons_and_backslashes_are_read() {
        for name in ["odm:v2.xml", "odm\\v2.xml", "c:odm.xml", "file:odm.xml"] {
            let scratch = Scratch::new("names");
            let path = scratch.write(name, ODM_13);
            assert_eq!(
                read_with(&path, &Options::default()).unwrap().rows(),
                3,
                "{name}"
            );
        }
    }

    /// Seam: a gzip stream built in the test, which a case's text files cannot
    /// hold.
    ///
    /// A gzip stream named as no archive is read as XML, whose first byte is
    /// not an XML character. An archive name that holds no archive is a case
    /// of `odm_reads_xml.json`.
    #[test]
    fn a_gzipped_xml_file_is_read_as_xml() {
        let gzipped_xml = gzip(ODM_13.as_bytes());
        refused(
            read_bytes("odm.xml.gz", &gzipped_xml, &Options::default()),
            "source_odm_xml_malformed",
            "invalid XML character U+001F",
        );
    }

    /// Seam: the magic bytes of other compressions, which are not UTF-8 text.
    #[test]
    fn other_compressed_archives_are_refused_with_their_format() {
        for (magic, format) in [
            (&b"BZh91AY&SY"[..], "bzip2"),
            (&[0xFD, b'7', b'z', b'X', b'Z', 0x00, 0x00][..], "xz"),
            (&[0x28, 0xB5, 0x2F, 0xFD, 0x00][..], "zstd"),
        ] {
            refused(
                read_bytes("odm.tar.compressed", magic, &Options::default()),
                "source_odm_compression_unsupported",
                &format!(
                    "uses {format} compression; only plain and gzip TAR archives are supported"
                ),
            );
        }
    }
}

mod archives {
    use super::*;

    fn read_tar(bytes: &[u8], options: &Options) -> Result<PublicTable, odm_items::Error> {
        read_bytes("odm.tar", bytes, options)
    }

    fn member(name: &str) -> Options {
        Options {
            archive_member: Some(name.into()),
            ..Options::default()
        }
    }

    /// Seam: TAR archives built in the test.
    #[test]
    fn plain_archives_and_header_layouts_are_read() {
        let plain = Tar::new()
            .special("study", b'5', "")
            .file("study/odm.xml", ODM_13.as_bytes())
            .finish();
        assert_eq!(read_tar(&plain, &Options::default()).unwrap().rows(), 3);
        let long = format!("{}/odm.xml", "nested".repeat(30));
        for archive in [
            Tar::new()
                .pax(&[("path", &long)], "short.bin", ODM_13.as_bytes())
                .finish(),
            Tar::new().gnu_long_name(&long, ODM_13.as_bytes()).finish(),
        ] {
            assert_eq!(read_tar(&archive, &member(&long)).unwrap().rows(), 3);
            assert_eq!(read_tar(&archive, &Options::default()).unwrap().rows(), 3);
        }
        let prefixed = Tar::new()
            .prefixed("study/folder", "odm.xml", ODM_13.as_bytes())
            .finish();
        assert_eq!(
            read_tar(&prefixed, &member("study/folder/odm.xml"))
                .unwrap()
                .rows(),
            3
        );
        let global = Tar::new()
            .pax_global(&[("comment", "release")])
            .file("odm.xml", ODM_13.as_bytes())
            .unterminated();
        assert_eq!(read_tar(&global, &Options::default()).unwrap().rows(), 3);
    }

    /// Seam: TAR archives built in the test.
    #[test]
    fn selection_counts_exact_names_and_candidates() {
        let none = Tar::new().file("notes.txt", b"x").finish();
        refused(
            read_tar(&none, &Options::default()),
            "source_odm_archive_member",
            "exactly one XML member",
        );
        refused(
            read_tar(&none, &member("odm.xml")),
            "source_odm_archive_member",
            r#"TAR input contains 0 entries named "odm.xml""#,
        );
        let twice = Tar::new()
            .file("odm.xml", ODM_13.as_bytes())
            .file("odm.xml", ODM_13.as_bytes())
            .finish();
        refused(
            read_tar(&twice, &member("odm.xml")),
            "source_odm_archive_member",
            r#"TAR input contains 2 entries named "odm.xml""#,
        );
        let upper = Tar::new().file("STUDY/ODM.XML", ODM_13.as_bytes()).finish();
        assert_eq!(read_tar(&upper, &Options::default()).unwrap().rows(), 3);
        refused(
            read_tar(&upper, &member("study/odm.xml")),
            "source_odm_archive_member",
            "0 entries",
        );
        let empty = vec![0_u8; 1024];
        refused(
            read_tar(&empty, &Options::default()),
            "source_odm_archive_member",
            "exactly one XML member",
        );
    }

    /// Seam: TAR archives built in the test, some with patched headers.
    #[test]
    fn unsafe_irregular_and_sparse_members_are_refused() {
        for name in ["../odm.xml", "/abs/odm.xml", "study/../../odm.xml"] {
            let archive = Tar::new().file(name, ODM_13.as_bytes()).finish();
            refused(
                read_tar(&archive, &Options::default()),
                "source_odm_archive_member",
                &format!("unsafe TAR member path {name:?}"),
            );
        }
        for (typeflag, link) in [
            (b'1', "other.xml"),
            (b'2', "other.xml"),
            (b'5', ""),
            (b'6', ""),
        ] {
            let archive = Tar::new().special("odm.xml", typeflag, link).finish();
            refused(
                read_tar(&archive, &Options::default()),
                "source_odm_archive_member",
                r#"TAR member "odm.xml" is not a regular file"#,
            );
        }
        // Directories carry no data blocks even with a size field.
        let sized_directory = Tar::new()
            .entry(&Header::new("study/", 0, 600), &[])
            .entry(&Header::new("data", b'5', 600), &[])
            .file("odm.xml", ODM_13.as_bytes())
            .finish();
        assert_eq!(
            read_tar(&sized_directory, &Options::default())
                .unwrap()
                .rows(),
            3
        );
        let v7_directory = Tar::new().special("odm.xml/", 0, "").finish();
        refused(
            read_tar(&v7_directory, &Options::default()),
            "source_odm_archive_member",
            "not a regular file",
        );
        let mut sparse = Header::new("odm.xml", b'S', ODM_13.len() as u64);
        sparse.gnu = true;
        let archive = Tar::new().entry(&sparse, ODM_13.as_bytes()).finish();
        refused(
            read_tar(&archive, &Options::default()),
            "source_odm_archive_member",
            "sparse file",
        );
        let pax_sparse = Tar::new()
            .pax(
                &[("GNU.sparse.major", "1"), ("GNU.sparse.minor", "0")],
                "odm.xml",
                ODM_13.as_bytes(),
            )
            .finish();
        refused(
            read_tar(&pax_sparse, &Options::default()),
            "source_odm_archive_member",
            "sparse file",
        );
        let mut extended = Header::new("skipped.bin", b'S', 4);
        extended.gnu = true;
        extended.sparse_extended = true;
        let mut extension = [0_u8; 512];
        extension[504] = 0;
        let skipped = Tar::new()
            .raw(&extended.encode())
            .raw(&extension)
            .raw(&[7; 512])
            .file("odm.xml", ODM_13.as_bytes())
            .finish();
        assert_eq!(read_tar(&skipped, &Options::default()).unwrap().rows(), 3);
    }

    /// Seam: plain and gzip TAR archives built in the test.
    #[test]
    fn member_and_expansion_bounds_apply() {
        let archive = Tar::new()
            .file("odm.xml", ODM_13.as_bytes())
            .file("large.bin", &vec![b'z'; 200_000])
            .finish();
        let size = ODM_13.len() as u64;
        let tight = Options {
            max_expanded_bytes: size,
            ..Options::default()
        };
        assert_eq!(read_tar(&archive, &tight).unwrap().rows(), 3);
        let small = Options {
            max_expanded_bytes: size - 1,
            ..Options::default()
        };
        refused(
            read_tar(&archive, &small),
            "source_odm_limit_exceeded",
            &format!("TAR member exceeds {} bytes", size - 1),
        );
        let compressed = gzip(&archive);
        let expansion = Options {
            max_expanded_bytes: 100_000,
            ..Options::default()
        };
        refused(
            read_bytes("odm.tar.gz", &compressed, &expansion),
            "source_odm_limit_exceeded",
            "compressed TAR input expands beyond 100000 bytes",
        );
        let roomy = Options {
            max_expanded_bytes: archive.len() as u64,
            ..Options::default()
        };
        assert_eq!(
            read_bytes("odm.tar.gz", &compressed, &roomy)
                .unwrap()
                .rows(),
            3
        );
        let concatenated = [gzip(&archive[..1024]), gzip(&archive[1024..])].concat();
        assert_eq!(
            read_bytes("odm.tgz", &concatenated, &roomy).unwrap().rows(),
            3
        );
    }

    /// Seam: sparse plain and gzip TAR files over the source bound.
    ///
    /// A local archive streams from its file: only the members a pass
    /// reaches are read, so an archive extended past the ten-gigabyte source
    /// bound reads its one member. The sparse tail lies after the block that
    /// ends the archive, which no pass reads past: in a plain archive, more
    /// zero blocks; after a gzip stream, bytes that are never decoded. A
    /// reader that holds the file whole refuses both.
    #[cfg(unix)]
    #[test]
    fn local_archives_stream_without_a_source_bound() {
        for (name, gzipped) in [("odm.tar", false), ("odm.tar.gz", true)] {
            let archive = Tar::new().file("odm.xml", ODM_13.as_bytes()).finish();
            let scratch = Scratch::new("sparse-tar");
            let path = scratch.write(name, if gzipped { gzip(&archive) } else { archive });
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .unwrap()
                .set_len(yamaa_core::limits::Limits::SOURCE_BYTES as u64 + 1)
                .unwrap();
            // The bytes the member, its header and the archive's end take.
            let small = Options {
                max_expanded_bytes: (512 + ODM_13.len().next_multiple_of(512) + 1024) as u64,
                ..Options::default()
            };
            assert_eq!(read_with(&path, &small).unwrap().rows(), 3, "{name}");
        }
    }

    /// Seam: TAR archives and gzip streams built in the test and damaged at
    /// byte offsets.
    #[test]
    fn damaged_archives_are_refused() {
        let valid = Tar::new()
            .file("odm.xml", ODM_13.as_bytes())
            .file("notes.txt", b"notes")
            .finish();
        let mut checksum = valid.clone();
        let second = 512 + ODM_13.len().div_ceil(512) * 512;
        checksum[second + 148] ^= 1;
        refused(
            read_tar(&checksum, &Options::default()),
            "source_odm_archive_invalid",
            "invalid TAR header",
        );
        refused(
            read_tar(&valid[..second + 100], &Options::default()),
            "source_odm_archive_invalid",
            "truncated TAR header",
        );
        refused(
            read_tar(&valid[..700], &Options::default()),
            "source_odm_archive_invalid",
            "truncated TAR member data",
        );
        // A size within one block of u64::MAX cannot be padded to whole blocks.
        let oversized = Tar::new()
            .pax(&[("size", &u64::MAX.to_string())], "huge.bin", b"")
            .file("odm.xml", ODM_13.as_bytes())
            .finish();
        refused(
            read_tar(&oversized, &Options::default()),
            "source_odm_archive_invalid",
            "invalid TAR member size",
        );
        let selected_short = Tar::new().file("odm.xml", ODM_13.as_bytes()).unterminated();
        refused(
            read_tar(&selected_short[..900], &Options::default()),
            "source_odm_archive_invalid",
            "truncated TAR member data",
        );
        let dangling = Tar::new()
            .file("odm.xml", ODM_13.as_bytes())
            .raw(&Header::new("PaxHeaders/x", b'x', 0).encode())
            .finish();
        refused(
            read_tar(&dangling, &Options::default()),
            "source_odm_archive_invalid",
            "extension header has no member",
        );
        // An unknown compression method in the gzip header.
        let mut corrupt = gzip(&valid);
        corrupt[2] = 7;
        refused(
            read_bytes("odm.tar.gz", &corrupt, &Options::default()),
            "source_odm_archive_invalid",
            "cannot read ODM archive",
        );
        // Without an end-of-archive block the stream is read to its gzip
        // trailer, whose CRC must match the data.
        let unterminated = Tar::new().file("odm.xml", ODM_13.as_bytes()).unterminated();
        let mut crc = gzip(&unterminated);
        let end = crc.len();
        crc[end - 8] ^= 0xFF;
        refused(
            read_bytes("odm.tar.gz", &crc, &Options::default()),
            "source_odm_archive_invalid",
            "cannot read ODM archive",
        );
    }
}
