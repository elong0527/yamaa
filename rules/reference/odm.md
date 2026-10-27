# ODM source profile

The `odm` profile reads a CDISC ODM 1.3 or 2.0 snapshot into the fixed item
table that an `odm` read binds ([NAME-8](../names.md#name-8)). An input whose
path ends in `.xml` selects it ([INPUT-1](../inputs.md#input-1)), and the
host function `read_odm` reads a file into the same table, with the same
refusals. No value, name or record is inferred beyond what the document
states.

## Reading

<a id="odm-1"></a>

### ODM-1. An `.xml` input is read as an ODM snapshot, refusing transactional content and skipping audit records

An `.xml` input is read as an ODM 1.3 or 2.0 snapshot into the item table of
[NAME-8](../names.md#name-8): one record per `ItemData` of its `ClinicalData`,
in document order. A `FileType` of `Transactional`, or a `TransactionType` of
`Remove` on any element, fails as `source_odm_transactional` before any record
is read, and a document without `ClinicalData` fails as
`source_odm_no_clinical_data`. Any other transaction type is read as the
present value. `AuditRecord`, `Signature`, `Annotation`, `ReferenceData` and
`AdminData` elements are skipped. `read_odm` gives the same table and
refusals.

Fails: `source_odm_transactional`, `source_odm_no_clinical_data`, `source_odm_xml_malformed`, `source_odm_xml_unsupported`, `source_odm_document_unsupported`, `source_odm_layout_unsupported`, `source_odm_attribute_missing`, `source_odm_attribute_ambiguous`, `source_odm_attribute_invalid`, `source_odm_metadata_conflict`, `source_odm_value_unsupported`, `source_odm_limit_exceeded`, `source_odm_archive_invalid`, `source_odm_compression_unsupported`, `source_odm_archive_member`, `value_not_permitted`.
Evidence: `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::a_transactional_file_is_refused_before_any_item`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::a_transactional_file_is_refused_before_a_malformed_element`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::a_subject_data_removal_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::a_study_event_data_removal_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::a_form_data_removal_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::an_item_group_data_removal_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::an_item_data_removal_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::an_odm_2_0_item_group_data_removal_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::an_insert_transaction_type_reads_as_the_present_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::an_update_transaction_type_reads_as_the_present_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::an_upsert_transaction_type_reads_as_the_present_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::a_context_transaction_type_reads_as_the_present_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::an_undefined_file_type_is_an_invalid_attribute`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::an_undefined_transaction_type_is_an_invalid_attribute`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::skipped_elements_and_their_content_are_not_read`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::a_document_without_clinical_data_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::an_empty_odm_2_0_root_has_no_clinical_data`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::clinical_data_inside_a_skipped_element_does_not_count`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::a_malformed_document_without_clinical_data_is_refused_as_xml`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::missing_paths_and_directories_fail_as_any_caller_path_does`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::a_directory_fails_as_a_path_that_is_not_a_regular_file`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_reads_xml.json::a_missing_path_fails_before_any_option_is_checked`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_archive_that_cannot_be_read_names_the_reason`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_archive_of_another_compression_names_the_format`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_archive_of_several_xml_members_needs_archive_member`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_archive_member_name_that_no_entry_has_counts_its_entries`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::odm_xml_deeper_than_its_limit_exceeds_a_read_limit`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::malformed_odm_xml_names_its_byte_and_reason`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_doctype_declaration_is_xml_the_reader_does_not_support`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::odm_xml_in_another_encoding_than_utf8_is_xml_the_reader_does_not_support`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_root_without_a_namespace_is_an_unsupported_document`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::nested_odm_form_data_is_a_layout_the_item_table_cannot_represent`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::odm_item_data_without_its_item_oid_misses_an_attribute`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::two_namespaced_odm_attributes_of_one_name_are_ambiguous`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_is_null_value_outside_its_list_is_an_invalid_attribute`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_file_type_outside_its_list_is_an_invalid_attribute`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::two_odm_definitions_of_one_key_with_different_names_conflict`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::odm_item_data_of_several_values_holds_a_value_the_item_table_cannot_represent`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::a_transactional_odm_file_is_not_a_snapshot`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_item_group_its_transaction_type_removes_is_not_a_snapshot`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_document_without_clinical_data_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_refusal_of_an_input_is_reported_at_the_input_path`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_option_below_its_minimum_is_not_permitted_at_its_name`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/odm.json::an_odm_source_the_host_cannot_read_is_the_opaque_resource_refusal`, `rust/crates/yamaa/tests/read_odm.rs::refusals_carry_their_registered_issue`, `rust/crates/yamaa/tests/read_odm.rs::only_a_snapshot_with_clinical_data_reads`, `rust/tests/host-fixtures/cases.json::archive-over-member-limit`, `python/tests/installed/installed_read_odm.py::test_refusals_raise_yamaa_error_carrying_their_issue`, `sdtm-dm-odm-snapshot`, `negative-odm-transactional`, `negative-odm-no-clinical-data`, `rust/crates/yamaa/tests/odm_input.rs::odm_reads_bind_the_item_table_of_an_xml_input`, `rust/crates/yamaa/tests/odm_input.rs::an_xml_input_no_odm_read_names_reads_every_column`, `rust/crates/yamaa/tests/odm_input.rs::the_profile_refuses_a_transactional_document_at_the_input`, `rust/crates/yamaa/tests/odm_input.rs::the_profile_refuses_a_document_without_clinical_data_at_the_input`, `rust/crates/yamaa/tests/odm_input.rs::an_xml_input_declares_no_types`, `rust/crates/yamaa-adapters/tests/format_features.rs::odm_reader_is_an_independent_capability`, `negative-odm-unescaped-ampersand`, `negative-odm-latin1-declaration`, `negative-odm-version-1-2`, `negative-odm-typed-item-data`, `negative-odm-empty-subject-key`, `negative-odm-vendor-attribute-twice`, `negative-odm-null-flag`, `negative-odm-item-name-conflict`, `negative-odm-null-with-value`, `rust/tests/host-fixtures/cases.json::tar-name-without-archive`, `rust/tests/host-fixtures/cases.json::bzip2-archive`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_ascii.json::ascii_declaration_preserves_items`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_ascii.json::ascii_xml_is_a_direct_specification_input`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/odm_ascii.json::ascii_declaration_refuses_utf8_source_bytes`.

**The item table.** `ClinicalData` gives `StudyOID` and `MetaDataVersionOID`,
`SubjectData` gives `SubjectKey`, `StudyEventData`, `FormData` and
`ItemGroupData` give their OIDs and repeat keys, and `ItemData` gives
`ItemOID` and `Value`; the other columns of the `read_odm` table are vendor
fields, which an `odm` read never sees ([NAME-8](../names.md#name-8)). An item
needs a `FormData` with one `ItemGroupData`, or two nested `ItemGroupData`, the
outer one giving the form columns. A name comes from the data element's
namespaced name attribute, or else from the definition outside `ClinicalData`
that precedes the item, keyed by study OID, metadata version OID and OID; two
definitions of one key with different names are refused. `ItemData` outside
`ClinicalData` gives no record.

**An `.xml` input.** The input is captured once, as every input is
([FILE-11](files.md#file-11)), and its bytes are read as one plain XML
document under `read_odm`'s defaults, so an archive is refused as XML. One
that an `odm` read names declares no `types`, `schema` or `ordinal` (NAME-8,
[INPUT-4](../inputs.md#input-4)). One that no `odm` read names reads every
column of the table, as the Parquet file of `read_odm`'s table would: each
`types` entry fails as `redundant_field_type`
([INPUT-6](../inputs.md#input-6)), and `schema`, `ordinal`, `empty_string`,
`key` and `require_match` apply to it as to a Parquet input. The run report
records the input's profile as `odm` and the items it decoded as its records
([CHECK-24](../verification.md#check-24)). A build without the ODM reader
refuses the input as `engine_rejected` with stage `ingest` and code
`odm_reader_unavailable`, as it refuses the other formats it does not
compile.

**The document.** It is UTF-8 without a DOCTYPE, well formed under XML 1.0
and Namespaces 1.0, with the root `ODM` in the ODM 1.3 or 2.0 namespace and an
`ODMVersion`, when it has one, that agrees with the namespace. Elements in
other namespaces are not read, and nested containers, nested items, ODM 1.3's
typed `ItemData` elements in clinical data and ambiguous values are refused.

**ASCII declarations.** An XML declaration may instead name `US-ASCII`,
compared ASCII case-insensitively, only when every source byte is ASCII.
Numeric character references may name non-ASCII characters: the restriction
is on the source bytes, not the decoded values. A non-ASCII source byte,
including a UTF-8 byte order mark, fails as `source_odm_xml_malformed`, with
its zero-based byte position and reason. This check covers skipped content,
comments and bytes beyond the first read buffer. `read_odm`, selected archive
members and direct XML inputs use the same check; no transcoding occurs.

**Snapshots.** The root's `FileType` reads when it is `Snapshot` or absent and
fails as `source_odm_transactional` when it is `Transactional`. Every element
read in the document's ODM namespace, the root included, is checked for a
`TransactionType`: `Remove` fails as `source_odm_transactional`, and `Insert`,
`Update`, `Upsert` and `Context` read as the present value. A `FileType` or
`TransactionType` value ODM does not define fails as
`source_odm_attribute_invalid`; values compare exactly. The table's
`TransactionType` column holds the item group's value.

**Skipped content.** An `AuditRecord`, `Signature`, `Annotation`,
`ReferenceData` or `AdminData` element is skipped with everything inside it:
nothing in it gives a record or a name, and nothing in it is checked except as
XML and against the read limits. A `ClinicalData` inside a skipped element
does not count as the document's.

**No clinical data.** A document is checked for `ClinicalData` once it has
been read whole, after every other check. A `ClinicalData` that holds no item
reads as an empty table.

**One issue per refusal.** A read stops at its first refusal in document order
and reports one issue, at no spec path, naming the source in `context.path`:
the path as `read_odm` was given it, or as an input declares it. An input's
issue also names its dataset and is reported at `input.<dataset>.path`. Each
condition is reported in the `ingest` phase:

| Condition | Rejects | Context beside `path` |
| --- | --- | --- |
| `source_odm_transactional` | a `FileType` of `Transactional`, or a `TransactionType` of `Remove` | `element`, `attribute`, `value` |
| `source_odm_no_clinical_data` | a document without `ClinicalData` | none |
| `source_odm_xml_malformed` | bytes that are not well-formed XML | `position`, the byte offset, and `reason` |
| `source_odm_xml_unsupported` | a DOCTYPE, or an encoding declaration other than UTF-8/UTF8 or US-ASCII | `reason` |
| `source_odm_document_unsupported` | a root that is not `ODM`, another namespace, a disagreeing `ODMVersion`, or a nested `ODM` | `reason` |
| `source_odm_layout_unsupported` | content the table cannot represent: a nested container or item, an item without its subject, event, form and group, a definition outside its `MetaDataVersion`, a typed `ItemData` | `reason` |
| `source_odm_attribute_missing` | an empty or absent attribute the element requires, such as `ItemOID` | `element`, `attribute` |
| `source_odm_attribute_ambiguous` | two namespaced attributes with one local name that the table reads | `attribute` |
| `source_odm_attribute_invalid` | an `IsNull`, `FileType` or `TransactionType` value outside its list | `element`, `attribute`, `value` |
| `source_odm_metadata_conflict` | two definitions of one key with different names | `definition`, `study`, `metadata_version`, `oid` |
| `source_odm_value_unsupported` | an item value the table cannot hold without loss | `item`, `reason` |
| `source_odm_limit_exceeded` | a source over a read limit | `reason` |
| `source_odm_archive_invalid` | a `read_odm` archive that cannot be read | `reason` |
| `source_odm_compression_unsupported` | a `read_odm` archive compressed with bzip2, xz or zstd | `format` |
| `source_odm_archive_member` | a `read_odm` archive member that cannot be selected or read | `reason` |

`IsNull` admits `yes`, `true` and `1`, and `no`, `false` and `0`, compared
ASCII case-insensitively.

**Read limits.** A text node, attribute value, comment or processing
instruction holds less than 10,000,000 bytes, a name at most 50,000 bytes,
namespace declarations in scope at most 4,096, and the table at most the
engine's table rows. An input is read with `read_odm`'s defaults: XML at most
268,435,456 bytes and nested at most 64 elements deep.

**`read_odm`.** The source is a caller's own path, captured once through the
resolver: one that reaches no regular file fails with
[FILE-7](files.md#file-7)'s and [FILE-8](files.md#file-8)'s conditions, naming
the path as given and no spec path ([FILE-11](files.md#file-11)). A file the
host cannot read is the engine's opaque refusal, `engine_rejected` with stage
`ingest` and code `resource_unreadable`. Its options are checked before the
source is read: `max_archive_members` and `max_expanded_bytes` at least 1 and
`max_xml_depth` at least 8, a smaller value failing as `value_not_permitted`
at the option's name, with `context.minimum`, in the `validation` phase. A
`max_xml_depth` above 256 bounds the nesting at 256. A source whose first
block is a TAR header, directly or after gzip decompression, is an archive,
and a `.tar`, `.tar.gz` or `.tgz` name that holds none is refused. An archive
is read when it holds exactly one `.xml` member outside macOS metadata, or
when `archive_member` names exactly one member; the member is a regular file,
not sparse, with a relative path without `..`. Every member counts toward
`max_archive_members`, and the selected member, a plain XML file, and a gzip
archive's expansion each hold at most `max_expanded_bytes`. A source a
caller's resolver serves is held whole, at most 10,737,418,240 bytes.

An archive is not an input in 1.0: `.tar` selects no profile
([INPUT-1](../inputs.md#input-1)), `read_odm` keeps reading archives, and
`transactions: replay` and `archive_member` stay reserved.
