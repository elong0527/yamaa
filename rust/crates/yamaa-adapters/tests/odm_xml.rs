#![cfg(feature = "odm")]
//! ODM XML clinical-item contracts. Every case of the retained Python reader
//! suite is ported here, followed by refusals that suite did not cover.
#[allow(dead_code)]
mod support;

use support::odm_files::{cell, column, edit, gzip, ordinals, Header, Scratch, Tar};
use yamaa_adapters::{
    arrow_table::ArrowTable,
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

/// Assert a refusal's stable code and that its message contains `message`.
#[track_caller]
fn refused(result: Result<PublicTable, odm_items::Error>, code: &str, message: &str) {
    let error = result.expect_err("the input must be refused");
    assert_eq!(error.code(), code, "{error}");
    let text = error.to_string();
    assert!(
        text.contains(message),
        "{text:?} does not contain {message:?}"
    );
}

fn values(table: &PublicTable) -> Vec<Option<String>> {
    column(table, "Value")
}

fn some(values: &[&str]) -> Vec<Option<String>> {
    values
        .iter()
        .map(|value| Some((*value).to_owned()))
        .collect()
}

mod python_reader_suite {
    use super::*;

    #[test]
    fn odm13_preserves_values_and_enriches_exact_metadata() {
        let table = rows(ODM_13);
        assert_eq!(ordinals(&table), [1, 2, 3]);
        assert_eq!(
            values(&table),
            [Some("2026-01-02".into()), Some(String::new()), None]
        );
        assert_eq!(
            column(&table, "ValuePresent"),
            some(&["true", "true", "false"])
        );
        assert_eq!(column(&table, "IsNull"), some(&["false", "false", "true"]));
        assert_eq!(
            cell(&table, "StudySubjectID", 0).as_deref(),
            Some("DISPLAY-1")
        );
        assert_eq!(cell(&table, "EventName", 0).as_deref(), Some("Screening"));
        assert_eq!(cell(&table, "FormName", 0).as_deref(), Some("Demographics"));
        assert_eq!(
            cell(&table, "ItemGroupName", 0).as_deref(),
            Some("Subject details")
        );
        assert_eq!(
            cell(&table, "ItemName", 0).as_deref(),
            Some("Collection date")
        );
        assert_eq!(cell(&table, "ItemGroupRepeatKey", 0).as_deref(), Some("7"));
    }

    #[test]
    fn two_groups_supply_form_and_item_group_context() {
        let table = rows(ODM_20);
        assert_eq!(table.rows(), 2);
        assert_eq!(cell(&table, "StudySubjectID", 0), None);
        assert_eq!(cell(&table, "FormOID", 0).as_deref(), Some("FO.20"));
        assert_eq!(cell(&table, "FormRepeatKey", 0).as_deref(), Some("2"));
        assert_eq!(
            cell(&table, "FormName", 0).as_deref(),
            Some("Biomarker form")
        );
        assert_eq!(cell(&table, "ItemGroupOID", 0).as_deref(), Some("IG.20"));
        assert_eq!(cell(&table, "ItemGroupRepeatKey", 0).as_deref(), Some("3"));
        assert_eq!(
            cell(&table, "ItemGroupName", 0).as_deref(),
            Some("Biomarker group")
        );
        assert_eq!(cell(&table, "ItemName", 0).as_deref(), Some("Test name"));
        assert_eq!(cell(&table, "Value", 0).as_deref(), Some("PD-L1"));
        assert_eq!(cell(&table, "Value", 1).as_deref(), Some(""));
        assert_eq!(cell(&table, "ValuePresent", 1).as_deref(), Some("true"));
    }

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

    #[test]
    fn missing_exact_metadata_leaves_names_null() {
        let table = rows(&edit(
            ODM_20,
            r#"ClinicalData StudyOID="S.20" MetaDataVersionOID="M.20""#,
            r#"ClinicalData StudyOID="OTHER" MetaDataVersionOID="OTHER.M""#,
        ));
        assert_eq!(cell(&table, "StudyOID", 0).as_deref(), Some("OTHER"));
        assert_eq!(
            cell(&table, "MetaDataVersionOID", 0).as_deref(),
            Some("OTHER.M")
        );
        for name in ["EventName", "FormName", "ItemGroupName", "ItemName"] {
            assert_eq!(cell(&table, name, 0), None, "{name}");
        }
    }

    #[test]
    fn item_cannot_be_null_and_contain_a_value() {
        refused(
            read_document(&edit(
                ODM_13,
                r#"<ItemData ItemOID="I.NULL" IsNull="Yes"/>"#,
                r#"<ItemData ItemOID="I.NULL" IsNull="Yes" Value="unexpected"/>"#,
            )),
            "unsupported_value",
            "null and contains a value",
        );
    }

    #[test]
    fn comment_or_pi_inside_value_does_not_truncate_it() {
        for annotation in ["<!-- split -->", "<?pi data?>"] {
            let table = rows(&edit(
                ODM_20,
                r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>"#,
                &format!(
                    r#"<ItemData ItemOID="I.TEST"><Value>PD{annotation}-L1</Value></ItemData>"#
                ),
            ));
            assert_eq!(values(&table), some(&["PD-L1", ""]), "{annotation}");
        }
    }

    #[test]
    fn element_child_inside_value_is_rejected_not_truncated() {
        for value in [
            "<Value>PD<Sub>L1</Sub></Value>",
            r#"<Value>PD<x:B xmlns:x="urn:example">-MID-</x:B>L1</Value>"#,
            "<Value>PD<Value>inner</Value>-L1</Value>",
        ] {
            refused(
                read_document(&edit(
                    ODM_20,
                    r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>"#,
                    &format!(r#"<ItemData ItemOID="I.TEST">{value}</ItemData>"#),
                )),
                "unsupported_value",
                "Value contains child elements",
            );
        }
    }

    #[test]
    fn value_child_in_foreign_namespace_is_rejected_not_dropped() {
        for value in [
            r#"<x:Value xmlns:x="urn:example">PD-L1</x:Value>"#,
            r#"<Value xmlns="">PD-L1</Value>"#,
        ] {
            refused(
                read_document(&edit(
                    ODM_20,
                    r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>"#,
                    &format!(r#"<ItemData ItemOID="I.TEST">{value}</ItemData>"#),
                )),
                "unsupported_value",
                "Value child is in an unsupported namespace",
            );
        }
    }

    #[test]
    fn item_data_text_content_is_rejected_not_dropped() {
        for (document, original, replacement) in [
            (
                ODM_13,
                r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                r#"<ItemData ItemOID="I.DATE">2026-01-02</ItemData>"#,
            ),
            (
                ODM_20,
                r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>"#,
                r#"<ItemData ItemOID="I.TEST">PD-L1</ItemData>"#,
            ),
            (
                ODM_20,
                r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>"#,
                r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value>stray</ItemData>"#,
            ),
        ] {
            refused(
                read_document(&edit(document, original, replacement)),
                "unsupported_value",
                "contains text outside its value",
            );
        }
    }

    #[test]
    fn whitespace_around_value_child_is_not_text_content() {
        let table = rows(&edit(
            ODM_20,
            r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>"#,
            "<ItemData ItemOID=\"I.TEST\"> \t\r\n<Value>PD-L1</Value>\r\n\t </ItemData>",
        ));
        assert_eq!(values(&table), some(&["PD-L1", ""]));
    }

    const UNICODE_SPACES: [&str; 5] = ["\u{a0}", "\u{85}", "\u{2003}", "\u{2028}", "\u{3000}"];

    #[test]
    fn unicode_space_outside_value_is_rejected() {
        for text in UNICODE_SPACES {
            for (document, original, replacement) in [
                (
                    ODM_13,
                    r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                    format!(r#"<ItemData ItemOID="I.DATE">{text}</ItemData>"#),
                ),
                (
                    ODM_20,
                    r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>"#,
                    format!(r#"<ItemData ItemOID="I.TEST">{text}<Value>PD-L1</Value></ItemData>"#),
                ),
                (
                    ODM_20,
                    r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>"#,
                    format!(r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value>{text}</ItemData>"#),
                ),
            ] {
                refused(
                    read_document(&edit(document, original, &replacement)),
                    "unsupported_value",
                    "contains text outside its value",
                );
            }
        }
    }

    #[test]
    fn unicode_space_inside_value_is_preserved() {
        for text in UNICODE_SPACES {
            for (document, original, replacement) in [
                (
                    ODM_13,
                    r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                    format!(r#"<ItemData ItemOID="I.DATE" Value="{text}"/>"#),
                ),
                (
                    ODM_20,
                    r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>"#,
                    format!(r#"<ItemData ItemOID="I.TEST"><Value>{text}</Value></ItemData>"#),
                ),
            ] {
                let table = rows(&edit(document, original, &replacement));
                assert_eq!(cell(&table, "Value", 0).as_deref(), Some(text));
                assert_eq!(cell(&table, "ValuePresent", 0).as_deref(), Some("true"));
            }
        }
    }

    #[test]
    fn namespaced_value_attribute_is_rejected_not_dropped() {
        refused(
            read_document(&edit(
                ODM_13,
                r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                r#"<ItemData ItemOID="I.DATE" oc:Value="2026-01-02"/>"#,
            )),
            "unsupported_value",
            "Value attribute is in an unsupported namespace",
        );
    }

    #[test]
    fn comment_or_pi_inside_item_data_is_ignored() {
        for annotation in ["<!-- c -->", "<?pi data?>"] {
            let table = rows(&edit(
                ODM_13,
                r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                &format!(
                    r#"<ItemData ItemOID="I.DATE" Value="2026-01-02">{annotation}</ItemData>"#
                ),
            ));
            assert_eq!(
                values(&table),
                [Some("2026-01-02".into()), Some(String::new()), None]
            );
        }
    }

    #[test]
    fn odm_version_with_junk_suffix_is_rejected() {
        for (document, bad) in [
            (ODM_13, "1.3junk"),
            (ODM_13, "1.31"),
            (ODM_13, "1.3.junk"),
            (ODM_13, "1.3."),
            (ODM_20, "2.0junk"),
            (ODM_20, "2.01"),
            (ODM_20, "2.0.x"),
            (ODM_20, "2.0.1a"),
        ] {
            let base = if document == ODM_13 { "1.3" } else { "2.0" };
            refused(
                read_document(&edit(
                    document,
                    &format!(r#"ODMVersion="{base}""#),
                    &format!(r#"ODMVersion="{bad}""#),
                )),
                "unsupported_document",
                "conflicts with namespace",
            );
        }
    }

    #[test]
    fn odm_dotted_sub_version_is_accepted() {
        for (document, good) in [
            (ODM_13, "1.3.1"),
            (ODM_13, "1.3.2"),
            (ODM_20, "2.0.1"),
            (ODM_20, "2.0.1.2"),
        ] {
            let base = if document == ODM_13 { "1.3" } else { "2.0" };
            let table = rows(&edit(
                document,
                &format!(r#"ODMVersion="{base}""#),
                &format!(r#"ODMVersion="{good}""#),
            ));
            assert!(table.rows() > 0);
            assert!(column(&table, "ODMVersion")
                .iter()
                .all(|version| version.as_deref() == Some(good)));
        }
    }

    #[test]
    fn nested_clinical_data_is_rejected() {
        let document = edit(
            &edit(
                ODM_13,
                r#"<ClinicalData StudyOID="S.13" MetaDataVersionOID="M.13">"#,
                r#"<ClinicalData StudyOID="S.13" MetaDataVersionOID="M.13"><ClinicalData StudyOID="EVIL" MetaDataVersionOID="EVIL.M">"#,
            ),
            "</ClinicalData>\n</ODM>",
            "</ClinicalData></ClinicalData>\n</ODM>",
        );
        refused(
            read_document(&document),
            "unsupported_layout",
            "nested ClinicalData is not supported",
        );
    }

    #[test]
    fn nested_study_is_rejected() {
        let document = edit(
            &edit(
                ODM_13,
                r#"<Study OID="S.13">"#,
                r#"<Study OID="S.13"><Study OID="EVIL">"#,
            ),
            "</Study>\n  <ClinicalData",
            "</Study></Study>\n  <ClinicalData",
        );
        refused(
            read_document(&document),
            "unsupported_layout",
            "nested Study is not supported",
        );
    }

    #[test]
    fn nested_metadata_version_is_rejected() {
        let document = edit(
            &edit(
                ODM_13,
                r#"<MetaDataVersion OID="M.13" Name="Metadata">"#,
                r#"<MetaDataVersion OID="M.13" Name="Metadata"><MetaDataVersion OID="EVIL.M">"#,
            ),
            "</MetaDataVersion>",
            "</MetaDataVersion></MetaDataVersion>",
        );
        refused(
            read_document(&document),
            "unsupported_layout",
            "nested MetaDataVersion is not supported",
        );
    }

    #[test]
    fn nested_study_inside_clinical_data_is_rejected() {
        refused(
            read_document(&edit(
                ODM_13,
                r#"<Study OID="S.13">"#,
                r#"<Study OID="S.13"><ClinicalData StudyOID="S.13" MetaDataVersionOID="M.13"><Study OID="EVIL"></Study></ClinicalData>"#,
            )),
            "unsupported_layout",
            "nested Study is not supported",
        );
    }

    #[test]
    fn nested_metadata_version_inside_clinical_data_is_rejected() {
        refused(
            read_document(&edit(
                ODM_13,
                r#"<MetaDataVersion OID="M.13" Name="Metadata">"#,
                r#"<MetaDataVersion OID="M.13" Name="Metadata"><ClinicalData StudyOID="S.13" MetaDataVersionOID="M.13"><MetaDataVersion OID="EVIL.M"></MetaDataVersion></ClinicalData>"#,
            )),
            "unsupported_layout",
            "nested MetaDataVersion is not supported",
        );
    }

    #[test]
    fn sequential_clinical_data_blocks_are_read() {
        let table = rows(&edit(
            ODM_13,
            "</ClinicalData>\n</ODM>",
            concat!(
                "</ClinicalData>",
                r#"<ClinicalData StudyOID="S.13" MetaDataVersionOID="M.13">"#,
                r#"<SubjectData SubjectKey="SUBJECT-2">"#,
                r#"<StudyEventData StudyEventOID="E.13">"#,
                r#"<FormData FormOID="F.13">"#,
                r#"<ItemGroupData ItemGroupOID="G.13">"#,
                r#"<ItemData ItemOID="I.DATE" Value="2026-02-02"/>"#,
                "</ItemGroupData></FormData></StudyEventData></SubjectData>",
                "</ClinicalData>\n</ODM>",
            ),
        ));
        assert_eq!(
            values(&table),
            [
                Some("2026-01-02".into()),
                Some(String::new()),
                None,
                Some("2026-02-02".into())
            ]
        );
        assert_eq!(cell(&table, "SubjectKey", 3).as_deref(), Some("SUBJECT-2"));
    }

    #[test]
    fn unsupported_group_depth_is_rejected() {
        let document = edit(
            &edit(
                ODM_20,
                r#"<ItemGroupData ItemGroupOID="IG.20" ItemGroupRepeatKey="3">"#,
                r#"<ItemGroupData ItemGroupOID="IG.20" ItemGroupRepeatKey="3"><ItemGroupData ItemGroupOID="EXTRA">"#,
            ),
            "</ItemGroupData>\n        </ItemGroupData>",
            "</ItemGroupData></ItemGroupData>\n        </ItemGroupData>",
        );
        refused(
            read_document(&document),
            "unsupported_layout",
            "requires FormData plus one",
        );
    }

    #[test]
    fn nested_item_data_is_rejected() {
        refused(
            read_document(&edit(
                ODM_13,
                r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                r#"<ItemData ItemOID="I.DATE" Value="outer"><ItemData ItemOID="I.EMPTY" Value="inner"/></ItemData>"#,
            )),
            "unsupported_layout",
            "nested ItemData is not supported",
        );
    }

    #[test]
    fn item_data_nested_below_a_group_inside_item_data_is_rejected() {
        refused(
            read_document(&edit(
                ODM_13,
                r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                r#"<ItemData ItemOID="I.DATE" Value="outer"><ItemGroupData ItemGroupOID="G.13"><ItemData ItemOID="I.EMPTY" Value="inner"/></ItemGroupData></ItemData>"#,
            )),
            "unsupported_layout",
            "nested ItemData is not supported",
        );
    }

    #[test]
    fn container_inside_item_data_is_rejected_not_leaked() {
        for (container, message) in [
            (
                r#"<ItemGroupData ItemGroupOID="LEAKED"/>"#,
                "ItemGroupData inside ItemData",
            ),
            (
                r#"<FormData FormOID="EVIL.F"/>"#,
                "FormData inside ItemData",
            ),
        ] {
            let document = edit(
                &edit(
                    &edit(ODM_13, r#"<FormData FormOID="F.13">"#, ""),
                    "</FormData>",
                    "",
                ),
                r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                &format!(r#"<ItemData ItemOID="I.DATE" Value="2026-01-02">{container}</ItemData>"#),
            );
            refused(read_document(&document), "unsupported_layout", message);
        }
    }

    #[test]
    fn subject_data_inside_item_data_is_rejected() {
        refused(
            read_document(&edit(
                ODM_13,
                r#"<SubjectData SubjectKey="SUBJECT-1""#,
                r#"<ItemData ItemOID="I.DATE" Value="rogue"><SubjectData SubjectKey="ROGUE"/></ItemData><SubjectData SubjectKey="SUBJECT-1""#,
            )),
            "unsupported_layout",
            "SubjectData inside ItemData",
        );
    }

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
            "archive_member",
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
            "limit",
            "more than 3 members",
        );
    }

    #[test]
    fn public_readers_reject_invalid_archive_member_bound() {
        let options = Options {
            max_archive_members: 0,
            ..Options::default()
        };
        refused(
            read_bytes("odm20.xml", ODM_20.as_bytes(), &options),
            "invalid_option",
            "max_archive_members must be at least 1",
        );
    }

    #[test]
    fn archive_rejects_non_regular_xml_member() {
        let archive = gzip(
            &Tar::new()
                .special("study/odm.xml", b'2', "elsewhere.xml")
                .finish(),
        );
        refused(
            read_bytes("unsafe.tar.gz", &archive, &Options::default()),
            "archive_member",
            "not a regular file",
        );
    }

    #[test]
    fn truncated_compressed_archive_has_helper_error() {
        let archive = gzip(&Tar::new().file("study/odm.xml", ODM_20.as_bytes()).finish());
        for cut in [30, 250] {
            refused(
                read_bytes("truncated.tar.gz", &archive[..cut], &Options::default()),
                "invalid_archive",
                "cannot read ODM archive",
            );
        }
    }

    #[test]
    fn malformed_xml_has_helper_error() {
        refused(
            read_document(r#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">"#),
            "malformed_xml",
            "malformed ODM XML",
        );
    }
}

mod table {
    use super::*;

    #[test]
    fn documents_without_items_produce_the_complete_schema() {
        let document = edit(
            ODM_13,
            &ODM_13[ODM_13.find("  <ClinicalData").unwrap()..ODM_13.find("</ODM>").unwrap()],
            "",
        );
        let table = rows(&document);
        assert_eq!(table.rows(), 0);
        assert_eq!(table.columns().len(), 30);
        assert_eq!(table.batches().len(), 1);
    }

    #[test]
    fn absent_null_and_empty_values_stay_distinct() {
        let table = rows(ODM_13);
        let states = (0..3)
            .map(|row| {
                (
                    cell(&table, "Value", row),
                    cell(&table, "ValuePresent", row).unwrap(),
                    cell(&table, "IsNull", row).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            states,
            [
                (Some("2026-01-02".into()), "true".into(), "false".into()),
                (Some(String::new()), "true".into(), "false".into()),
                (None, "false".into(), "true".into()),
            ]
        );
        let absent = rows(&edit(
            ODM_13,
            r#"<ItemData ItemOID="I.NULL" IsNull="Yes"/>"#,
            r#"<ItemData ItemOID="I.NULL"/>"#,
        ));
        assert_eq!(
            (
                cell(&absent, "Value", 2),
                cell(&absent, "ValuePresent", 2).as_deref(),
                cell(&absent, "IsNull", 2).as_deref()
            ),
            (None, Some("false"), Some("false"))
        );
    }

    #[test]
    fn context_columns_project_every_level() {
        let table = rows(ODM_13);
        for (name, expected) in [
            ("ODMVersion", Some("1.3")),
            ("FileOID", Some("FILE.13")),
            ("StudyOID", Some("S.13")),
            ("MetaDataVersionOID", Some("M.13")),
            ("SubjectKey", Some("SUBJECT-1")),
            ("SubjectStatus", Some("Available")),
            ("StudyEventOID", Some("E.13")),
            ("StudyEventRepeatKey", None),
            ("StartDate", Some("2026-01-02")),
            ("EventStatus", None),
            ("EventWorkflowStatus", None),
            ("FormOID", Some("F.13")),
            ("FormRepeatKey", None),
            ("FormLayoutOID", None),
            ("FormStatus", None),
            ("FormWorkflowStatus", None),
            ("OpenQueries", None),
            ("ItemGroupOID", Some("G.13")),
            ("TransactionType", Some("Insert")),
            ("ItemOID", Some("I.DATE")),
        ] {
            assert_eq!(cell(&table, name, 0).as_deref(), expected, "{name}");
        }
    }

    #[test]
    fn the_table_is_an_ordinary_engine_input() {
        let table = rows(ODM_13);
        let input = ArrowTable::try_new(odm_items::schema(), table.batches().to_vec()).unwrap();
        assert_eq!(input.row_count(), 3);
        assert_eq!(input.cell(0, 26).unwrap(), ValueRef::Str("2026-01-02"));
        assert_eq!(input.cell(2, 26).unwrap(), ValueRef::Missing);
        assert_eq!(input.cell(2, 29).unwrap(), ValueRef::Int(3));
        let fields = yamaa_core::odm_source::FIELDS;
        for field in fields {
            assert!(odm_items::COLUMNS.contains(&field), "{field}");
        }
    }

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

    #[test]
    fn line_ends_and_attribute_whitespace_normalize_as_xml_requires() {
        let table = rows(
            &edit(
                ODM_20,
                r#"<ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>"#,
                "<ItemData ItemOID=\"I.TEST\"><Value>a\r\nb\rc&#13;d<![CDATA[e\r\nf]]></Value></ItemData>",
            )
            .replace(
                r#"<ItemData ItemOID="I.RESULT"><Value/></ItemData>"#,
                "<ItemData ItemOID=\"I.RESULT\" Value=\"x\ty\r\nz&#9;w&#10;v\"/>",
            ),
        );
        assert_eq!(values(&table), some(&["a\nb\nc\rde\nf", "x y z\tw\nv"]));
    }

    #[test]
    fn missing_odm_version_and_file_oid_default_from_the_namespace() {
        let table = rows(&edit(
            ODM_20,
            "\n     ODMVersion=\"2.0\" FileOID=\"FILE.20\"",
            "",
        ));
        assert_eq!(column(&table, "ODMVersion"), some(&["2.0", "2.0"]));
        assert_eq!(column(&table, "FileOID"), [None, None]);
        let empty = rows(&edit(ODM_20, r#"ODMVersion="2.0""#, r#"ODMVersion="""#));
        assert_eq!(cell(&empty, "ODMVersion", 0).as_deref(), Some("2.0"));
    }
}

mod metadata {
    use super::*;

    #[test]
    fn conflicting_definition_names_are_refused() {
        for (kind, original, replacement) in [
            (
                "event",
                r#"<StudyEventDef OID="E.13" Name="Screening"/>"#,
                r#"<StudyEventDef OID="E.13" Name="Screening"/><StudyEventDef OID="E.13" Name="Baseline"/>"#,
            ),
            (
                "form",
                r#"<FormDef OID="F.13" Name="Demographics"/>"#,
                r#"<FormDef OID="F.13" Name="Demographics"/><FormDef OID="F.13"/>"#,
            ),
            (
                "group",
                r#"<ItemGroupDef OID="G.13" Name="Subject details"/>"#,
                r#"<ItemGroupDef OID="G.13" Name="Subject details"/><ItemGroupDef OID="G.13" Name="Other"/>"#,
            ),
            (
                "item",
                r#"<ItemDef OID="I.DATE" Name="Collection date" DataType="text"/>"#,
                r#"<ItemDef OID="I.DATE" Name="Collection date" DataType="text"/><ItemDef OID="I.DATE" Name="Visit date"/>"#,
            ),
        ] {
            refused(
                read_document(&edit(ODM_13, original, replacement)),
                "metadata_conflict",
                &format!("conflicting {kind} definition for S.13/M.13/"),
            );
        }
    }

    #[test]
    fn repeated_identical_definitions_and_other_versions_are_admitted() {
        let table = rows(&edit(
            ODM_13,
            "  </Study>",
            concat!(
                "  </Study>",
                r#"<Study OID="S.13"><MetaDataVersion OID="M.OTHER">"#,
                r#"<ItemDef OID="I.DATE" Name="Other version"/>"#,
                r#"</MetaDataVersion><MetaDataVersion OID="M.13">"#,
                r#"<ItemDef OID="I.DATE" Name="Collection date"/>"#,
                "</MetaDataVersion></Study>",
            ),
        ));
        assert_eq!(
            cell(&table, "ItemName", 0).as_deref(),
            Some("Collection date")
        );
    }

    #[test]
    fn only_definitions_before_an_item_and_outside_clinical_data_apply() {
        let late = rows(&edit(
            &edit(
                ODM_13,
                r#"<ItemDef OID="I.DATE" Name="Collection date" DataType="text"/>"#,
                "",
            ),
            "</ClinicalData>\n</ODM>",
            concat!(
                "</ClinicalData>",
                r#"<Study OID="S.13"><MetaDataVersion OID="M.13">"#,
                r#"<ItemDef OID="I.DATE" Name="Late name"/>"#,
                "</MetaDataVersion></Study>\n</ODM>",
            ),
        ));
        assert_eq!(cell(&late, "ItemName", 0), None);
        let inside = rows(&edit(
            &edit(
                ODM_13,
                r#"<ItemDef OID="I.DATE" Name="Collection date" DataType="text"/>"#,
                "",
            ),
            r#"<FormData FormOID="F.13">"#,
            r#"<ItemDef OID="I.DATE" Name="Inside clinical data"/><FormData FormOID="F.13">"#,
        ));
        assert_eq!(cell(&inside, "ItemName", 0), None);
    }

    #[test]
    fn namespaced_names_on_data_elements_override_definitions() {
        let table = rows(&edit(
            &edit(
                &edit(
                    &edit(
                        ODM_13,
                        r#"<StudyEventData StudyEventOID="E.13""#,
                        r#"<StudyEventData StudyEventOID="E.13" oc:EventName="Vendor event" oc:Status="done" oc:WorkflowStatus="locked""#,
                    ),
                    r#"<FormData FormOID="F.13">"#,
                    r#"<FormData FormOID="F.13" FormRepeatKey="4" oc:FormName="" oc:FormLayoutOID="L1" oc:Status="s" oc:WorkflowStatus="w" oc:OpenQueries="2">"#,
                ),
                r#"<ItemGroupData ItemGroupOID="G.13""#,
                r#"<ItemGroupData ItemGroupOID="G.13" oc:ItemGroupName="Vendor group""#,
            ),
            r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
            r#"<ItemData ItemOID="I.DATE" Value="2026-01-02" oc:ItemName="Vendor item"/>"#,
        ));
        for (name, expected) in [
            ("EventName", Some("Vendor event")),
            ("EventStatus", Some("done")),
            ("EventWorkflowStatus", Some("locked")),
            ("FormRepeatKey", Some("4")),
            ("FormName", Some("")),
            ("FormLayoutOID", Some("L1")),
            ("FormStatus", Some("s")),
            ("FormWorkflowStatus", Some("w")),
            ("OpenQueries", Some("2")),
            ("ItemGroupName", Some("Vendor group")),
            ("ItemName", Some("Vendor item")),
        ] {
            assert_eq!(cell(&table, name, 0).as_deref(), expected, "{name}");
        }
        assert_eq!(
            cell(&table, "ItemName", 1).as_deref(),
            Some("Collected empty")
        );
    }

    #[test]
    fn ambiguous_namespaced_attributes_are_refused() {
        refused(
            read_document(&edit(
                ODM_13,
                r#"oc:Status="Available""#,
                r#"oc:Status="Available" xmlns:v="urn:vendor" v:Status="Removed""#,
            )),
            "ambiguous_attribute",
            r#"multiple namespaced attributes have local name "Status""#,
        );
        refused(
            read_document(&edit(
                ODM_13,
                r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                r#"<ItemData ItemOID="I.DATE" Value="2026-01-02" oc:ItemName="a" xml:ItemName="b"/>"#,
            )),
            "ambiguous_attribute",
            "ItemName",
        );
    }

    #[test]
    fn definitions_need_their_study_and_metadata_version() {
        refused(
            read_document(&edit(
                ODM_13,
                "  <Study OID=\"S.13\">\n    <MetaDataVersion",
                "  <MetaDataVersion OID=\"X\"/><Study OID=\"S.13\">\n    <MetaDataVersion",
            )),
            "unsupported_layout",
            "MetaDataVersion appears outside Study",
        );
        refused(
            read_document(&edit(
                ODM_13,
                "  <Study OID=\"S.13\">",
                "  <ItemDef OID=\"X\"/><Study OID=\"S.13\">",
            )),
            "unsupported_layout",
            "ItemDef appears outside MetaDataVersion",
        );
    }
}

mod layout {
    use super::*;

    #[test]
    fn every_is_null_spelling_is_decoded_and_others_are_refused() {
        for (spelling, expected) in [
            ("Yes", "true"),
            ("yes", "true"),
            ("TRUE", "true"),
            ("1", "true"),
            ("No", "false"),
            ("no", "false"),
            ("FaLsE", "false"),
            ("0", "false"),
        ] {
            let table = rows(&edit(
                ODM_13,
                r#"IsNull="Yes""#,
                &format!(r#"IsNull="{spelling}""#),
            ));
            assert_eq!(
                cell(&table, "IsNull", 2).as_deref(),
                Some(expected),
                "{spelling}"
            );
            assert_eq!(cell(&table, "Value", 2), None);
        }
        for spelling in ["maybe", "", " yes", "ye\u{17f}", "fal\u{17f}e"] {
            refused(
                read_document(&edit(
                    ODM_13,
                    r#"IsNull="Yes""#,
                    &format!(r#"IsNull="{spelling}""#),
                )),
                "invalid_is_null",
                "unsupported IsNull value",
            );
        }
        let explicit_false = rows(&edit(
            ODM_13,
            r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
            r#"<ItemData ItemOID="I.DATE" Value="2026-01-02" IsNull="No"/>"#,
        ));
        assert_eq!(
            cell(&explicit_false, "Value", 0).as_deref(),
            Some("2026-01-02")
        );
    }

    #[test]
    fn typed_item_data_elements_are_refused_inside_clinical_data() {
        for element in [
            "ItemDataString",
            "ItemDataInteger",
            "ItemDataDate",
            "ItemDataAny",
        ] {
            refused(
                read_document(&edit(
                    ODM_13,
                    r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                    &format!(r#"<{element} ItemOID="I.DATE">2026-01-02</{element}>"#),
                )),
                "unsupported_layout",
                &format!("typed ItemData element {element} is not supported"),
            );
        }
        let outside = rows(&edit(
            ODM_13,
            "  <ClinicalData",
            "  <ItemDataString ItemOID=\"I.X\">ignored</ItemDataString>\n  <ClinicalData",
        ));
        assert_eq!(outside.rows(), 3);
        // Only the ODM 1.3 typed elements are refused; other names stay
        // ignored like any undefined ODM element.
        let undefined = rows(&edit(
            ODM_13,
            r#"<ItemData ItemOID="I.EMPTY" Value=""/>"#,
            r#"<ItemData ItemOID="I.EMPTY" Value=""/><ItemDataX ItemOID="I.X">v</ItemDataX>"#,
        ));
        assert_eq!(undefined.rows(), 3);
        let version_two = rows(&edit(
            ODM_20,
            r#"<ItemData ItemOID="I.RESULT"><Value/></ItemData>"#,
            r#"<ItemData ItemOID="I.RESULT"><Value/></ItemData><ItemDataString ItemOID="I.X">v</ItemDataString>"#,
        ));
        assert_eq!(version_two.rows(), 2);
    }

    #[test]
    fn item_data_outside_clinical_data_is_ignored() {
        let table = rows(&edit(
            ODM_13,
            "    </MetaDataVersion>",
            "      <ItemData ItemOID=\"I.META\" Value=\"ignored\">text<x:y xmlns:x=\"urn:x\"/></ItemData>\n    </MetaDataVersion>",
        ));
        assert_eq!(
            column(&table, "ItemOID"),
            some(&["I.DATE", "I.EMPTY", "I.NULL"])
        );
    }

    #[test]
    fn foreign_elements_are_ignored_but_their_odm_descendants_count() {
        let table = rows(&edit(
            &edit(
                ODM_13,
                r#"<ItemData ItemOID="I.DATE" Value="2026-01-02"/>"#,
                r#"<oc:Wrapper><ItemData ItemOID="I.DATE" Value="2026-01-02"><oc:Note>kept aside</oc:Note></ItemData></oc:Wrapper>"#,
            ),
            "  </ClinicalData>",
            "  <oc:Audit><oc:Entry By=\"x\"/></oc:Audit></ClinicalData>",
        ));
        assert_eq!(
            values(&table),
            [Some("2026-01-02".into()), Some(String::new()), None]
        );
    }

    #[test]
    fn containers_inside_foreign_item_data_elements_close_normally() {
        // The Python reader matched an ItemData parent by local name alone,
        // so a group inside a foreign `oc:ItemData` never closed and leaked
        // into every later item. Foreign elements stay transparent here.
        let table = rows(&edit(
            ODM_13,
            "        <FormData FormOID=\"F.13\">",
            "        <oc:ItemData><ItemGroupData ItemGroupOID=\"LEAK\"/></oc:ItemData>\n        <FormData FormOID=\"F.13\">",
        ));
        assert_eq!(
            column(&table, "ItemGroupOID"),
            some(&["G.13", "G.13", "G.13"])
        );
    }

    #[test]
    fn items_need_subject_and_event_context() {
        refused(
            read_document(&edit(
                ODM_13,
                "<SubjectData SubjectKey=\"SUBJECT-1\" oc:StudySubjectID=\"DISPLAY-1\"\n                 oc:Status=\"Available\">",
                "<ItemGroupData ItemGroupOID=\"X\"><ItemGroupData ItemGroupOID=\"Y\"><ItemData ItemOID=\"I\" Value=\"v\"/></ItemGroupData></ItemGroupData><SubjectData SubjectKey=\"SUBJECT-1\">",
            )),
            "unsupported_layout",
            "ItemData requires subject and event context",
        );
    }

    #[test]
    fn required_attributes_must_be_present_and_non_empty() {
        for (original, replacement, message) in [
            (
                r#"ItemData ItemOID="I.DATE""#,
                r#"ItemData ItemOID="""#,
                "ItemData requires non-empty @ItemOID",
            ),
            (
                r#"ClinicalData StudyOID="S.13""#,
                "ClinicalData",
                "ClinicalData requires non-empty @StudyOID",
            ),
            (
                r#"SubjectData SubjectKey="SUBJECT-1""#,
                "SubjectData",
                "SubjectData requires non-empty @SubjectKey",
            ),
            (
                r#"StudyEventData StudyEventOID="E.13""#,
                "StudyEventData",
                "StudyEventData requires non-empty @StudyEventOID",
            ),
            (
                r#"FormData FormOID="F.13""#,
                r#"FormData FormOID="""#,
                "FormData requires non-empty @FormOID",
            ),
            (
                r#"ItemGroupData ItemGroupOID="G.13""#,
                "ItemGroupData",
                "ItemGroupData requires non-empty @ItemGroupOID",
            ),
            (
                r#"Study OID="S.13""#,
                "Study",
                "Study requires non-empty @OID",
            ),
            (
                r#"ItemDef OID="I.NULL""#,
                "ItemDef",
                "ItemDef requires non-empty @OID",
            ),
        ] {
            refused(
                read_document(&edit(ODM_13, original, replacement)),
                "missing_attribute",
                message,
            );
        }
    }

    #[test]
    fn root_namespace_and_nesting_are_checked() {
        refused(
            read_document(r#"<Study xmlns="http://www.cdisc.org/ns/odm/v1.3"/>"#),
            "unsupported_document",
            "document root must be ODM",
        );
        refused(
            read_document("<ODM/>"),
            "unsupported_document",
            "unsupported ODM namespace (none)",
        );
        refused(
            read_document(r#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.2"/>"#),
            "unsupported_document",
            r#"unsupported ODM namespace "http://www.cdisc.org/ns/odm/v1.2""#,
        );
        refused(
            read_document(&edit(
                ODM_13,
                "  <Study OID=\"S.13\">",
                "  <ODM/><Study OID=\"S.13\">",
            )),
            "unsupported_document",
            "document contains a nested ODM element",
        );
        refused(
            read_document(&edit(ODM_20, r#"ODMVersion="2.0""#, r#"ODMVersion="1.3""#)),
            "unsupported_document",
            r#"ODMVersion "1.3" conflicts with namespace"#,
        );
    }
}

mod xml_strictness {
    use super::*;

    fn wrap(body: &str) -> String {
        format!(r#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">{body}</ODM>"#)
    }

    #[test]
    fn doctype_and_entities_are_refused() {
        refused(
            read_document(&format!("<!DOCTYPE ODM [<!ENTITY e \"x\">]>\n{}", wrap(""))),
            "unsupported_xml",
            "DOCTYPE declarations are not supported",
        );
        let scratch = Scratch::new("external");
        let secret = scratch.write("secret.txt", "secret");
        refused(
            read_document(&format!(
                "<!DOCTYPE ODM [<!ENTITY e SYSTEM \"file://{}\">]>{}",
                secret.display(),
                wrap("&e;")
            )),
            "unsupported_xml",
            "DOCTYPE",
        );
        refused(
            read_document(&wrap("&e;")),
            "malformed_xml",
            "entity &e; is not defined",
        );
        refused(
            read_document(r#"<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3" FileOID="&e;"/>"#),
            "malformed_xml",
            "entity &e; is not defined",
        );
    }

    #[test]
    fn well_formedness_follows_xml_1_0() {
        for (document, message) in [
            (
                format!("{}<ODM/>", wrap("")),
                "extra content after the root element",
            ),
            (
                format!("{} text", wrap("")),
                "content is not allowed outside the root element",
            ),
            (
                format!("x{}", wrap("")),
                "content is not allowed outside the root element",
            ),
            (
                format!(" {}", "<?xml version=\"1.0\"?><ODM/>"),
                "XML declaration allowed only at the start",
            ),
            (wrap("<a></b>"), "expected `</a>`"),
            (wrap("<a b=\"1\" b=\"2\"/>"), "attribute b is repeated"),
            (
                wrap("<a b=\"1\"c=\"2\"/>"),
                "whitespace is required between attributes",
            ),
            (wrap("<a b=1/>"), "value must be quoted"),
            (wrap("<a b=\"<\"/>"), "unescaped '<'"),
            (wrap("]]>"), "sequence ']]>' is not allowed"),
            (wrap("&#1;"), "not a valid XML character"),
            (wrap("&#xD800;"), "not a valid XML character"),
            (wrap("&#X41;"), "malformed character reference"),
            (wrap("&#x;"), "malformed character reference"),
            (wrap("&#99999999999;"), "not a valid XML character"),
            (wrap("&#x110000;"), "not a valid XML character"),
            (wrap("&a b;"), "malformed reference"),
            (wrap("& "), "reference"),
            (wrap("<!-- a -- b -->"), "`--`"),
            (wrap("<?XML x?>"), "target xml is reserved"),
            (wrap("<?p:i x?>"), "invalid processing instruction target"),
            (wrap("<1a/>"), "invalid element name"),
            (wrap("<a:b:c xmlns:a=\"urn:a\"/>"), "invalid element name"),
            (String::new(), "document has no root element"),
            (
                "<?xml version=\"1.0\"?>".into(),
                "document has no root element",
            ),
            (
                "<?xml version=\"2.0\"?><ODM/>".into(),
                "XML version must be 1.x",
            ),
            (
                "<?xml version=\"1.0\" standalone=\"maybe\"?><ODM/>".into(),
                "standalone must be yes or no",
            ),
        ] {
            refused(read_document(&document), "malformed_xml", message);
        }
    }

    #[test]
    fn namespaces_follow_namespaces_1_0() {
        for (document, message) in [
            (wrap("<p:a/>"), "namespace prefix p is not defined"),
            (wrap("<a p:b=\"1\"/>"), "namespace prefix p is not defined"),
            (
                wrap("<a xmlns:p=\"\"/>"),
                "namespace prefix p cannot be undeclared",
            ),
            (
                wrap("<a xmlns:p=\"urn:u\" xmlns:q=\"urn:u\" p:x=\"1\" q:x=\"2\"/>"),
                "attribute x in namespace urn:u is repeated",
            ),
            (
                wrap("<a xmlns:xml=\"urn:other\"/>"),
                "the xml prefix cannot be bound",
            ),
            (
                wrap("<a xmlns:xmlns=\"urn:other\"/>"),
                "the xmlns prefix cannot be declared",
            ),
            (
                wrap("<a xmlns:p=\"http://www.w3.org/XML/1998/namespace\"/>"),
                "cannot be bound",
            ),
            (wrap("<a xmlns:p=\"http://x/a b\"/>"), "is not a valid URI"),
            (wrap("<a xmlns=\"http://x/\u{e9}\"/>"), "is not a valid URI"),
            (
                wrap("<a xmlns:p=\"https://w://x/ns\"/>"),
                "is not a valid URI",
            ),
            (
                wrap("<a xmlns:p=\"http://x:99999999999/\"/>"),
                "is not a valid URI",
            ),
            (wrap("<a xmlns:p=\"1abc:x\"/>"), "is not a valid URI"),
            (wrap("<a xmlns:p=\"http://x/%zz\"/>"), "is not a valid URI"),
            (
                wrap("<a xmlns:p=\"http://x/a#b#c\"/>"),
                "is not a valid URI",
            ),
            (wrap("<xmlns:a/>"), "reserved prefix xmlns"),
        ] {
            refused(read_document(&document), "malformed_xml", message);
        }
        let declared = rows(&edit(
            ODM_13,
            r#"xmlns="http://www.cdisc.org/ns/odm/v1.3""#,
            r#"xmlns="http://www.cdisc.org/ns/odm/v1&#46;3" xml:lang="en""#,
        ));
        assert_eq!(declared.rows(), 3);
        for uri in [
            "urn:example",
            "relative",
            "a:b:c",
            "http://[::1]:8080/x?q=1#f",
            "http://user:pw@host.example:80/a/%41",
            "#",
        ] {
            let document = edit(
                ODM_13,
                r#"xmlns:oc="https://www.openclinica.com/ns/odm_ext_v130/v3.1""#,
                &format!(r#"xmlns:oc="{uri}""#),
            );
            assert_eq!(rows(&document).rows(), 3, "{uri}");
        }
        let prefixed = ODM_20
            .replace(
                r#"xmlns="http://www.cdisc.org/ns/odm/v2.0""#,
                r#"xmlns:odm="http://www.cdisc.org/ns/odm/v2.0""#,
            )
            .replace('<', "<odm:")
            .replace("<odm:/", "</odm:")
            .replace("<odm:?xml", "<?xml");
        let prefixed = rows(&prefixed);
        assert_eq!(values(&prefixed), some(&["PD-L1", ""]));
    }

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
            "limit",
            "XML namespace declarations in scope exceed 4096",
        );
    }

    #[test]
    fn characters_and_encodings_are_utf8_xml_only() {
        let utf16 = wrap("")
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        for (bytes, code, message) in [
            (
                wrap("\u{1}").into_bytes(),
                "malformed_xml",
                "invalid XML character U+0001",
            ),
            (
                wrap("\u{fffe}").into_bytes(),
                "malformed_xml",
                "invalid XML character U+FFFE",
            ),
            (
                wrap("\u{ffff}").into_bytes(),
                "malformed_xml",
                "invalid XML character U+FFFF",
            ),
            (
                wrap("\0").into_bytes(),
                "malformed_xml",
                "invalid XML character U+0000",
            ),
            (
                format!(
                    "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>{}",
                    wrap("")
                )
                .into_bytes(),
                "unsupported_xml",
                "must be encoded as UTF-8, not ISO-8859-1",
            ),
            (
                [&[0xFF, 0xFE][..], &utf16].concat(),
                "unsupported_xml",
                "not UTF-16",
            ),
            (utf16.clone(), "unsupported_xml", "not UTF-16"),
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
            "malformed_xml",
            "invalid UTF-8",
        );
        // An invalid byte in the first token is reported at its source
        // offset, past a byte order mark too.
        for (prefix, at) in [(&[][..], 4), (&[0xEF, 0xBB, 0xBF][..], 7)] {
            let bytes = [prefix, b"<ODM\xff>"].concat();
            refused(
                read_bytes("first.xml", &bytes, &Options::default()),
                "malformed_xml",
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
        let lower = ODM_13.replace(r#"encoding="UTF-8""#, r#"encoding="utf8""#);
        assert_eq!(rows(&lower).rows(), 3);
        let controls = rows(&edit(
            ODM_13,
            r#"Value="2026-01-02""#,
            "Value=\"\u{7f}\u{85}\"",
        ));
        assert_eq!(cell(&controls, "Value", 0).as_deref(), Some("\u{7f}\u{85}"));
    }

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
            "limit",
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
            "limit",
            "XML text node exceeds",
        );
        refused(
            read_document(&edit(
                ODM_13,
                r#"Value="2026-01-02""#,
                &format!(r#"Value="{over}""#),
            )),
            "limit",
            "XML attribute value exceeds 9999999 bytes",
        );
        refused(
            read_document(&wrap(&format!("<!--{over}-->"))),
            "limit",
            "XML comment exceeds",
        );
        refused(
            read_document(&wrap(&format!("<{}/>", "a".repeat(50_001)))),
            "limit",
            "XML name exceeds 50000 bytes",
        );
        assert!(read_document(&wrap(&format!("<{}/>", "a".repeat(50_000)))).is_ok());
    }

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
            "limit",
            "XML depth exceeds 8",
        );
        assert!(read_bytes("deep.xml", nested(256).as_bytes(), &options(1000)).is_ok());
        refused(
            read_bytes("deep.xml", nested(257).as_bytes(), &options(1000)),
            "limit",
            "XML depth exceeds 256",
        );
        refused(
            read_bytes("deep.xml", ODM_13.as_bytes(), &options(7)),
            "invalid_option",
            "max_xml_depth must be at least 8",
        );
    }
}

mod inputs {
    use super::*;

    #[test]
    fn missing_paths_and_directories_are_not_found() {
        let scratch = Scratch::new("missing");
        let missing = scratch.path().join("absent.xml");
        refused(
            read_with(&missing, &Options::default()),
            "not_found",
            "absent.xml",
        );
        refused(
            read_with(scratch.path(), &Options::default()),
            "not_found",
            "not a regular file",
        );
        refused(
            read_with(
                &missing,
                &Options {
                    max_expanded_bytes: 0,
                    ..Options::default()
                },
            ),
            "not_found",
            "absent.xml",
        );
    }

    #[test]
    fn plain_xml_refuses_archive_member_and_oversized_files() {
        let options = Options {
            archive_member: Some("odm.xml".into()),
            ..Options::default()
        };
        refused(
            read_bytes("odm13.xml", ODM_13.as_bytes(), &options),
            "archive_member",
            "archive_member requires a TAR input",
        );
        let size = ODM_13.len() as u64;
        let exact = Options {
            max_expanded_bytes: size,
            ..Options::default()
        };
        assert_eq!(
            read_bytes("odm13.xml", ODM_13.as_bytes(), &exact)
                .unwrap()
                .rows(),
            3
        );
        let small = Options {
            max_expanded_bytes: size - 1,
            ..Options::default()
        };
        refused(
            read_bytes("odm13.xml", ODM_13.as_bytes(), &small),
            "limit",
            &format!("ODM XML exceeds {} bytes", size - 1),
        );
        refused(
            read_bytes(
                "odm13.xml",
                ODM_13.as_bytes(),
                &Options {
                    max_expanded_bytes: 0,
                    ..Options::default()
                },
            ),
            "invalid_option",
            "max_expanded_bytes must be at least 1",
        );
    }

    #[test]
    fn archive_names_without_archives_are_refused() {
        for name in ["odm.tar", "ODM.TAR.GZ", "odm.tgz"] {
            refused(
                read_bytes(name, ODM_13.as_bytes(), &Options::default()),
                "invalid_archive",
                "not a TAR archive",
            );
        }
        let gzipped_xml = gzip(ODM_13.as_bytes());
        refused(
            read_bytes("odm.xml.gz", &gzipped_xml, &Options::default()),
            "malformed_xml",
            "invalid XML character U+001F",
        );
    }

    #[test]
    fn other_compressed_archives_are_refused_with_their_format() {
        for (magic, format) in [
            (&b"BZh91AY&SY"[..], "bzip2"),
            (&[0xFD, b'7', b'z', b'X', b'Z', 0x00, 0x00][..], "xz"),
            (&[0x28, 0xB5, 0x2F, 0xFD, 0x00][..], "zstd"),
        ] {
            refused(
                read_bytes("odm.tar.compressed", magic, &Options::default()),
                "unsupported_compression",
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

    #[test]
    fn selection_counts_exact_names_and_candidates() {
        let none = Tar::new().file("notes.txt", b"x").finish();
        refused(
            read_tar(&none, &Options::default()),
            "archive_member",
            "exactly one XML member",
        );
        refused(
            read_tar(&none, &member("odm.xml")),
            "archive_member",
            r#"TAR input contains 0 entries named "odm.xml""#,
        );
        let twice = Tar::new()
            .file("odm.xml", ODM_13.as_bytes())
            .file("odm.xml", ODM_13.as_bytes())
            .finish();
        refused(
            read_tar(&twice, &member("odm.xml")),
            "archive_member",
            r#"TAR input contains 2 entries named "odm.xml""#,
        );
        let upper = Tar::new().file("STUDY/ODM.XML", ODM_13.as_bytes()).finish();
        assert_eq!(read_tar(&upper, &Options::default()).unwrap().rows(), 3);
        refused(
            read_tar(&upper, &member("study/odm.xml")),
            "archive_member",
            "0 entries",
        );
        let empty = vec![0_u8; 1024];
        refused(
            read_tar(&empty, &Options::default()),
            "archive_member",
            "exactly one XML member",
        );
    }

    #[test]
    fn unsafe_irregular_and_sparse_members_are_refused() {
        for name in ["../odm.xml", "/abs/odm.xml", "study/../../odm.xml"] {
            let archive = Tar::new().file(name, ODM_13.as_bytes()).finish();
            refused(
                read_tar(&archive, &Options::default()),
                "archive_member",
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
                "archive_member",
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
            "archive_member",
            "not a regular file",
        );
        let mut sparse = Header::new("odm.xml", b'S', ODM_13.len() as u64);
        sparse.gnu = true;
        let archive = Tar::new().entry(&sparse, ODM_13.as_bytes()).finish();
        refused(
            read_tar(&archive, &Options::default()),
            "archive_member",
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
            "archive_member",
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
            "limit",
            &format!("TAR member exceeds {} bytes", size - 1),
        );
        let compressed = gzip(&archive);
        let expansion = Options {
            max_expanded_bytes: 100_000,
            ..Options::default()
        };
        refused(
            read_bytes("odm.tar.gz", &compressed, &expansion),
            "limit",
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
            "invalid_archive",
            "invalid TAR header",
        );
        refused(
            read_tar(&valid[..second + 100], &Options::default()),
            "invalid_archive",
            "truncated TAR header",
        );
        refused(
            read_tar(&valid[..700], &Options::default()),
            "invalid_archive",
            "truncated TAR member data",
        );
        // A size within one block of u64::MAX cannot be padded to whole blocks.
        let oversized = Tar::new()
            .pax(&[("size", &u64::MAX.to_string())], "huge.bin", b"")
            .file("odm.xml", ODM_13.as_bytes())
            .finish();
        refused(
            read_tar(&oversized, &Options::default()),
            "invalid_archive",
            "invalid TAR member size",
        );
        let selected_short = Tar::new().file("odm.xml", ODM_13.as_bytes()).unterminated();
        refused(
            read_tar(&selected_short[..900], &Options::default()),
            "invalid_archive",
            "truncated TAR member data",
        );
        let dangling = Tar::new()
            .file("odm.xml", ODM_13.as_bytes())
            .raw(&Header::new("PaxHeaders/x", b'x', 0).encode())
            .finish();
        refused(
            read_tar(&dangling, &Options::default()),
            "invalid_archive",
            "extension header has no member",
        );
        // An unknown compression method in the gzip header.
        let mut corrupt = gzip(&valid);
        corrupt[2] = 7;
        refused(
            read_bytes("odm.tar.gz", &corrupt, &Options::default()),
            "invalid_archive",
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
            "invalid_archive",
            "cannot read ODM archive",
        );
    }
}
