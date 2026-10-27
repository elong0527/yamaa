# Define-XML

This file is a profile: it is optional, and it applies only to a build that
selects an `sdtm`, `adam` or `send` submission section of a project file.
Such a build composes the section, the study and the submission metadata of the
specifications the section lists into one deterministic Define-XML document.
The default release is 2.1.11; an explicit `define_version: "2.0.0"` selects
the earlier representation ([DEFINE-29](#define-29)). A domain build never
generates a Define-XML document.

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
bytes. An explicit `created` is a whole-second civil datetime; omission selects
the current UTC time during preparation, and Save retains that time. An omitted
`file_oid` is `study.id`, the selected lower-case section key and
`Define-XML_2.1` (or `Define-XML_2.0` for 2.0.0), joined by dots. An omitted metadata-version ID is
`study.id.section`, and an omitted name is `study.name section metadata`.

Fails: `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/service_build_metadata.rs::clock_is_observed_once_and_calendar_admission_is_owned_by_the_engine`, `python/tests/installed/installed_public_define.py::test_defaults_selection_bound_lists_and_required_acrf_aliases`.

Explicit overrides retain their admitted text, independently of these
defaults. An explicit `created` that is not a whole-second civil datetime fails
with `value_not_permitted`.

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
Evidence: `sdtm-dm-metadata`, `sdtm-ta-sponsor-epoch`, `sdtm-vs-epoch-from-subject-elements`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_published_codelist_without_its_nci_code_is_refused_under_submission`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_published_item_without_its_nci_code_is_refused_under_submission`, `rust/tests/host-fixtures/cases.json::submission-column-label`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_without_its_form_is_refused_under_submission`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_citing_its_form_without_a_page_is_refused_under_submission`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_without_its_form_cites_the_form_without_a_page_under_other`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_citing_its_form_without_a_page_is_admitted_under_other`.

Nothing in this profile is relaxed by `Other`; that context only omits the
requirements a regulator imposes. Under `Submission`:

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

A codelist `alias`, or the `alias` of a published item, missing under
`Submission` fails with `missing_required_field`; a sponsor's extension of an
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
([DEFINE-8](#define-8)). The `sdtm`, `adam` and `send` sections produce
separate documents, and each document carries the controlled-terminology
standards cited by its bound codelists' sources.

Fails: `standard_model_mismatch`, `duplicate_identifier`.
Evidence: `sdtm-dm-metadata`, `schema-section-standards`, `negative-section-standard-model`, `negative-section-standard-repeated`, `rust/crates/yamaa/tests/language_header.rs::a_section_lists_only_specifications_of_its_model`, `rust/crates/yamaa/tests/language_header.rs::a_dataset_binds_the_section_standard_it_selects`, `rust/crates/yamaa-core/src/front_end/v1/project.rs::define_names_a_1_0_section_s_guides_by_their_place_in_standards`, `rust/crates/yamaa-core/src/front_end/v1/project.rs::define_requires_a_1_0_section_s_standards_by_their_1_0_name`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_section_guide_of_another_model_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_section_guide_listed_twice_is_refused`, `rust/crates/yamaa-core/tests/schema_submission.rs::every_shipped_guide_spelling_is_closed_metadata_not_family_composition`.

Composition checks every entry of the list. A guide outside the section's
model fails with `standard_model_mismatch` at `<section>.standards[<i>].name`,
naming the guide in `value`, the section in `section` and its model in
`model`. A name an earlier entry takes fails with `duplicate_identifier` at
the later entry's `name`, naming it in `identifier`. A section without
`standards`, or with an empty list, fails under [DEFINE-28](#define-28).

The document lists every guide of the section in declaration order, whether
or not a dataset binds it, before the terminology standards, and each
`ItemGroupDef` names its dataset's bound guide in `def:StandardOID`
([DEFINE-15](#define-15)). Names are unique within a section, so a guide's
generated identifier stays `STD.<name>` ([DEFINE-9](#define-9)). A guide's
`status`, `Draft`, `Provisional` or `Final`, is its `Status`
([DEFINE-13](#define-13)).

[SUB-3](submission.md#sub-3) checks the bound guide against the model the
specification declares, which a 1.0 section's listing makes the section's,
so a guide this rule admits belongs to it.

<a id="define-7"></a>

### DEFINE-7. Every composed specification declares dataset and column submission metadata and column labels

Every specification a document names must declare `root.submission`, and every
column in its `output.columns` must declare both `label` and
`column.submission`. A specification named by an entry that declares no
`root.submission`, or an output column with no `label` or no
`column.submission`, fails.

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
Evidence: `sdtm-dm-metadata`, `schema-section-standards`, `negative-unknown-standard`, `rust/crates/yamaa/tests/language_header.rs::a_dataset_binds_the_section_standard_it_selects`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_dataset_selecting_a_guide_its_section_lacks_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_origin_citing_an_undeclared_document_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_unknown_per_value_codelist_fails_at_its_row_path`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_per_value_codelist_of_another_type_fails_at_its_row_path`.

A `submission.standard`, `codelist` or document reference naming no
declaration fails with `unknown_standard`, `unknown_codelist` or
`unknown_document`, reporting the referencing position. Guide names compare
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

Evidence: `sdtm-dm-metadata`, `adam-adlb-metadata`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::value_identifiers_survive_unrelated_insertion_and_entry_reordering`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::where_values_keep_their_text_with_xml_escaping`.

| Object | Identifier |
|---|---|
| `Study` | `STDY.<study.id>` |
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

In the `def:Standard` rows, `standard` is an entry of the selected section's
`standards` for an implementation guide ([DEFINE-6](#define-6)), and for a terminology
standard the `standard` of a codelist source whose codelists the document binds
([CT-2](terminology.md#ct-2)). Sources that declare the same `name`,
`publishing_set` and `version` cite one terminology standard and share its
`def:Standard`; two terminology standards with one `publishing_set` fail under
[DEFINE-10](#define-10).

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

Collisions are impossible rather than resolved. Dataset, codelist, standard and
document identifiers are each unique within the study document; a column name
is unique within its specification; a column's entry ids identify its
per-value definitions; and the prefixes are disjoint. Two kinds of object share
one identifier space, `def:leaf`, so a document identifier must not equal a
dataset identifier.

Fails: `duplicate_define_identifier`.
Evidence: `negative-terminology-version-collision`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_document_identifier_equal_to_a_dataset_identifier_collides`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_later_release_of_a_cited_terminology_standard_is_refused_at_its_publishing_set`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::another_standard_name_under_a_cited_publishing_set_is_refused_at_its_publishing_set`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::sources_citing_one_terminology_standard_share_its_one_definition`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/admission.json::independent_complete_sixteen_record_project_report_preserves_every_field_and_order`.

The shared `def:leaf` space is checked, not disambiguated. A duplicate dataset
`id`, codelist `id` or document `id`, or a document `id` equal to a dataset
`id`, fails with `duplicate_define_identifier`. A terminology standard's
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
`http://www.w3.org/1999/xlink`, and no other namespace.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::fixed_time_xml_has_one_column_list_two_ordered_item_and_where_references`, `rust/crates/yamaa-application/tests/file_resources.rs::dm_both_goldens_without_input_execution_and_save_uses_held_bytes`.

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
      <def:WhereClauseDef>*        value-list order, then entry order
      <ItemGroupDef>*              datasets, in entry order
      <ItemDef>*                   dataset order, then output.columns order; per-value definitions follow their base column
      <CodeList>*                  codelists, in declaration order
      <MethodDef>*                 ItemDef order, including per-value methods
      <def:CommentDef>*            comment order, below
      <def:leaf>*                  documents, in declaration order
    </MetaDataVersion>
  </Study>
</ODM>
```

Comment definitions are emitted per dataset in entry order: the dataset's own
comment first, its columns' comments in `output.columns` order, with each
column's per-value comments following its base comment in entry order.

A dataset's `def:leaf` is a child of its `ItemGroupDef`. A supporting
document's `def:leaf` is a child of `MetaDataVersion`, as the schema requires.
No extension element is generated.

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
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_empty_generator_version_leaves_the_source_system_incomplete`, `python/tests/installed/installed_public_define.py::test_source_version_defaults_and_schema_facet_failures`.

| Attribute | Source |
|---|---|
| `ODMVersion` | fixed `1.3.2` |
| `FileType` | fixed `Snapshot` |
| `FileOID` | `file_oid` |
| `CreationDateTime` | selected `created`, or preparation-time UTC |
| `Originator` | `originator`, when declared |
| `SourceSystem` | fixed `yamaa` |
| `SourceSystemVersion` | section override or actual installed version |
| `def:Context` | `context` |

Each section may declare a non-empty `source_system_version`. Its value
overrides the source-system version metadata in both Define-XML and
Dataset-JSON, and omission selects the actual installed yamaa version. A
source-system version that would be written empty fails with
`source_system_incomplete`. Qualification retains the actual generator version
separately from this metadata override. The generated Define-XML release is
`2.1.11`, and `SourceSystem` is `yamaa`.

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

Every kind may be referenced from an origin, a method or a comment. `link`
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

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::the_ae_metadata_only_document_has_its_fixed_time_and_independent_structure`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::the_vs_metadata_only_document_has_its_fixed_time_and_independent_structure`.

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
| `def:StandardOID` | the generated identifier of the guide [DEFINE-6](#define-6) binds the dataset to |
| `def:CommentOID` | generated, when `submission.comment` is declared |
| `def:HasNoData` | `Yes` when `submission.has_no_data`, else omitted |
| `Description` | `submission.label` |
| `ItemRef` | one per `output.columns` entry, in that order |
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

Historical export instead uses only its literal `submission.artifact` link,
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
with the attributes the table below lists, and `OrderNumber` is written for
every `ItemRef`. `KeySequence` describes the non-missing row identity `keys`
asserts ([OUTPUT-3](../output.md#output-3)) and is not
independently declarable: membership in `keys` determines it, and using a
sponsor variable in `output.order_by` or a `unique` verification does not
independently assign it.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::fixed_time_xml_has_one_column_list_two_ordered_item_and_where_references`, `python/tests/installed/installed_public_define.py::test_ae_vs_independent_fixed_time_schema_and_structure`.

| Attribute | Source |
|---|---|
| `ItemOID` | the column's generated identifier |
| `OrderNumber` | the column's one-based position in `output.columns` |
| `Mandatory` | `Yes` when the mandatory resolution of [SUB-10](submission.md#sub-10) is true, else `No` |
| `KeySequence` | one-based position in `keys`; omit when not a key |
| `Role` | `submission.role`, when declared |
| `MethodOID` | generated method identifier; omit without `submission.method` |

The [output identity example](../output.md#output-3) shows how to
use those variables when some values may be missing.

`OrderNumber` is written for every `ItemRef` of a container or for none of
them, and this profile writes it for every one. Define-XML makes the attribute
optional and falls back to document order; writing it always means the
presented order survives any later reordering of the document.

<a id="define-19"></a>

### DEFINE-19. Each column produces one base `ItemDef` carrying one `def:Origin` per declared origin

Each column produces one base `ItemDef` with the attributes and children the
table below lists; per-value definitions use the same mapping with the
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
| `Description` | the column's `label`, or a per-value definition's entry `label` when declared |
| `CodeListRef` | the bound codelist's generated identifier, when declared |
| `def:Origin` | one per declared origin, per the origin mapping below |

[SUB-15](submission.md#sub-15) derives the origin `Source` for the `ADaM`
model. Each `def:Origin`'s `Description` child carries that origin's
`description` when declared, and one `def:DocumentRef` child follows per
document reference of that origin in declaration order, including the
page-less annotated case report form reference
[SUB-17](submission.md#sub-17) derives under `context: Other` when a
collected origin declares none. A value-level entry without origins of its
own takes the column's ([DEFINE-27](#define-27)).
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
`def:CommentDef` carrying its generated identifier. Every `Description` and
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

The generated document must be valid against the selected Define-XML schema; a
document that is not fails, reporting the schema violation, and the run
publishes nothing. Publication follows the procedure of
[OUTPUT-8](../output.md#output-8) exactly: the complete
document is written to a temporary regular file in the target's directory,
flushed and closed, and atomically moved onto the target. A failure leaves the
previous file and no residue.

Fails: `value_not_permitted`, `publication_failed`.
Evidence: `rust/crates/yamaa-application/src/file_define/publication_failure_tests.rs::later_publication_failure_preserves_prior_bytes_original_cause_and_stopped_remainder`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::a_creation_time_outside_the_schema_facet_is_a_schema_violation`, `python/tests/installed/installed_public_define.py::test_source_version_defaults_and_schema_facet_failures`.

Conformance to this profile is not a substitute for schema validation: an
implementation that produces a schema-invalid document has a defect regardless
of which rule it satisfied. A schema-invalid document fails with
`value_not_permitted`. Generation completes before publication begins, so a
failure at any point publishes nothing. A failed publication fails under
[OUTPUT-8](../output.md#output-8) with `publication_failed`.

## Scope

<a id="define-26"></a>

### DEFINE-26. Constructs outside the profile have no schema field and are never generated

The constructs listed below are outside this profile. None has a field in the
schema that [Submission metadata](submission.md),
[Controlled terminology](terminology.md) and the project schema close,
so a specification that tries to declare one is rejected as an unknown field.
An `Alias` carrying a transport name longer than eight characters is not
generated, and a `FormalExpression` is generated only from a declared
`method.expression`, never from a derivation.

Evidence: `rust/crates/yamaa-core/tests/schema_submission.rs::unknown_and_non_text_fields_preserve_exact_occurrences_without_fabricated_causes`, `rust/crates/yamaa-core/tests/schema_submission.rs::complete_nested_metadata_retains_integer_text_documents_pages_and_order_after_drop`.

- `arm:AnalysisResultDisplays`: an analysis-results metadata design
- Split datasets and their `Alias` domain description: a specification that
  produces one dataset in several files
- `def:IsNonStandard` on a dataset or column: a conformance-profile rule
  defining non-standard
- A comment on a standard, codelist, or metadata version: a shared comment
  identifier space

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
the column's metadata merged with the entry's. The column's own `ItemDef`
carries the `def:ValueListRef`.

Evidence: `sdtm-lb-metadata`, `adam-adlb-metadata`, `sdtm-suppdm-metadata`, `schema-value-level-not-equal`, `schema-value-level-not-in`, `rust/crates/yamaa/tests/language_header.rs::a_value_level_entry_selects_the_rows_that_hold_none_of_its_values`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::fixed_time_xml_has_one_column_list_two_ordered_item_and_where_references`, `rust/crates/yamaa-adapters/tests/submission_encoders.rs::several_values_are_a_membership_and_several_columns_one_range_check_each`, `rust/crates/yamaa-application/tests/file_resources.rs::lb_value_metadata_uses_original_published_bytes_and_family_valid_completion`.

The mapping to Define-XML is fixed:

- each value-list `ItemRef` carries `ItemOID`, the entry's per-value
  identifier ([DEFINE-9](#define-9)), `OrderNumber`, the entry's one-based
  position among the column's entries, `Mandatory`, the entry's resolution of
  [SUB-10](submission.md#sub-10), and `Role` and `MethodOID` when its merged
  metadata declares them. The column's `ItemDef` carries its single
  `def:ValueListRef` after its `def:Origin` elements;
- each `def:WhereClauseDef` carries one `RangeCheck` per where column, in the
  clause's written order: `Comparator="EQ"` when the column lists one value
  and `"IN"` when it lists several, or the comparator the column writes,
  `"NE"` or `"NOTIN"` ([SUB-12](submission.md#sub-12)), `SoftHard="Soft"`,
  `def:ItemOID` the where column's `ItemDef` identifier, and one `CheckValue`
  per listed value in written order; and
- each per-value `ItemDef` maps the merged metadata as
  [DEFINE-19](#define-19) does, with the entry's `label` as its
  `Description` when declared and its `Length` from
  [SUB-24](submission.md#sub-24) or [SUB-7](submission.md#sub-7). An entry
  that declares origins writes its own, and one that declares none takes
  the column's whole list. Associated methods, comments and document
  references remain resolvable.

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
when it updates that schema's controlled terminology.
Unpublished versions and incomplete version strings are refused.
A 2.0 document uses only facts that version
can represent and requires one Final implementation guide. Declarations
requiring 2.1 constructs fail before publication.

Fails: `value_not_permitted`.
Evidence: `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_define_2_0_document_preserves_its_declared_metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/define_versions.json::define_2_1_0_preserves_the_selected_release`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/define_versions.json::define_2_1_11_preserves_the_selected_release`.

For 2.0 these mappings replace the version-specific fields of the 2.1 tables
above. The document uses the v2.0 namespace, `DefineVersion="2.0.0"`,
`StandardName` and `StandardVersion` on `MetaDataVersion`, and a dataset
`Class` attribute. ADaMIG, SDTMIG and SENDIG are written as ADaM-IG, SDTM-IG
and SEND-IG. It omits 2.1-only standards, standard references, codelist
non-standard flags, context and origin sources. Under SDTM and ADaM,
Collected is written as CRF; Derived, Assigned, Protocol and Predecessor retain their
names, and Other and Not Available are refused. Under SEND the admitted
types are Collected, Derived, Other and Not Available, written as COLLECTED,
DERIVED, OTHER and NOT AVAILABLE; Assigned, Protocol and Predecessor are refused.

Multiple guides, terminology releases, a subclass and HasNoData have no
faithful representation and are refused. An explicit origin source
is representable only when it is the older type's implicit source:
Investigator for Collected, Sponsor for Derived, Assigned, Protocol or Other. This
version choice does not change derivations, ordinary metadata validation or
the transport encoders.

<a id="define-30"></a>

### DEFINE-30. Historical metadata is a literal record and makes no assertion about current derivations or data

A dataset's explicit `submission.mode: historical` reproduces received
metadata in Define-XML 2.0 under `context: Other` without transport; omission
retains validated mode. Historical declarations add no execution constraints;
executable types, keys and explicit verifications remain active. Composition
admits closed metadata, resolves every codelist and document reference, and
requires a schema-valid document. It makes no data-conformance assertion.

Fails: `value_not_permitted`, `submission_metadata_missing`, `unknown_codelist`.
Evidence: `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_historical_define_2_0_keeps_declarations_independent_of_execution`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::historical_metadata_keeps_explicit_verifications`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::historical_links_ignore_current_output_location_and_run_report`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::historical_where_clauses_ignore_executable_integer_types_codelist_values_and_row_overlap`.

The historical export requires explicit column `data_type`, `mandatory` and
labels. Lengths, formats, origins, methods and codelists remain as declared.
`key_sequence` declares a historical ItemRef key position independently of
executable `keys`. `artifact` declares a literal relative package link; its
filename supplies the leaf title. Omission writes no historical artifact
leaf. Both fields are refused on validated metadata. Historical value-level
entries inherit the column metadata as ordinary entries do, except key
positions never inherit.

The current executable output need not share the document's directory, and
historical composition does not read its run report, artifact or inputs.
Publication still protects declared inputs and outputs from being overwritten.
Historical declarations do not request unrelated section specifications for
inventory or predecessor assertions. Distinct historical datasets may share
an executable output path; each keeps its own literal metadata and artifact link.
Value-level where clauses retain structural and output-column name checks,
but executable types, terminology membership and overlapping row selections
do not constrain a historical declaration.

Historical codelists, mandatory flags, lengths, origins and value-level
metadata do not bind execution. They describe received declarations, not the
current derived artifacts. Column types, executable keys, explicit
verifications and all other ordinary checks still apply. Output and
where-clause names remain checked during composition. An independent
metadata comparison and data audit assess those separate claims. No
reference XML is read or passed through by this profile, and no transport
is published or certified.
