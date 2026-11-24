# Define-XML

This file is a profile: it is optional, and it applies only to a build that
selects an `sdtm`, `adam` or `send` submission section of a project file.
Such a build composes the section, the study and the submission metadata of the
specifications the section lists into one deterministic Define-XML document.
The default release is 2.1.11; an explicit `define_version: "2.0.0"` selects
the earlier representation ([DEFINE-29](#define-29)). An `adam` section may
also declare analysis results metadata, which the document carries as
`arm:AnalysisResultDisplays` ([DEFINE-31](#define-31),
[DEFINE-32](#define-32)). A domain build never generates a Define-XML
document.

## The study document

<a id="define-1"></a>

### DEFINE-1. A Define-XML document is composed from one selected submission section of the project file

A study document is composed from one selected `sdtm`, `adam` or `send` section
of an admitted project file ([PROJECT-1](project.md#project-1)). The project
supplies `study` and shared `codelists`; the selected section supplies its
implementation guides, supporting documents, an ordered `specs` list and one
`define` target. Each section produces exactly one Define-XML document. The
selected project section is the only place the composition is declared.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa/tests/project_operations.rs::a_submission_composes_the_selected_section`, `python/tests/installed/installed_public_define.py::test_defaults_selection_bound_lists_and_required_acrf_aliases`.

An implementation must not assemble a document from whatever specifications a
directory holds: which datasets one submission represents is a study decision,
and a directory listing is not one. Composition requires the `study`, the
section's `standards` and `define` that a run of the same project does not
([DEFINE-28](#define-28)).

<a id="define-2"></a>

### DEFINE-2. The section's `define` target names the document's file, ends in `.xml`, and is `define.xml` beside a transport

The selected section's `define` names the file the document produces, and its
extension must be `.xml`; when the section declares `transport`, requesting a
transport file per dataset, its file name must be exactly `define.xml`. The
extension mapping of [OUTPUT-5](../output.md#output-5) is not consulted: that
mapping selects a profile for a dataset artifact, and this artifact has one
form. A `define`
target whose extension is not `.xml`, or that is not named `define.xml` beside
a transport, fails validation.

Fails: `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `negative-transport-define-name`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_define_target_without_the_xml_extension_is_not_permitted`.

The transport files are written beside the document
([JSON-1](dataset-json.md#json-1)) and Dataset-JSON names it as `define.xml`
([JSON-8](dataset-json.md#json-8)). The file name is compared exactly, case
included, and the directory is free.

<a id="define-3"></a>

### DEFINE-3. Two generations with the same captured inputs and creation time produce the same bytes

Two generations with the same captured inputs and `created` produce the same
bytes. An explicit `created` is a whole-second civil datetime, which a trailing
`Z` may mark as UTC; omission selects the current UTC time during preparation,
and Save retains that time. An omitted
`file_oid` is `study.id`, the selected lower-case section key and
`Define-XML_2.1` (or `Define-XML_2.0` for 2.0.0), joined by dots. An omitted metadata-version ID is
`study.id.section`, and an omitted name is `study.name section metadata`.

Fails: `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/service_build_metadata.rs::clock_is_observed_once_and_calendar_admission_is_owned_by_the_engine`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_creation_time_with_a_utc_offset_is_not_permitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_creation_time_marked_utc_is_written_through_to_the_document_and_dataset_json`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_creation_time_marked_utc_writes_the_transport_header_without_its_designator`, `python/tests/installed/installed_public_define.py::test_defaults_selection_bound_lists_and_required_acrf_aliases`.

Explicit overrides retain their admitted text, independently of these
defaults. An explicit `created` that is not a whole-second civil datetime,
alone or followed by `Z`, fails with `value_not_permitted`, a numeric offset
included. The document and each Dataset-JSON file write `created` as given, `Z`
included, and an XPT header, which has no designator, writes its civil time.

<a id="define-4"></a>

### DEFINE-4. A `Submission` context adds the requirements a regulator imposes, and `Other` relaxes nothing else

`context` states whether the document is used in a submission. Under
`context: Other`, a document is generated from whatever the declarations supply
and is valid against the Define-XML 2.1 schema. Under `context: Submission`,
each requirement listed below additionally holds; each is a requirement the
Define-XML specification or a regulator's transport places on a submission. A
requirement unmet under `Submission` fails, reporting the requirement and the
position.

Fails: `missing_required_field`, `sas_name_too_long`, `transport_label_invalid`, `transport_text_invalid`.
Evidence: `sdtm-dm-metadata`, `sdtm-ta-sponsor-epoch`, `sdtm-vs-epoch-from-subject-elements`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_submission_section_without_a_transport_is_refused_at_its_transport`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_section_under_other_names_its_artifact_without_a_transport`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_submission_section_without_a_transport_is_refused_before_its_artifact_is_located`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_published_codelist_without_its_nci_code_is_refused_under_submission`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_published_item_without_its_nci_code_is_refused_under_submission`, `rust/tests/host-fixtures/cases.json::submission-column-label`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_without_its_form_is_refused_under_submission`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_citing_its_form_without_a_page_is_refused_under_submission`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_without_its_form_cites_the_form_without_a_page_under_other`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_citing_its_form_without_a_page_is_admitted_under_other`.

Nothing in this profile is relaxed by `Other`; that context only omits the
requirements a regulator imposes. Under `Submission`:

- the section declares a `transport` (`xpt`, `dataset_json` or
  `dataset_ndjson`), since a regulator takes a dataset only in a transport
  file, so the document never names a CSV or Parquet artifact
  ([DEFINE-16](#define-16));
- every dataset declares `submission.domain` or inherits one, for the `SDTM`
  and `SEND` models;
- every dataset's `SASDatasetName` and every column's `SASFieldName` is
  written, so [DEFINE-22](#define-22) must admit every name;
- every dataset's `submission.label` and every column's `label` is at most 40
  characters of printable ASCII ([XPT-5](xpt.md#xpt-5));
- when the section declares `transport`, every published text value is
  printable ASCII of at most 200 bytes ([XPT-7](xpt.md#xpt-7)), for a
  Dataset-JSON file as for a SAS Transport file;
- every dataset carries a `def:ArchiveLocationID`, unless it declares
  `has_no_data`;
- every dataset carries a `Description` and a `def:Class`;
- every dataset declares at least one key, which
  [OUTPUT-2](../output.md#output-2) already requires, and
  every key column carries a `KeySequence`;
- every column carries a `def:Origin`, which [SUB-14](submission.md#sub-14)
  already requires;
- every `Collected` origin from `Investigator` or `Subject` declares
  `documents` with at least one `pages` reference into the annotated case
  report form, which [SUB-17](submission.md#sub-17) requires; and
- every codelist that declares `items` and is drawn from a standard whose name
  is `CDISC/NCI` declares `alias`, and so does every one of its items except
  one declared `extended: true`, which has no NCI code and declares no alias
  ([CT-6](terminology.md#ct-6)). An external codelist is excluded: it
  represents its dictionary rather than CDISC controlled terminology, and has
  no items to identify.

A section without `transport` under `Submission` fails with
`missing_required_field` at its `transport` when its document is composed;
running its specifications is unaffected, and `context: Other` composes the
document beside the artifacts. A codelist `alias`, or the `alias` of a
published item, missing under `Submission` fails with
`missing_required_field`; a sponsor's extension of an
extensible list, such as an added epoch, unit or test code, needs none. A
collected origin without a page reference fails as
[SUB-17](submission.md#sub-17) states. A name, label or text value outside
these limits fails as [XPT-5](xpt.md#xpt-5) and [XPT-7](xpt.md#xpt-7) state;
under `context: Other`, a SAS Transport file still refuses what it cannot
carry.

## Composition

<a id="define-5"></a>

### DEFINE-5. Each `specs` entry contributes one dataset, identified by its domain and placed in entry order

Each `specs` entry names one specification resolved with its parent layers.
That specification's `domain` supplies the dataset ID and name; the ID is
unique within the document and supplies every generated identifier for that
dataset. The entry order is the document's dataset order. Two entries must not
name the same specification, and two validated entries must not resolve to the same
artifact path.

Fails: `duplicate_dataset_entry`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_section_listing_one_dataset_twice_is_refused`, `rust/crates/yamaa-core/tests/schema_submission.rs::thirty_domains_with_fifteen_hundred_nodes_admit_complete_column_and_row_scans`.

Written resource and output paths retain their original declaring layer under
[File resolution](../reference/files.md). This profile imposes no other order:
the recommended order of a submission's datasets is a property of the standard
a study follows rather than of this language, and a study that follows one
writes its entries in it. A document that described one artifact twice would
carry two definitions a reader cannot reconcile; two entries naming the same
specification, or two validated entries resolving to the same artifact path, fail with
`duplicate_dataset_entry`.

<a id="define-6"></a>

### DEFINE-6. The section's implementation guides belong to its model, and each dataset is bound to the one it selects

The selected section's `standards` lists one or more implementation guides
of type `IG` with distinct names, each belonging to the selected section's
model. Each represented dataset is bound to the guide its specification's
`submission.standard` names, or to the first when it names none
([DEFINE-8](#define-8)); a dataset declaring `non_standard: true` is bound to
none ([SUB-4](submission.md#sub-4)). The `sdtm`, `adam` and `send` sections
produce separate documents, and each document carries the
controlled-terminology standards cited by its bound codelists' sources.

Fails: `standard_model_mismatch`, `duplicate_identifier`.
Evidence: `sdtm-dm-metadata`, `schema-section-standards`, `negative-section-standard-model`, `negative-section-standard-repeated`, `rust/crates/yamaa/tests/language_header.rs::a_section_lists_only_specifications_of_its_model`, `rust/crates/yamaa/tests/language_header.rs::a_dataset_binds_the_section_standard_it_selects`, `rust/crates/yamaa-core/src/front_end/v1/project.rs::define_names_a_1_0_section_s_guides_by_their_place_in_standards`, `rust/crates/yamaa-core/src/front_end/v1/project.rs::define_requires_a_1_0_section_s_standards_by_their_1_0_name`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_section_guide_of_another_model_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_section_guide_listed_twice_is_refused`, `rust/crates/yamaa-core/tests/schema_submission.rs::every_shipped_guide_spelling_is_closed_metadata_not_family_composition`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_non_standard_dataset_is_marked_non_standard_and_names_no_guide`.

Composition checks every entry of the list. A guide outside the section's
model fails with `standard_model_mismatch` at `<section>.standards[<i>].name`,
naming the guide in `value`, the section in `section` and its model in
`model`. A name an earlier entry takes fails with `duplicate_identifier` at
the later entry's `name`, naming it in `identifier`. A section without
`standards`, or with an empty list, fails under [DEFINE-28](#define-28).

The document lists every guide of the section in declaration order, whether
or not a dataset binds it, before the terminology standards, and each
`ItemGroupDef` names its dataset's bound guide in `def:StandardOID`
([DEFINE-15](#define-15)). A non-standard dataset's `ItemGroupDef` carries
`def:IsNonStandard="Yes"` instead, so each carries exactly one of the two, as
a codelist with items does ([DEFINE-20](#define-20)). Names are unique within
a section, so a guide's generated identifier stays `STD.<name>`
([DEFINE-9](#define-9)). A guide's `status`, `Draft`, `Provisional` or
`Final`, is its `Status` ([DEFINE-13](#define-13)).

[SUB-3](submission.md#sub-3) checks the bound guide against the model the
specification declares, which a 1.0 section's listing makes the section's,
so a guide this rule admits belongs to it.

<a id="define-7"></a>

### DEFINE-7. Every composed specification declares dataset and column submission metadata and column labels

Every specification a document names must declare `root.submission`, and every
column in its `output.columns` must declare both `label` and
`column.submission`. A specification named by an entry that declares no
`root.submission`, or an output column with no `label` or no
`column.submission`, fails. A column that reproduced metadata omits declares
`column.submission` alone ([DEFINE-30](#define-30)).

Fails: `submission_metadata_missing`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_output_column_without_submission_metadata_is_refused`, `rust/crates/yamaa-application/tests/submission_service.rs::missing_study_is_the_real_typed_owner_before_any_metadata_verification`, `rust/crates/yamaa/tests/language_header.rs::missing_submission_metadata_is_reported_at_the_listing_or_the_column_submission`.

A document reports on the whole artifact, so a column with nothing to report is
a gap in the document, not an omitted row. A specification without
`root.submission` fails with `submission_metadata_missing` at its listing,
`<section>.specs[<n>]`, naming its domain in `detail`. An output column
without `column.submission` fails at that column's `submission`,
`<section>.specs[<n>].columns.<column>.submission`, naming the column in
`detail`. An output column without `label` fails at the same path, with
`label` in `detail`; a column that lacks both fails once, for its
`submission`.

Validated metadata describes every output column, since the run publishes
each one. That includes a column derived only to identify rows, such as a
sequence number declared in `keys`: a key is an output column
([OUTPUT-2](../output.md#output-2)), so the published dataset carries it and
the document describes it as a submitted variable, with its own `label` and
`column.submission`. Only reproduced metadata leaves an output column out of
the document, to record a received document that omits it
([DEFINE-30](#define-30)).

The specification's `output.columns` and `keys` are validated under
[OUTPUT-1](../output.md#output-1) and [OUTPUT-2](../output.md#output-2).
A composed submission reports those findings at the corresponding entry under
its specification listing. Output columns rejected by OUTPUT-1 have no
metadata for this rule to check.

<a id="define-8"></a>

### DEFINE-8. Composition resolves the codelist, standard and document names a specification leaves unresolved

Composition resolves the names [Submission metadata](submission.md) and
[Controlled terminology](terminology.md) leave unresolved: a column's
`codelist`, every document reference, and the standard a dataset's
`submission.standard` selects from the section's `standards`, the first entry
when it is omitted. A name matching no declaration fails. Composition also
checks that the selected standard is an implementation guide of the model the
specification declares ([SUB-3](submission.md#sub-3)).

Fails: `unknown_standard`, `unknown_codelist`, `unknown_document`.
Evidence: `sdtm-dm-metadata`, `schema-section-standards`, `negative-unknown-standard`, `rust/crates/yamaa/tests/language_header.rs::a_dataset_binds_the_section_standard_it_selects`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_dataset_selecting_a_guide_its_section_lacks_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_origin_citing_an_undeclared_document_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_unknown_per_value_codelist_fails_at_its_row_path`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_per_value_codelist_of_another_type_fails_at_its_row_path`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_display_or_an_analysis_result_citing_an_undeclared_document_is_refused`.

A `submission.standard`, `codelist` or document reference naming no
declaration fails with `unknown_standard`, `unknown_codelist` or
`unknown_document`, reporting the referencing position. A document a result
display or an analysis result names resolves the same way
([DEFINE-32](#define-32)), at the display's `documents` or the result's
`join`, `documentation` or `programming_code`. Guide names compare
exactly. `unknown_standard` is reported at
`<section>.specs[<n>].submission.standard`, naming the selected name in
`value`, the section in `section` and the names of its `standards`, in
declaration order, in `standards`. Names are unique within a section
([DEFINE-6](#define-6)), so a selection is never ambiguous. A specification
checked or run alone selects nothing, so its `submission.standard` is
resolved only here. The rules that depend on the model are checked on the
specification alone ([SUB-23](submission.md#sub-23)), and again here under
the section's model.

## Identifiers

<a id="define-9"></a>

### DEFINE-9. Every generated identifier is a fixed prefix and the declared identifiers of the object's position

Every generated identifier is a prefix, a period, and the declared identifiers
of the object's position, as the table below fixes; the construction is total.
An implementation must not mint an identifier from a counter, a hash or a
random value. One base `ItemDef` is generated per dataset and column, and
neither base nor per-value `ItemDef` elements are shared between datasets.

Evidence: `sdtm-dm-metadata`, `adam-adlb-metadata`, `schema-analysis-results`, `schema-codelist-dotted-id`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_display_and_a_result_that_declare_oid_carry_it_and_their_where_clauses_and_join_keep_the_generated_identities`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_display_oid_without_its_prefix_or_a_result_oid_ending_in_a_space_is_a_pattern_mismatch`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_declared_study_oid_is_the_study_oid_of_the_document_and_its_transport`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::value_identifiers_survive_unrelated_insertion_and_entry_reordering`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::where_values_keep_their_text_with_xml_escaping`.

| Object | Identifier |
|---|---|
| `Study` | `STDY.<study.id>`, or the section's `study_oid` |
| `MetaDataVersion` | `MDV.<metadata_version.id>` |
| `def:Standard`, implementation guide | `STD.<standard.name>` |
| `def:Standard`, terminology | `STD.CT_<standard.publishing_set>` |
| `ItemGroupDef` | `IG.<dataset.id>` |
| `ItemDef` | `IT.<dataset.id>.<column>` |
| `ItemDef`, per value | `IT.<dataset.id>.<column>.<entry id>` |
| `def:ValueListDef` | `VL.<dataset.id>.<column>` |
| `def:WhereClauseDef` | `WC.<dataset.id>.<column>.<entry id>` |
| `CodeList` | `CL.<codelist.id>` |
| `MethodDef` | `MT.<dataset.id>.<column>` |
| `MethodDef`, per value | `MT.<dataset.id>.<column>.<entry id>` |
| `def:CommentDef`, dataset | `COM.<dataset.id>` |
| `def:CommentDef`, column | `COM.<dataset.id>.<column>` |
| `def:CommentDef`, per value | `COM.<dataset.id>.<column>.<entry id>` |
| `def:leaf`, document | `LF.<document.id>` |
| `def:leaf`, dataset archive | `LF.<dataset.id>` |
| `arm:ResultDisplay` | `RD.<display.id>`, or the display's `oid` |
| `arm:AnalysisResult` | `AR.<display.id>.<result.id>`, or the result's `oid` |
| `def:WhereClauseDef`, analysis dataset | `WCA.<display.id>.<result.id>.<dataset.id>` |
| `def:CommentDef`, analysis join | `COMA.<display.id>.<result.id>` |

In the `def:Standard` rows, `standard` is an entry of the selected section's
`standards` for an implementation guide ([DEFINE-6](#define-6)), and for a terminology
standard the `standard` of a codelist source whose codelists the document binds
([CT-2](terminology.md#ct-2)). Sources that declare the same `name`,
`publishing_set` and `version` cite one terminology standard and share its
`def:Standard`; two terminology standards with one `publishing_set` fail under
[DEFINE-10](#define-10).

A codelist `id` may itself join segments with periods
([CT-1](terminology.md#ct-1)), so a codelist `ADSL.ARMCD` is `CL.ADSL.ARMCD`.

A section's non-empty `study_oid` is the document's `Study` OID in place of
`STDY.<study.id>`, written as declared, with no prefix, so a received document
whose `Study` OID repeats its `FileOID` can be written. A document holds one
`Study`, the only element of its type and one no element of the document
references, so the declaration cannot collide with an identifier of its space
([DEFINE-10](#define-10)). Dataset-JSON's `studyOID` carries the same value
([JSON-1](dataset-json.md#json-1)).

A reproduced method or comment that declares `oid` carries that identifier in
place of its `MT.` or `COM.` row, and every reference to it cites the declared
one ([DEFINE-30](#define-30)). The declaration is still one a reviewer can
read, and the schema keeps its prefix: a method `oid` begins `MT.` and a comment
`oid` begins `COM.`, followed by printable ASCII without spaces.

A result display or an analysis result that declares `oid` carries that
identifier in place of its `RD.` or `AR.` row, written as declared, so a
received document's identifiers, such as `RD.Table 14.2.2.1.3.1.1` and
`AR.Table1`, can be written; unlike a method's or a comment's, the declaration
needs no reproduced metadata. The schema keeps the prefix: a display `oid`
begins `RD.` and a result `oid` begins `AR.`, followed by printable ASCII that
may hold spaces, as the ODM `oid` type admits, and does not end in one. The
display's and the result's `id` still identify them and build every other
analysis results identifier.

A result display's `id` may hold periods, as `T14.3.01` does, while a result
`id` and a dataset identifier hold none, so each of the four generated
analysis results identifiers reads back from the right into exactly one
display, result and dataset ([DEFINE-32](#define-32)). An analysis dataset's
where clause and an analysis join comment take the prefixes `WCA.` and `COMA.`, never `WC.` and
`COM.`: no declared identifiers, and no declared `oid`, which begins `MT.` or
`COM.`, can spell them. A display's and a result's documents are the
section's `documents` and keep `LF.<document.id>`.

A per-value entry id is the `id` of the column's value-level entry
([SUB-12](submission.md#sub-12)), an identifier unique within the column. It
identifies the definition independently of entry order; `ItemRef.OrderNumber`
remains the entry's position in the list. OIDs admit string values, while a
`def:leaf` ID separately requires an XML NCName. Generated text remains
subject to [DEFINE-24](#define-24).

A generated identifier is a function of declarations a reviewer can read, so a
document regenerated after an unrelated edit keeps every identifier it had.
Per-value definitions follow [DEFINE-27](#define-27).

<a id="define-10"></a>

### DEFINE-10. Declared identifiers are unique in their space, so generated identifiers cannot collide

Collisions are impossible rather than resolved. Dataset, codelist, standard,
document and result display identifiers are each unique within the study
document; a column name is unique within its specification; a column's entry
ids identify its per-value definitions, and a display's result ids its
results; and the prefixes are disjoint. Two kinds of object share
one identifier space, `def:leaf`, so a document identifier must not equal a
dataset identifier.

Fails: `duplicate_define_identifier`.
Evidence: `negative-terminology-version-collision`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_declared_display_oid_equal_to_another_display_s_generated_one_is_a_duplicate_identifier`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::one_declared_result_oid_on_results_of_two_displays_is_a_duplicate_identifier`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::one_received_method_oid_on_two_different_definitions_is_a_duplicate_identifier`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_received_method_oid_equal_to_a_generated_one_is_a_duplicate_identifier`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_document_identifier_equal_to_a_dataset_identifier_collides`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_later_release_of_a_cited_terminology_standard_is_refused_at_its_publishing_set`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::another_standard_name_under_a_cited_publishing_set_is_refused_at_its_publishing_set`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::sources_citing_one_terminology_standard_share_its_one_definition`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/admission.json::independent_complete_fifteen_record_project_report_preserves_every_field_and_order`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_display_id_or_name_another_source_declares_and_a_result_id_its_display_repeats_are_refused`.

A declared method or comment `oid` ([DEFINE-30](#define-30)) shares its
prefix's space with the generated identifiers. Elements that declare one `oid`
and write the same definition share one `MethodDef` or `def:CommentDef`; any
other repeat, a different definition under one declared `oid` or a declared
`oid` equal to a generated one, fails with `duplicate_define_identifier`,
reporting the identifier.

A declared result display or analysis result `oid` ([DEFINE-9](#define-9))
shares the `RD.` or `AR.` space with the generated ones and is never shared:
one equal to another display's or result's identifier, declared or generated,
fails with `duplicate_define_identifier` at the later display or result,
reporting the identifier.

The shared `def:leaf` space is checked, not disambiguated. A duplicate dataset
`id`, codelist `id` or document `id`, or a document `id` equal to a dataset
`id`, fails with `duplicate_define_identifier`. A result display `id` an
earlier display of the section declares, in the same analysis results source
or another, fails with `duplicate_define_identifier` at the later `id`,
reporting it as `identifier`; no source replaces another's display
([DEFINE-32](#define-32)). A terminology standard's
identifier is generated from its `publishing_set` alone
([DEFINE-9](#define-9)), so when two codelist sources whose codelists the
document binds declare one `publishing_set` with a different `name` or
`version`, the later source's `publishing_set` fails with
`duplicate_define_identifier`, reporting the identifier.

## Document structure

<a id="define-11"></a>

### DEFINE-11. The document has one fixed element structure in a fixed order, and omits an element without content

The generated document is exactly the structure below, and every list appears
in the order stated; an element whose content is absent is omitted rather than
written empty. The element order is the order the Define-XML 2.1 schema
requires, and within each repeated element the order below is fixed. `ODM`
declares the ODM 1.3 namespace as the default, `def` for
`http://www.cdisc.org/ns/def/v2.1` and `xlink` for
`http://www.w3.org/1999/xlink`, then `arm` for
`http://www.cdisc.org/ns/arm/v1.0` only when a result display is declared, and
no other namespace.

Evidence: `sdtm-dm-metadata`, `schema-analysis-results`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::fixed_time_xml_has_one_column_list_two_ordered_item_and_where_references`, `rust/crates/yamaa-application/tests/file_resources.rs::dm_both_goldens_without_input_execution_and_save_uses_held_bytes`.

```text
<?xml version="1.0" encoding="UTF-8"?>
<?xml-stylesheet ...?>            when stylesheet is declared
<ODM>
  <Study>
    <GlobalVariables>
      <StudyName/> <StudyDescription/> <ProtocolName/>
    </GlobalVariables>
    <MetaDataVersion>
      <def:Standards>              standards, in declaration order
      <def:AnnotatedCRF>           annotated_crf documents, declaration order
      <def:SupplementalDoc>        supplemental documents, declaration order
      <def:ValueListDef>*          dataset order, then output.columns order
      <def:WhereClauseDef>*        value-list order, then entry order; then analysis datasets
      <ItemGroupDef>*              datasets, in entry order
      <ItemDef>*                   dataset order, then output.columns order; per-value definitions follow their base column
      <CodeList>*                  codelists, in declaration order
      <MethodDef>*                 ItemDef order, including per-value methods
      <def:CommentDef>*            comment order, below
      <def:leaf>*                  documents, in declaration order
      <arm:AnalysisResultDisplays> result displays, when one is declared
    </MetaDataVersion>
  </Study>
</ODM>
```

Comment definitions are emitted per dataset in entry order: the dataset's own
comment first, its columns' comments in `output.columns` order, with each
column's per-value comments following its base comment in entry order. The
analysis join comments follow every dataset's, in display and result order.

The where clauses of analysis datasets follow every value-level where clause,
in display, result and dataset order. A dataset's `def:leaf` is a child of its
`ItemGroupDef`. A supporting document's `def:leaf` is a child of
`MetaDataVersion`, as the schema requires. `arm:AnalysisResultDisplays` is the
last child of `MetaDataVersion`, where the ARM v1.0 schema places it, and the
only extension element generated ([DEFINE-31](#define-31)); a document
without a result display carries neither the element nor its namespace, and
keeps the bytes it had.

## Element mapping

<a id="define-12"></a>

### DEFINE-12. `ODM`, `Study` and `MetaDataVersion` carry fixed attributes from the study and the selected section

`ODM` carries its namespace declarations, then the attributes the table below
lists, in that order. `Study` carries `OID` alone, and its `GlobalVariables`
child carries `StudyName`, `StudyDescription` and `ProtocolName`, in that
order, from `study.name`, `study.description` and `study.protocol_name`.
`MetaDataVersion` carries `OID`, `Name` from `metadata_version.name`,
`Description` when declared, and `def:DefineVersion` from `define_version`, in
that order.

Fails: `source_system_incomplete`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_empty_generator_version_leaves_the_source_system_incomplete`, `python/tests/installed/installed_public_define.py::test_source_version_defaults_and_schema_facet_failures`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_declared_source_system_names_the_generator_in_both_documents`.

| Attribute | Source |
|---|---|
| `ODMVersion` | fixed `1.3.2` |
| `FileType` | fixed `Snapshot` |
| `FileOID` | `file_oid` |
| `CreationDateTime` | selected `created`, or preparation-time UTC |
| `Originator` | `originator`, when declared |
| `SourceSystem` | section `source_system`, or `yamaa` |
| `SourceSystemVersion` | section override or actual installed version |
| `def:Context` | `context` |

Each section may declare a non-empty `source_system` and a non-empty
`source_system_version`. They override the source-system name and version
metadata in both Define-XML and Dataset-JSON; omission names `yamaa` and
selects the actual installed yamaa version. A source-system version that would
be written empty fails with `source_system_incomplete`. Qualification retains
the actual generator version separately from this metadata override.
The generated Define-XML release is `2.1.11`.

<a id="define-13"></a>

### DEFINE-13. Each standard and supporting document produces its definition and its leaf

Each `def:Standard` carries `OID`, `Name`, `Type`, `PublishingSet`, `Version`
and `Status`, in that order. `PublishingSet` must be declared for a `CT`
standard and must not be declared for an `IG` standard. Each document produces
one `def:leaf` whose `ID` is its generated identifier, whose `xlink:href` is
the document's literal `link`, and whose `def:title` is its `title`. A
document's kind decides where else it is listed.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::unused_terminology_is_omitted_and_missing_document_links_do_not_read_study_files`, `rust/crates/yamaa-core/tests/schema_submission.rs::structural_findings_are_complete_ordered_and_only_supported_causes_are_portable`.

- a document of kind `annotated_crf` is also listed in `def:AnnotatedCRF`;
- one of kind `supplemental` is also listed in `def:SupplementalDoc`; and
- one of kind `other` is listed in neither.

Every kind may be referenced from an origin, a method or a comment, and from
a result display or an analysis result's documentation or programming code
([DEFINE-31](#define-31)). `link`
names a file the submission package carries. This profile does not require the
file to exist when the document is generated: supporting documents are
assembled outside this language, and a document generated before its annotated
case report form is finished is still the correct document. A `PublishingSet`
declared on an `IG` standard or absent from a `CT` standard fails validation.

<a id="define-14"></a>

### DEFINE-14. At most one annotated case report form is declared, and a value collected from a person requires one

At most one document may declare kind `annotated_crf`, and one must be declared
whenever any column's origin is `Collected` from an `Investigator` or a
`Subject`. A document reference on such an origin that names any other document
fails. More than one document of kind `annotated_crf`, or none where this rule
requires one, fails validation.

Fails: `origin_document_missing`, `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::two_annotated_crfs_are_not_permitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_collected_origin_without_an_annotated_crf_is_refused`.

[SUB-17](submission.md#sub-17) names that document and, under `context: Other`,
derives the reference to it, and neither is possible unless exactly one exists.
More than one
`annotated_crf` document fails with `value_not_permitted`; none where a
collected origin requires one fails with `origin_document_missing`.

<a id="define-15"></a>

### DEFINE-15. Each dataset produces one `ItemGroupDef`, whose `Purpose` follows from its model

Each dataset produces one `ItemGroupDef` with the attributes and children the
table below lists. `Purpose` is derived from the model alone, not declared:
`Tabulation` for `SDTM` and `SEND`, and `Analysis` for `ADaM`. `Domain` is
`submission.domain`, defaulting to the specification's `domain`, and is omitted
for `ADaM`.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::the_ae_metadata_only_document_has_its_fixed_time_and_independent_structure`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::the_vs_metadata_only_document_has_its_fixed_time_and_independent_structure`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_non_standard_dataset_is_marked_non_standard_and_names_no_guide`.

| Attribute or child | Source |
|---|---|
| `OID` | generated |
| `Name` | the specification's `domain`, per [DEFINE-5](#define-5) |
| `Domain` | `submission.domain`, default `domain`; omit for `ADaM` |
| `Purpose` | `Tabulation` for `SDTM` and `SEND`, `Analysis` for `ADaM` |
| `SASDatasetName` | the specification's `domain`, when [DEFINE-22](#define-22) admits it |
| `Repeating` | `Yes` when `submission.repeating`, else `No` |
| `IsReferenceData` | `Yes` when `submission.reference_data`, else `No` |
| `def:Structure` | `submission.structure` |
| `def:ArchiveLocationID` | the dataset's generated leaf identifier, when it has a leaf |
| `def:StandardOID` | the generated identifier of the guide [DEFINE-6](#define-6) binds the dataset to; omitted for a non-standard dataset |
| `def:IsNonStandard` | `Yes` when `submission.non_standard`, else omitted |
| `def:CommentOID` | generated, when `submission.comment` is declared |
| `def:HasNoData` | `Yes` when `submission.has_no_data`, else omitted |
| `Description` | `submission.label` |
| `ItemRef` | one per `output.columns` entry that produces one ([DEFINE-18](#define-18)), in `OrderNumber` order |
| `def:Class` | `submission.class`, with `def:SubClass` when declared |
| `def:leaf` | the file the package carries, per [DEFINE-16](#define-16) |

A declared `Purpose` could only agree with or contradict the derivation.

<a id="define-16"></a>

### DEFINE-16. A dataset's leaf names its transport file or its artifact, relative to the document and inside its directory

A validated dataset's `def:leaf` names the file the package carries for it:
its transport file beside the document when the selected section declares
`transport` ([JSON-1](dataset-json.md#json-1)), and otherwise the artifact its
specification produces. `xlink:href` is that path relative to the directory
holding the generated document, written with `/` separators, and `def:title`
is the path's final component. The href must not begin with a parent
traversal. A dataset with no data and no transport file has no leaf.

Fails: `artifact_outside_document`.
Evidence: `sdtm-dm-metadata`, `sdtm-dm-xpt`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::inherited_original_output_supplies_both_json_capture_and_publication_target`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_transport_file_is_published_beside_the_document_wherever_its_dataset_is`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_artifact_outside_the_documents_directory_is_refused_without_a_transport`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_artifact_outside_the_define_directory_is_refused`.

Reproduced export instead uses only its literal `submission.artifact` link,
or no leaf when omitted ([DEFINE-30](#define-30)); its executable output does
not determine the link.

A document that pointed outside its own directory would describe a file that is
not in the package it belongs to. Without a transport, an artifact path that
cannot be expressed relative to the document's directory without a parent
traversal fails with `artifact_outside_document` at the dataset's `specs`
entry, reporting the dataset as `dataset`, the artifact's `output.path` as its
specification writes it as `artifact`, and the document's path as the project
writes it as `define`. A transport file is the document's sibling, so its href
is its file name wherever the artifact is published.

In validated mode, the leaf names a file this language produced: the artifact
[Output](../output.md) publishes, the Dataset-JSON file
[Dataset-JSON](dataset-json.md) writes or the SAS Transport file
[SAS Transport](xpt.md) writes. A document that named any other container would
assert a file no run wrote. When a transport is requested, a dataset declared
to have no data has no file ([JSON-3](dataset-json.md#json-3)); its
`ItemGroupDef` carries neither `def:ArchiveLocationID` nor `def:leaf`, as
[DEFINE-4](#define-4) admits for `def:HasNoData`.

<a id="define-17"></a>

### DEFINE-17. A dataset declared to have no data carries a comment that says why

A `def:HasNoData` dataset must declare `submission.comment`, which states why a
planned dataset holds no records. A `has_no_data` dataset with no
`submission.comment` fails validation, reporting the dataset.

Fails: `submission_metadata_missing`.
Evidence: `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_dataset_without_data_needs_a_comment`, `rust/crates/yamaa-core/tests/schema_submission.rs::dataset_defaults_and_comment_survive_original_document_drop`.

<a id="define-18"></a>

### DEFINE-18. Each output column produces one numbered `ItemRef`, keyed by its position in `keys`

Each `output.columns` entry produces one `ItemRef` inside its `ItemGroupDef`,
except a column that reproduced metadata omits ([DEFINE-30](#define-30)),
with the attributes the table below lists, and `OrderNumber` is written for
every `ItemRef`. The `ItemRef`s are written in `OrderNumber` order, which is
`output.columns` order unless reproduced metadata declares `order`
([DEFINE-30](#define-30)). `KeySequence` describes the non-missing row identity `keys`
asserts ([OUTPUT-3](../output.md#output-3)) and is not
independently declarable: membership in `keys` determines it, and using a
sponsor variable in `output.order_by` or a `unique` verification does not
independently assign it.

Evidence: `sdtm-dm-metadata`, `sdtm-dm-no-deaths`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::fixed_time_xml_has_one_column_list_two_ordered_item_and_where_references`, `python/tests/installed/installed_public_define.py::test_ae_vs_independent_fixed_time_schema_and_structure`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_omitted_column_writes_no_item_ref_or_item_def_and_the_rest_number_from_one`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::declared_orders_write_the_item_refs_in_that_order_with_the_declared_numbers`.

| Attribute | Source |
|---|---|
| `ItemOID` | the column's generated identifier |
| `OrderNumber` | the column's one-based position among the `output.columns` entries that produce an `ItemRef`, or the `order` reproduced metadata declares ([DEFINE-30](#define-30)) |
| `Mandatory` | `Yes` when the mandatory resolution of [SUB-10](submission.md#sub-10) is true, else `No` |
| `KeySequence` | one-based position in `keys`; omit when not a key |
| `Role` | `submission.role`, when declared |
| `MethodOID` | generated method identifier; omit without `submission.method` |
| `def:HasNoData` | `Yes` when the column has no data ([SUB-28](submission.md#sub-28)), else omitted |

The [output identity example](../output.md#output-3) shows how to
use those variables when some values may be missing.

`OrderNumber` is written for every `ItemRef` of a container or for none of
them, and this profile writes it for every one. Define-XML makes the attribute
optional and falls back to document order; writing it always means the
presented order survives any later reordering of the document.

<a id="define-19"></a>

### DEFINE-19. Each column produces one base `ItemDef` carrying one `def:Origin` per declared origin

Each column that produces an `ItemRef` ([DEFINE-18](#define-18)) produces one
base `ItemDef` with the attributes and children the table below lists; per-value definitions use the same mapping with the
effective metadata of [DEFINE-27](#define-27). A base or per-value `ItemDef`
carries one `def:Origin` per origin, in declaration order, each with `Type`
and, when [Submission metadata](submission.md) admits one, `Source`. A
`def:DocumentRef` carries `leafID`, the referenced document's generated leaf
identifier.

Evidence: `sdtm-dm-metadata`, `sdtm-lb-metadata`, `schema-column-origins`, `rust/crates/yamaa/tests/language_header.rs::a_column_declares_several_origins`, `rust/crates/yamaa-application/tests/file_resources.rs::lb_value_metadata_uses_original_published_bytes_and_family_valid_completion`.

| Attribute or child | Source |
|---|---|
| `OID` | generated |
| `Name` | the column name |
| `DataType` | the resolved submission data type of [SUB-6](submission.md#sub-6) |
| `Length` | the length of [SUB-7](submission.md#sub-7), [SUB-24](submission.md#sub-24) or [SUB-25](submission.md#sub-25), when one applies |
| `SignificantDigits` | `submission.significant_digits`, when [Submission metadata](submission.md) admits it |
| `SASFieldName` | the column name, when [DEFINE-22](#define-22) admits it |
| `def:DisplayFormat` | `submission.display_format`, or the format [SUB-25](submission.md#sub-25) fixes |
| `def:CommentOID` | generated, when `submission.comment` is declared |
| `Description` | the column's `label`, or a per-value definition's entry `label` when declared; a reproduced per-value definition's entry `label` only ([DEFINE-30](#define-30)) |
| `CodeListRef` | the bound codelist's generated identifier, when declared |
| `def:Origin` | one per declared origin, per the origin mapping below |

[SUB-15](submission.md#sub-15) derives the origin `Source` for the `ADaM`
model. Each `def:Origin`'s `Description` child carries that origin's
`description` when declared, and one `def:DocumentRef` child follows per
document reference of that origin in declaration order, including the
page-less annotated case report form reference
[SUB-17](submission.md#sub-17) derives under `context: Other` when a
collected origin declares none. A validated value-level entry without
origins of its own takes the column's ([DEFINE-27](#define-27)).
Rows with different provenance use the separate per-value definitions of
[DEFINE-27](#define-27).

A `def:DocumentRef` carries one `def:PDFPageRef` child when `pages` is
declared, carrying `PageRefs`, `Type`, and `Title` when declared.

<a id="define-20"></a>

### DEFINE-20. Each codelist produces one `CodeList`, whose item elements follow from whether its items declare decodes

Each codelist produces one `CodeList` carrying `OID`, `Name`, `DataType`, then
`def:StandardOID` when `standard` is declared, then `def:IsNonStandard="Yes"`
when `standard` is not declared and the codelist declares `items`, then
`SASFormatName` from `format_name` when declared. A codelist declaring `items`
produces `CodeListItem` children when its items declare `decode` and
`EnumeratedItem` children when they do not. A codelist declaring `external`
carries one `ExternalCodeList` child and no items.

Evidence: `sdtm-dm-metadata`, `sdtm-ae-coding`, `sdtm-ta-sponsor-epoch`, `python/tests/installed/installed_public_define.py::test_ae_vs_independent_fixed_time_schema_and_structure`.

A codelist declaring `items` therefore always carries exactly one of
`def:StandardOID` and `def:IsNonStandard`, the choice Define-XML requires of
it. An external codelist naming no standard carries neither:
`def:IsNonStandard` marks items a sponsor defined, and an external codelist has
none. [CT-5](terminology.md#ct-5) requires the `decode` choice to be uniform
within a codelist, so the element is determined rather than chosen per item.

Each item carries `CodedValue` from `value`, `Rank` when declared, and
`def:ExtendedValue="Yes"` when `extended`. A `CodeListItem` carries a `Decode`
child holding the decode text, and either element carries an `Alias` child with
`Context="nci:ExtCodeID"` when the item declares `alias`, which an extended
item never does ([CT-6](terminology.md#ct-6)). Items keep the
codelist's declaration order, and `OrderNumber` is not written: the document
order already presents them in it, and a second ordering could disagree.

A codelist declaring `alias` carries an `Alias` child with
`Context="nci:ExtCodeID"` after its items. The `ExternalCodeList` child carries
`Dictionary`, `Version`, and `href` when declared. `SASFormatName` carries
`format_name` as declared, which begins with `$` exactly when the codelist is
`text` ([CT-12](terminology.md#ct-12)).

<a id="define-21"></a>

### DEFINE-21. Methods and comments produce `MethodDef` and `def:CommentDef` elements, and all translated text carries one language

Each declared method produces one `MethodDef` carrying `OID`, `Name` from
`method.name` or, when omitted, `<dataset.id>.<column>`, and `Type` from
`method.type`, in that order. Each declared comment produces one
`def:CommentDef` carrying its identifier. A reproduced method or comment that
declares `oid` produces one definition however many elements declare it with
the same definition ([DEFINE-10](#define-10)), written where the first of them
would write its own. Every `Description` and
`Decode` holds exactly one `TranslatedText` whose `xml:lang` is the document's
`language`, and one document carries one language.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::fixed_time_xml_has_one_column_list_two_ordered_item_and_where_references`, `rust/crates/yamaa-core/tests/schema_submission.rs::structural_findings_are_complete_ordered_and_only_supported_causes_are_portable`.

A `MethodDef`'s `Description` child carries the method's `description`, one
`FormalExpression` child follows when `expression` is declared, carrying
`Context` and the code as its text, and one `def:DocumentRef` child follows per
declared document reference. A `def:CommentDef` carries a `Description` child
holding the comment text, and one `def:DocumentRef` child per declared document
reference.

<a id="define-22"></a>

### DEFINE-22. Transport names are written only when they match the SAS transport name patterns

`SASDatasetName` is written only for a dataset name matching
`^[A-Z][A-Z0-9]{0,7}$`, and `SASFieldName` only for a column name matching
`^[A-Z][A-Z0-9_]{0,7}$`. A name its pattern refuses is omitted, and fails
instead under `Submission` or when the section sets `transport: xpt`. Define-XML
carries a long `Alias` element, which [DEFINE-26](#define-26) defers.

Fails: `sas_name_too_long`.
Evidence: `sdtm-dm-metadata`, `negative-transport-name-too-long`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_domain_longer_than_eight_characters_is_refused_under_submission`.

A transport name this rule does not admit fails with `sas_name_too_long` at the
dataset's or column's position, reporting the name and the pattern; lower case,
a leading underscore and a ninth character are all refused.

## Serialization

<a id="define-23"></a>

### DEFINE-23. The document's bytes are fixed: UTF-8, one element per line, two-space indentation and fixed escapes

The document is encoded UTF-8 and carries no byte-order mark; `U+000A`
terminates every line, including the last, and `U+000D` is never written. The
remaining byte-level decisions are fixed exactly as the list below states. The
bytes are fixed rather than the information: the same inputs produce
byte-identical files, and a golden document is compared byte for byte.

Evidence: `sdtm-dm-metadata`, `rust/tests/host-fixtures/cases.json::newlines-in-text`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::where_values_keep_their_text_with_xml_escaping`.

- the first line is `<?xml version="1.0" encoding="UTF-8"?>`;
- a declared `stylesheet` produces a second line,
  `<?xml-stylesheet type="text/xsl" href="..."?>`, and no stylesheet produces
  no line;
- each element begins on its own line, indented by two spaces per level of
  nesting below `ODM`, which is unindented;
- an element whose content is text carries that text between its tags on one
  line, opening tag, text, and closing tag together. Text that holds a
  `U+000A` therefore spans lines, and the following element still begins at
  its own indentation;
- an element with no content is written as a self-closing tag with no space
  before `/>`;
- attributes are written on the element's opening line, separated by one
  space, each as `Name="value"`, in the order this profile's mapping tables
  list them, with namespace declarations first on `ODM`;
- in an attribute value, `&` is written `&amp;`, `<` is written `&lt;`, `>` is
  written `&gt;`, and `"` is written `&quot;`;
- in text content, `&` is written `&amp;`, `<` is written `&lt;`, and `>` is
  written `&gt;`; and
- no comment, processing instruction other than the stylesheet, CDATA section,
  or blank line is written.

<a id="define-24"></a>

### DEFINE-24. Generated text never carries a carriage return or tab, nor a line feed in an attribute value

`U+000D` and `U+0009` must not appear in any generated attribute value or text,
and `U+000A` must not appear in an attribute value. A `U+000D`, a `U+0009` or
an attribute-value `U+000A` in generated text fails.

Fails: `untransportable_text`.
Evidence: `rust/tests/host-fixtures/cases.json::column-label`, `rust/crates/yamaa-application/tests/file_resources.rs::xml_text_findings_keep_field_owners_inheritance_and_render_order_in_both_hosts`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::a_tab_in_generated_text_is_untransportable_at_the_field_that_holds_it`.

The failure reports `untransportable_text` with the declaration that carries
the character.

<a id="define-25"></a>

### DEFINE-25. A complete document, valid against the selected Define-XML schema, is published atomically or not at all

The generated document must be valid against the selected Define-XML schema,
extended by the ARM v1.0 schema when the document carries result displays; a
document that is not fails, reporting the schema violation, and the run
publishes nothing. Publication follows the procedure of
[OUTPUT-8](../output.md#output-8) exactly: the complete
document is written to a temporary regular file in the target's directory,
flushed and closed, and atomically moved onto the target. A failure leaves the
previous file and no residue.

Fails: `value_not_permitted`, `publication_failed`.
Evidence: `rust/crates/yamaa-application/src/file_define/publication_failure_tests.rs::later_publication_failure_preserves_prior_bytes_original_cause_and_stopped_remainder`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::a_creation_time_outside_the_schema_facet_is_a_schema_violation`, `python/tests/installed/installed_public_define.py::test_source_version_defaults_and_schema_facet_failures`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_refuses_a_non_standard_dataset`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_refuses_a_declared_column_without_data`, `tools/tests/test_stage_define_inputs.py::test_staged_oracles_validate_goldens_and_reject_invalid_artifacts`.

Conformance to this profile is not a substitute for schema validation: an
implementation that produces a schema-invalid document has a defect regardless
of which rule it satisfied. A schema-invalid document fails with
`value_not_permitted`. Generation completes before publication begins, so a
failure at any point publishes nothing. A failed publication fails under
[OUTPUT-8](../output.md#output-8) with `publication_failed`.

A Define-XML schema alone has no place for `arm:AnalysisResultDisplays`: the
root schema of a document that carries it is the ARM v1.0 schema bound to the
selected release, which extends that release's schema with the one element
([DEFINE-31](#define-31)). Without that element the document is valid against
the Define-XML schema itself.

A declaration the selected release cannot represent fails under this rule
too, before any document is generated: a Define-XML 2.0 document refuses the
facts [DEFINE-29](#define-29) lists with `value_not_permitted`, its `detail`
`schema_violation: Define-XML 2.0 representation` and its `value` naming the
fact, at the path of the declaration that states it. An origin is refused at
its own `type` or `source`, and `value` is that type or source as declared.
A project check reports the same origin refusal earlier, at the
specification's own path, before any document is composed
([PROJECT-7](project.md#project-7)).

## Scope

<a id="define-26"></a>

### DEFINE-26. Constructs outside the profile cannot be declared and are never generated

The constructs listed below are outside this profile and are never generated.
None has a field or a value in the schema that
[Submission metadata](submission.md), [Controlled terminology](terminology.md)
and the project schema close, so a specification that tries to declare one
fails schema validation. An `Alias` carrying a transport name longer than
eight characters is not generated, and a `FormalExpression` is generated only
from a declared `method.expression`, never from a derivation.

Evidence: `rust/crates/yamaa-core/tests/schema_submission.rs::unknown_and_non_text_fields_preserve_exact_occurrences_without_fabricated_causes`, `rust/crates/yamaa-core/tests/schema_submission.rs::complete_nested_metadata_retains_integer_text_documents_pages_and_order_after_drop`.

- A sponsor value of an analysis result's `reason` or `purpose`: Analysis
  Results Metadata 1.0 publishes both codelists as extensible, and this
  profile admits their published values only ([DEFINE-31](#define-31))
- Programming code read from a program file into `arm:Code`: `code` is
  written in the declaration, and a program file is cited as a document
  ([DEFINE-31](#define-31))
- Split datasets and their `Alias` domain description: a specification that
  produces one dataset in several files
- `def:IsNonStandard` on a column: the supplemental qualifier dataset a
  non-standard variable belongs in under `Submission`
- A comment on a standard, codelist, or metadata version: a shared comment
  identifier space
- A value-level definition selected by several where clauses, joined by OR,
  or by a band between two bounds on one where column: each entry has one
  `def:WhereClauseRef` and one `def:WhereClauseDef`, and each where column of
  its clause one `RangeCheck` ([DEFINE-27](#define-27),
  [SUB-12](submission.md#sub-12))
- `FirstPage` and `LastPage` on a `def:PDFPageRef`: a page range. A page
  reference carries `PageRefs`, `Type` and an optional `Title` only
- Several `FormalExpression` elements on one `MethodDef`: a method declares
  at most one `expression` ([SUB-21](submission.md#sub-21))
- A pooled method or comment in validated metadata, one definition several
  elements reference: a method or comment is declared on the element it
  describes, and no other element can cite it. Each validated column or
  value-level entry with a method has its own `MethodDef`, and each output
  dataset, column or entry with a comment its own `def:CommentDef`, even when
  their declarations are identical. Reproduced metadata shares a definition
  only through a declared `oid` ([DEFINE-30](#define-30))
- `RoleCodeListOID` on an `ItemRef`: a codelist of roles. An `ItemRef`
  carries a `Role` from the closed role list ([SUB-9](submission.md#sub-9))
  and no codelist for it

The long transport-name `Alias` and `FormalExpression` have a declaration but
no generation, and each is stated rather than silently dropped. For a name
longer than eight characters [DEFINE-22](#define-22) omits the attribute and
fails under `Submission` or `transport: xpt` instead.

<a id="define-27"></a>

### DEFINE-27. A column's value-level entries generate one value list, and a where clause and a definition per entry

For each column declaring `value_level` entries
([SUB-12](submission.md#sub-12)), the document generates one
`def:ValueListDef` with one `ItemRef` per entry, in entry order, each carrying
a `def:WhereClauseRef`; one `def:WhereClauseDef` per entry with one
`RangeCheck` per where column; and one per-value `ItemDef` per entry carrying
the column's metadata merged with the entry's, or in reproduced mode the
entry's own ([DEFINE-30](#define-30)). The column's own `ItemDef` carries the
`def:ValueListRef`.

Evidence: `sdtm-lb-metadata`, `adam-adlb-metadata`, `sdtm-suppdm-metadata`, `schema-value-level-not-equal`, `schema-value-level-not-in`, `schema-value-level-ordered`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_value_level_entry_selecting_no_row_has_no_data`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_reproduced_value_level_entry_writes_only_its_declarations_beside_the_column_name_type_and_mandatory`, `rust/crates/yamaa/tests/language_header.rs::a_value_level_entry_selects_the_rows_that_hold_none_of_its_values`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::fixed_time_xml_has_one_column_list_two_ordered_item_and_where_references`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::several_values_are_a_membership_and_several_columns_one_range_check_each`, `rust/crates/yamaa-application/tests/file_resources.rs::lb_value_metadata_uses_original_published_bytes_and_family_valid_completion`.

The mapping to Define-XML is fixed:

- each value-list `ItemRef` carries `ItemOID`, the entry's per-value
  identifier ([DEFINE-9](#define-9)), `OrderNumber`, the entry's one-based
  position among the column's entries, `Mandatory`, the entry's resolution of
  [SUB-10](submission.md#sub-10), `Role` and `MethodOID` when its merged
  metadata declares them, and `def:HasNoData="Yes"` when the entry has no
  data ([SUB-28](submission.md#sub-28)). The column's `ItemDef` carries its
  single `def:ValueListRef` after its `def:Origin` elements;
- each `def:WhereClauseDef` carries one `RangeCheck` per where column, in the
  clause's written order: `Comparator="EQ"` when the column lists one value
  and `"IN"` when it lists several, or the comparator the column writes,
  `"NE"`, `"NOTIN"`, `"LT"`, `"LE"`, `"GT"` or `"GE"`
  ([SUB-12](submission.md#sub-12)), `SoftHard="Soft"`, `def:ItemOID` the
  where column's `ItemDef` identifier, and one `CheckValue` per listed value
  in written order. An ordered column's one value is written as written,
  except that an `ADaM` date, datetime or time, which its `ItemDef` types as
  `integer` ([SUB-25](submission.md#sub-25)), writes its SAS number: `GE
  2026-03-01` on a date writes `24166`; and
- each per-value `ItemDef` maps the merged metadata as
  [DEFINE-19](#define-19) does, with the entry's `label` as its
  `Description` when declared and its `Length` from
  [SUB-24](submission.md#sub-24) or [SUB-7](submission.md#sub-7). An entry
  that declares origins writes its own, and a validated one that declares
  none takes the column's whole list. Associated methods, comments and
  document references remain resolvable.

A transport file carries columns, not value-level definitions
([JSON-9](dataset-json.md#json-9), [XPT-4](xpt.md#xpt-4)); a value-level
`Length` measured over an entry's rows appears in the document alone.

This linkage follows the bundled authoritative Define-XML 2.1 XSDs:
`define-extension.xsd` places `ValueListRef` only in the `ItemDef` extension
(at most one), places `WhereClauseRef` in the `ItemRef` extension, and orders
all `ValueListDef` elements before all `WhereClauseDef` elements ahead of the
ordinary metadata definitions. `define-ns.xsd` requires a nonempty value list
with unique item references and order numbers, and a where clause with at
least one `RangeCheck`. A value list per entry linked from the column's
`ItemRef` would violate these constraints.

A generator that cannot consume an entry fails loudly, naming its column and
entry; it never quietly omits a per-value definition.

<a id="define-28"></a>

### DEFINE-28. Define-XML requires the project's `study` and the section's `standards` and `define`, which a run does not

A Define-XML generation whose project declares no `study`, or whose selected
section declares no `standards` or `define`, fails with
`missing_required_field` before any specification is captured. It reports
every missing field at once, each at its project path such as `study`,
`sdtm.standards` or `sdtm.define`. A run of the same project needs none of them
and does not report them.

Fails: `missing_required_field`.
Evidence: `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_reports_the_missing_study_guide_and_target_before_reading_specifications`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_reports_a_missing_target_before_reading_specifications`, `rust/crates/yamaa-core/src/front_end/v1/project.rs::define_requires_a_1_0_section_s_standards_by_their_1_0_name`.

A project can therefore list its specifications and run them before its
submission metadata is ready ([PROJECT-3](project.md#project-3)). An empty
`standards` list leaves the section without the guide
[DEFINE-6](#define-6) requires, and fails at `<section>.standards` as an
omitted one does. A project submission refuses a selected section that the project does not declare
before any Define-XML generation, with `unknown_section`
([PROJECT-14](project.md#project-14)).

<a id="define-29"></a>

### DEFINE-29. A section selects its Define-XML release explicitly, retaining 2.1.11 when omitted

A section's `define_version` selects `2.0.0` or any published 2.1 release
from `2.1.0` through `2.1.11`; omission retains 2.1.11 and its existing bytes.
The document preserves the selected release in `def:DefineVersion`. All 2.1
releases use the v2.1 namespace and the maintained 2.1 enumeration schema,
as [CDISC recommends](https://www.cdisc.org/standards/data-exchange/define-xml/define-xml-v2-1)
when it updates that schema's controlled terminology. The project schema
refuses unpublished and incomplete versions. A 2.0 document requires one Final
implementation guide and refuses facts it cannot represent, but omits a
codelist source's terminology standard. Declarations requiring 2.1 constructs
fail before publication, under [DEFINE-25](#define-25).

Evidence: `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_define_2_0_document_preserves_its_declared_metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/define_versions.json::define_2_1_0_preserves_the_selected_release`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/define_versions.json::define_2_1_11_preserves_the_selected_release`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_refuses_a_non_standard_dataset`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_refuses_a_declared_column_without_data`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_omits_the_measured_no_data_flag`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_writes_an_extended_item_of_a_published_codelist_without_its_terminology_standard`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_reproduced_define_2_0_writes_an_extended_item_of_a_published_codelist_without_its_terminology_standard`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_refuses_a_second_implementation_guide`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::analysis_results_compose_under_define_2_0`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_refuses_other_origins_for_adam`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_refuses_an_origin_source_other_than_its_implicit_one`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_writes_a_collected_origin_from_a_vendor_as_edt`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_reproduced_define_2_0_writes_a_collected_origin_from_a_vendor_as_edt`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_refuses_a_collected_origin_from_the_subject`.

For 2.0 these mappings replace the version-specific fields of the 2.1 tables
above. The document uses the v2.0 namespace, `DefineVersion="2.0.0"`,
`StandardName` and `StandardVersion` on `MetaDataVersion`, and a dataset
`Class` attribute. ADaMIG, SDTMIG and SENDIG are written as ADaM-IG, SDTM-IG
and SEND-IG. It omits 2.1-only standards, standard references, codelist
non-standard flags, context, origin sources and the `def:HasNoData` that
[SUB-28](submission.md#sub-28) measures on an `ItemRef`. Under SDTM and ADaM,
Collected from Vendor is written as eDT and Collected from Investigator as
CRF; Derived, Assigned, Protocol and Predecessor retain their names, and Other
and Not Available are refused. Under SEND the admitted
types are Collected, Derived, Other and Not Available, written as COLLECTED,
DERIVED, OTHER and NOT AVAILABLE; Assigned, Protocol and Predecessor are refused.
Under SDTM and ADaM an Other origin therefore has no 2.0 representation: a
validated section records it with a 2.1 release, which omission selects, and
reproduced metadata declares the type the received 2.0 document recorded,
with that document's method or predecessor. One judgement decides what 2.0
records of an origin, for the document and for a project check, which reports
each origin it cannot record before composition
([PROJECT-7](project.md#project-7)).

Multiple guides, a subclass, a dataset's HasNoData, a column's or
value-level entry's declared `has_no_data` and a non-standard dataset have no
faithful representation and are refused. A 2.0 origin records no source, so
each 2.0 type implies one, and an explicit origin source is representable only
as the implied source of the type it writes: Investigator for CRF and SEND's
COLLECTED, Vendor for eDT, Sponsor for Derived, Assigned, Protocol or Other.
An eDT origin requires no annotated case report form, which
[SUB-17](submission.md#sub-17) requires only of a value collected from a
person.
Collected from Subject has no 2.0 type that implies it and is refused: a
diary or questionnaire answered on paper is CRF data and one answered
electronically is eDT, and the declaration does not say which. A validated
section records it with a 2.1 release, as it records Other. This version
choice does not change derivations, ordinary metadata validation or the
transport encoders.

A terminology standard that a codelist source names is the one fact a 2.0
document omits rather than refuses. Only implementation guides are counted
against the one guide 2.0 records. The standard still resolves its source's
codelists, so an extended item of an extensible published codelist
([CT-6](terminology.md#ct-6)) is admitted and writes
`def:ExtendedValue="Yes"` ([DEFINE-20](#define-20)), while the document
carries neither a terminology `def:Standard` nor a `def:StandardOID`.
Reproduced metadata ([DEFINE-30](#define-30)) composes through the same 2.0
profile and admits it alike.

Analysis results metadata is generated under every admitted release, 2.0.0
included, in the one `http://www.cdisc.org/ns/arm/v1.0` namespace
([DEFINE-31](#define-31)): Analysis Results Metadata 1.0 extends Define-XML
2.0, and the Define-XML 2.1 specification binds the same extension to 2.1.
The `def` elements inside it take the namespace of the selected release.

Each refusal is a schema violation of the selected release and reports
[DEFINE-25](#define-25), as that rule states. A `define_version` outside the
admitted releases fails as a value the project schema does not permit
([SCHEMA-12](../reference/schema-language.md#schema-12)).

<a id="define-30"></a>

### DEFINE-30. Reproduced metadata is a literal record and makes no assertion about current derivations or data

A dataset's explicit `submission.mode: reproduced` reproduces received
metadata in Define-XML 2.0 under `context: Other` without transport; omission
retains validated mode. Reproduced declarations add no execution constraints;
executable types, keys and explicit verifications remain active. Composition
admits closed metadata, resolves every codelist and document reference, and
requires a schema-valid document. It makes no data-conformance assertion.

Fails: `value_not_permitted`, `submission_metadata_missing`, `unknown_codelist`.
Evidence: `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_reproduced_define_2_0_keeps_declarations_independent_of_execution`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::reproduced_metadata_keeps_explicit_verifications`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_reproduced_value_level_entry_writes_only_its_declarations_beside_the_column_name_type_and_mandatory`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_reproduced_method_or_comment_keeps_its_received_oid_and_identical_definitions_share_one`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_validated_method_declares_no_received_oid`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_validated_comment_declares_no_received_oid`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_validated_dataset_comment_declares_no_received_oid`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_reproduced_where_clause_may_select_on_its_own_column`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::reproduced_links_ignore_current_output_location_and_run_report`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::reproduced_where_clauses_ignore_executable_integer_types_codelist_values_and_row_overlap`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::define_2_0_refuses_a_declared_column_without_data`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_validated_column_declares_no_has_no_data`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::reproduced_metadata_refuses_define_2_1`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::reproduced_metadata_refuses_submission_context`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::reproduced_metadata_refuses_transport`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_omitted_column_writes_no_item_ref_or_item_def_and_the_rest_number_from_one`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_omitted_executable_key_composes_and_writes_no_key_sequence`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_column_that_writes_omitted_false_is_described_as_usual`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_omitted_column_declares_no_other_submission_field`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_where_clause_naming_an_omitted_column_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_dataset_whose_every_output_column_is_omitted_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_validated_column_declares_no_omitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::declared_orders_write_the_item_refs_in_that_order_with_the_declared_numbers`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_omitted_column_declares_no_order_beside_columns_that_do`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::once_one_described_column_declares_order_every_one_does`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::two_columns_declaring_one_order_are_refused_at_the_second`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_order_that_is_not_a_positive_number_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_validated_column_declares_no_order`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_reproduced_dataset_s_analysis_where_values_ignore_its_types_and_codelists`.

The mode records what a received document declares, whatever its age.
Define-XML 2.0, `context: Other` and the absence of a transport are the bounds
of this profile, not properties of the received document: a reproduced
dataset in a section that selects a 2.1 release, the `Submission` context or
a transport fails with `value_not_permitted` at its `submission`.

Validated metadata, the default, is checked against what yamaa can see: a
declared origin against the derivation
([SUB-19](submission.md#sub-19)), a length and a mandatory flag against the
values a run completes ([SUB-8](submission.md#sub-8),
[SUB-11](submission.md#sub-11)), and the published data when the section
declares a transport or the specification a run report
([SUB-24](submission.md#sub-24), [SUB-28](submission.md#sub-28),
[PROJECT-15](project.md#project-15)). Neither mode asserts that the document
passes a regulator's or a validator's conformance checks.

The reproduced export requires explicit column `data_type`, `mandatory` and
labels for the columns the document describes. Lengths, formats, origins,
methods and codelists remain as declared.
`key_sequence` declares a reproduced ItemRef key position independently of
executable `keys`, and a column's or value-level entry's `has_no_data` its
ItemRef's `def:HasNoData`, which validated metadata measures instead
([SUB-28](submission.md#sub-28)). `artifact` declares a literal relative
package link; its filename supplies the leaf title. Omission writes no
reproduced artifact leaf. A method's or comment's `oid` declares the
`MethodOID` or `CommentOID` the received document carries
([DEFINE-9](#define-9)), and elements that declare one `oid` with the same
definition share it ([DEFINE-10](#define-10)). These fields are refused on
validated metadata, each with `value_not_permitted` at the `submission` that
declares it. A reproduced value-level entry records one received per-value definition,
so it takes from its column only what that definition cannot omit: the
`ItemDef` `Name`, which is the column's name, and, when the entry declares
none, the column's `data_type` for `DataType` and `mandatory` for the
value-list `ItemRef`'s required `Mandatory`. `OID` is generated
([DEFINE-9](#define-9)) and `SASFieldName` follows the name
([DEFINE-22](#define-22)). Every other field, the `label` that writes
`Description` included, appears on the per-value `ItemDef` or `ItemRef` only
when the entry declares it, and key positions and `has_no_data` never
inherit. A Define-XML
2.0 document has no `def:HasNoData`, so a declared `has_no_data: true` is
refused there ([DEFINE-29](#define-29)), as a dataset's is.

A column's `omitted: true` records that the received document leaves out a
column the output dataset carries. The document writes no `ItemRef` or
`ItemDef` for that column, and the columns it does describe take
`OrderNumber` 1 to n ([DEFINE-18](#define-18)), unless they declare `order`,
below. The output dataset keeps the
column with its type, derivation and verifications, and the column may be one
of the executable `keys`. `omitted: true` is the only field of its
`submission`: another field beside it fails with `value_not_permitted` at
`<column>.submission.omitted`, and the column needs no `label`, `data_type`
or `mandatory`. `omitted: false` declares an ordinary column. A value-level
entry has no such field
([SCHEMA-13](../reference/schema-language.md#schema-13)), and validated
metadata refuses it written at all, with `value_not_permitted` at the
column's `submission`: a validated document describes the dataset it
publishes, which carries every output column, a row identity derived for
`keys` included ([DEFINE-7](#define-7)). A where clause that names an
omitted column fails with `value_level_where_invalid`, reason `omitted`
([SUB-12](submission.md#sub-12)): its `RangeCheck` would name an `ItemDef`
the document does not hold. A dataset whose every output column is omitted
fails with `value_not_permitted` at the `omitted` of its first output column,
since an `ItemGroupDef` requires an `ItemRef` ([DEFINE-25](#define-25)).

A column's `order` records the `OrderNumber` the received document gives the
column's `ItemRef`, where that document lists its `ItemRef`s in another order
than the dataset carries its columns. The `ItemRef`s are written in the
declared order, each with its declared number, gaps included
([DEFINE-18](#define-18)); the output dataset, the `ItemDef`s and the
`MethodDef`s keep `output.columns` order. A dataset need not declare `order`,
and none of its columns then does. Once one column the document describes
declares it, every such column does, since `OrderNumber` is written on every
`ItemRef` of a container or on none: a column without it fails with
`submission_metadata_missing` at its `submission`, reporting `order`. An
omitted column declares none. An `order` that is not a positive integer, or
that repeats the `order` of a column earlier in `output.columns`, fails with
`value_not_permitted` at that `order`. A value-level entry has no such field
([SCHEMA-13](../reference/schema-language.md#schema-13)), and validated
metadata refuses it written at all, with `value_not_permitted` at the
column's `submission`: a validated document describes the dataset it
publishes, and the SDTM implementation guide asks the document's variable
order to follow the dataset's.

The current executable output need not share the document's directory, and
reproduced composition does not read its run report, artifact or inputs.
Publication still protects declared inputs and outputs from being overwritten.
Reproduced declarations do not request unrelated section specifications for
inventory or predecessor assertions. Distinct reproduced datasets may share
an executable output path; each keeps its own literal metadata and artifact link.
Value-level where clauses retain structural and output-column name checks,
but executable types, terminology membership and overlapping row selections
do not constrain a reproduced declaration, and a where clause may name the
column its entry describes, as a received value list keyed by its own column
does ([SUB-12](submission.md#sub-12)). An ordered where column is the
exception: it still must not be `str`, and its value must convert to its
type, since the comparison has nothing else to order by
([SUB-12](submission.md#sub-12)).

The where clause of an analysis dataset that names a reproduced dataset is
checked as a reproduced value-level where clause is: its where columns are
output columns and its structure is whole, while the dataset's types and
codelists do not bind its listed values, which are written as declared
([DEFINE-32](#define-32)).

Reproduced codelists, mandatory flags, lengths, origins and value-level
metadata do not bind execution. They describe received declarations, not the
current derived artifacts. Column types, executable keys, explicit
verifications and all other ordinary checks still apply. Output and
where-clause names remain checked during composition. An independent
metadata comparison and data audit assess those separate claims. No
reference XML is read or passed through by this profile, and no transport
is published or certified.

## Analysis results

<a id="define-31"></a>

### DEFINE-31. Each result display produces one `arm:ResultDisplay` and each of its results one `arm:AnalysisResult`, in the ARM v1.0 schema's order

Each result display an `adam` section declares produces one
`arm:ResultDisplay` inside the document's one `arm:AnalysisResultDisplays`,
and each result of a display one `arm:AnalysisResult`, with the attributes and
children the tables below list, in the order the ARM v1.0 schema gives them.
Every element and attribute of Analysis Results Metadata 1.0 has one source: a
declaration, or a derivation from the datasets the section composes. `reason`
and `purpose` take the published values only.

Evidence: `schema-analysis-results`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::analysis_results_compose_under_define_2_0`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_analysis_reason_outside_the_published_values_is_not_permitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_tab_in_programming_code_is_untransportable`, `tools/tests/test_stage_define_inputs.py::test_staged_oracles_validate_goldens_and_reject_invalid_artifacts`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_analysis_result_writes_listed_and_excluded_values_a_documented_join_and_the_first_parameter_dataset`.

| `arm:ResultDisplay` attribute or child | Source |
|---|---|
| `OID` | `oid`, otherwise generated ([DEFINE-9](#define-9)) |
| `Name` | `name` |
| `Description` | `description` |
| `def:DocumentRef` | one per entry of `documents`, in written order |
| `arm:AnalysisResult` | one per entry of `results`, in written order |

| `arm:AnalysisResult` attribute or child | Source |
|---|---|
| `OID` | `oid`, otherwise generated ([DEFINE-9](#define-9)) |
| `ParameterOID` | derived, when a listed dataset outputs `PARAMCD` ([DEFINE-32](#define-32)) |
| `AnalysisReason` | `reason` |
| `AnalysisPurpose` | `purpose` |
| `Description` | `description` |
| `arm:AnalysisDatasets` | one, holding one `arm:AnalysisDataset` per key of `datasets`, in written order |
| `arm:Documentation` | `documentation`, when declared |
| `arm:ProgrammingCode` | `programming_code`, when declared |

`arm:AnalysisDatasets` carries `def:CommentOID`, the generated identifier of
the join comment, when `join` is declared, and that comment produces one
`def:CommentDef` as [DEFINE-21](#define-21) maps a comment. Each
`arm:AnalysisDataset` carries `ItemGroupOID`, its dataset's `ItemGroupDef`
identifier, then one `def:WhereClauseRef` when `where` is declared, naming the
generated `def:WhereClauseDef`, then one `arm:AnalysisVariable` per entry of
`variables`, each carrying `ItemOID`, that column's `ItemDef` identifier. A
dataset that lists no variable writes none.

The where clause of an analysis dataset produces one `def:WhereClauseDef`
with one `RangeCheck` per where column, mapped as
[DEFINE-27](#define-27) maps a value-level entry's. A value listed for a where
column that is not `str` is written as an ordered value is: as written, except
that an `ADaM` date, datetime or time writes its SAS number.

`arm:Documentation` carries a `Description` holding the documentation text,
then one `def:DocumentRef` per document it refers to. `arm:ProgrammingCode`
carries `Context` from `context`, then one `arm:Code` holding `code` and one
`def:DocumentRef` per entry of `documents`. A `def:DocumentRef` carries a
`def:PDFPageRef` when `pages` is declared, as
[DEFINE-19](#define-19) writes one.

`reason` takes one of `SPECIFIED IN PROTOCOL`, `SPECIFIED IN SAP`,
`DATA DRIVEN` and `REQUESTED BY REGULATORY AGENCY`, and `purpose` one of
`PRIMARY OUTCOME MEASURE`, `SECONDARY OUTCOME MEASURE` and
`EXPLORATORY OUTCOME MEASURE`. An analysis that both the protocol and the
statistical analysis plan specify declares `SPECIFIED IN SAP`. Any other
value fails as a value the schema does not permit
([SCHEMA-12](../reference/schema-language.md#schema-12),
[DEFINE-26](#define-26)).

Code, documentation and description text is written exactly as declared and
escaped as [DEFINE-23](#define-23) states: a YAML `|` block keeps its final
line feed and `|-` drops it. A tab or a carriage return in any of it, pasted
code included, fails with `untransportable_text` under
[DEFINE-24](#define-24), and every `Description` carries the document's one
language ([DEFINE-21](#define-21)). A display's `name` and a code's
`context` are attribute values, where a line feed fails too; a fault in one
is reported at the display or the `programming_code` that writes it.

<a id="define-32"></a>

### DEFINE-32. An analysis results source resolves every dataset, column and document it names against the section that declares it

An `adam` section's `analysis_results` lists sources in order, each written
inline or as the path of a `type: analysis_results` file; displays keep source
order, then declaration order. Composition resolves each result's `datasets`
keys against the datasets the section composes, each variable and where
column against that dataset's output columns and each document against the
section's `documents`, and derives `ParameterOID`. A result that reads
several datasets declares `join`, and every result names a variable. No data
is read.

Fails: `unknown_dataset`, `analysis_dataset_invalid`, `duplicate_identifier`, `duplicate_define_identifier`, `missing_required_field`, `value_not_permitted`.
Evidence: `schema-analysis-results`, `negative-analysis-unknown-dataset`, `negative-analysis-where-not-in-codelist`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_analysis_result_over_two_of_the_sections_datasets_composes`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_analysis_dataset_the_section_does_not_compose_is_unknown`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_analysis_variable_or_where_column_its_dataset_does_not_declare_or_output_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_analysis_where_clause_names_no_column_a_value_outside_its_codelist_or_one_that_does_not_convert`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_display_id_or_name_another_source_declares_and_a_result_id_its_display_repeats_are_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_result_reading_two_datasets_needs_its_join_one_variable_and_code_or_documents`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_source_a_display_and_a_result_declare_at_least_one_member_and_code_its_context`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_join_on_a_result_that_reads_one_dataset_is_not_permitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_display_or_an_analysis_result_citing_an_undeclared_document_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_reproduced_dataset_s_analysis_where_values_ignore_its_types_and_codelists`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::analysis_results_under_sdtm_is_an_unknown_field`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_analysis_results_path_naming_a_codelist_file_is_refused_at_its_listing`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_parent_project_s_analysis_results_file_composes_as_the_source_written_inline`, `rust/crates/yamaa-core/src/submission_values.rs::an_analysis_clause_lists_values_of_any_type_and_names_no_column_of_its_own`, `rust/crates/yamaa-core/src/language.rs::referencing_fields_name_the_type_their_file_declares`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_analysis_result_writes_listed_and_excluded_values_a_documented_join_and_the_first_parameter_dataset`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_empty_analysis_join_is_shorter_than_its_minimum_length`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_analysis_variable_or_where_column_a_reproduced_dataset_omits_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_dataset_that_omits_paramcd_is_not_a_result_s_parameter_dataset`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_analysis_where_column_listing_one_value_in_two_spellings_is_refused`.

Only an `adam` section declares `analysis_results`; under `sdtm` or `send`
the field fails as `unknown_field`
([SCHEMA-13](../reference/schema-language.md#schema-13)). A file source is a
path relative to the project file ([PROJECT-4](project.md#project-4)), read
with the project's codelist files whenever the project is prepared, and
admitted against the analysis results schema; a path naming a file of another
type fails as `unexpected_file_type` at its listing,
`adam.analysis_results[<i>]`
([SCHEMA-2](../reference/schema-language.md#schema-2)). An inline source and
a file hold the same fields and lower through one reader. A file read on its
own is checked for its shape only: the names it writes resolve when a section
composes it. `analysis_results` is a list, so a later project file replaces
it whole ([PROJECT-16](project.md#project-16)).

A declaration is reported at its position in the section,
`adam.analysis_results[<s>].displays[<d>].results[<r>]`, whichever kind of
source holds it. A schema finding names the display and the result by
identity, in the notation of [LAYER-9](../inheritance.md#layer-9): the display
by its `name` and the result by its `id`, as
`adam.analysis_results[0].displays['Table 1'].results.R1.reason` does. A
source declares at least one display, a display at least
one result and a result at least one dataset; an empty `displays`, `results`
or `datasets` fails with `missing_required_field` there, naming the field.

A display's `id` is unique among the displays of every source of the section
([DEFINE-10](#define-10)); a repeat is refused, never taken as a replacement.
A display's `name` identifies the display in the report, so it is unique among
them too: a repeat fails with `duplicate_define_identifier` at the later
`name`, reporting it as `identifier`. A result's `id` is unique within its
display, and a repeat fails with `duplicate_identifier` at the later `id`. A
dataset is unique within its result as a key of `datasets`.

A `datasets` key naming no dataset the section composes fails with
`unknown_dataset` at that key, `datasets.<dataset>`, reporting the key as
`dataset`, the section as `section` and the identifiers of the datasets it
composes, in entry order, as `datasets`. A document naming no declaration
fails with `unknown_document` ([DEFINE-8](#define-8)).

Each entry of `variables` and each where column is a declared column of the
dataset that its `output.columns` carries and its document defines: a column
that reproduced metadata leaves out has no `ItemDef` to name
([DEFINE-30](#define-30)). A fault fails with
`analysis_dataset_invalid`, reporting the display and the result by their
identifiers as `display` and `result`, the dataset as `dataset` and the
`reason`: at `datasets.<dataset>.variables[<i>]`, with the column as
`variable`, `undeclared`, `not_output`, `omitted` or, for a column an earlier
entry lists, `duplicate_variable`; and at `datasets.<dataset>.where.<where column>`,
or at `where` itself for a clause without columns, with the where column as
`where_column`, one of the reasons below.

A where clause is the value-level form, checked by the checker of
[SUB-12](submission.md#sub-12), and admits what Analysis Results Metadata 1.0
admits in a selection: the comparators `EQ`, `NE`, `LT`, `LE`, `GT`, `GE`,
`IN` and `NOTIN`, joined by AND only, and at most one clause on a dataset. An
analysis dataset describes no column of its own and its clause selects no
rows in a build, so the check differs in three ways:

- no where column is the clause's own, and no clause overlaps another;
- a where column that is not `str` may list values, as `AVISITN: ['24']`
  does: each value is written as text, must convert to the column's type under
  [VALUE-3](../values.md#value-3), and is checked against no codelist. Two
  values that convert to one, as `'24'` and `'24.0'` do, list it twice
  (`duplicate_value`); and
- a `str` where column's listed values are checked against its codelists
  exactly as a value-level clause's are.

The reasons are therefore `no_columns`, `undeclared`, `not_output`, `omitted`,
`ordered_str`, `no_values`, `value_count`, `duplicate_value`,
`not_in_codelist` and `value_type`, with the `values` and `comparator` that
SUB-12 reports beside them. A dataset in `submission.mode: reproduced` is
checked as a reproduced value-level clause is
([DEFINE-30](#define-30)).

`ParameterOID` is `IT.<dataset>.PARAMCD` for the first dataset of the result,
in written order, whose document defines a column named `PARAMCD`, an output
column that reproduced metadata does not leave out, and is omitted when none
does. It is not declarable: a declared value could only agree with the
result's datasets or contradict them.

A result that lists two or more datasets declares `join`, the comment that
says how they are joined, and fails with `missing_required_field` at `join`
without it. A result that lists one dataset has nothing to join, and its
`join` fails with `value_not_permitted`, reporting the text as `value` and
`a result that reads one dataset joins nothing` as `detail`. A `join` or
`documentation` written as empty text fails as a value shorter than its
minimum length ([SCHEMA-12](../reference/schema-language.md#schema-12)). A
result whose datasets list no variable between them fails with
`missing_required_field` at its `datasets`, naming `variables`; one dataset
of several may list none, as one that only selects the subjects does.
`programming_code` declares `code`, `documents` or both, and `context`
whenever it declares `code`: otherwise it fails with `missing_required_field`
at `programming_code`, naming `code`, or at `programming_code.context`.
`context: Submission` adds no requirement to an analysis result
([DEFINE-4](#define-4)).

One result's findings are reported in this order: a repeated `id`, an empty
`datasets`, `join`, a document it names that is not declared,
`programming_code`, each dataset in written order, its variables before its
where clause, and last a result that names no variable.

Composition never evaluates an analysis where clause against rows. Whether a
dataset that holds more records than a result uses declares a clause, whether
a result on specific parameters selects them by `PARAMCD`, and whether a
clause selects any record, stay with the author.
