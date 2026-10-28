# Submission metadata

This contract describes validated metadata. The explicit historical export
mode is governed by [DEFINE-30](define-xml.md#define-30); its received
declarations do not constrain data execution.

This file is a profile: it is optional, and it applies to a specification that
declares `submission` metadata and to the project file section that
composes such specifications into a [Define-XML](define-xml.md) document and
[Dataset-JSON](dataset-json.md) or [SAS Transport](xpt.md) files. It governs
the dataset and column metadata a submission document is generated from: what
each field means, which combinations a standard admits, and what the derivation
refutes. A specification that declares no submission metadata is unaffected,
except that its free-form `metadata` maps follow [SUB-2](#sub-2).

## Declaring submission metadata

<a id="sub-1"></a>

### SUB-1. Submission metadata is optional and is declared only for columns the artifact carries

`root.submission` carries the dataset's metadata and `column.submission`
carries one column's. Both are optional in the schema, and a specification
that no submission document represents needs neither;
[Define-XML](define-xml.md) requires them from a specification that joins a
document. Submission metadata is declared only for a column `output.columns`
carries, and declaring it for an internal column fails validation, reporting
the column.

Fails: `submission_not_output_column`.
Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_column_with_submission_metadata_that_is_not_output_is_refused`, `rust/crates/yamaa-core/tests/schema_submission.rs::absent_submission_remains_absent_and_preserves_ordinary_column_declarations`.

An internal column is not in the artifact, so it is not in the document that
describes the artifact; declaring metadata for one is an error rather than an
ignored declaration.

A column's `codelist` names a codelist the project file declares.
[Controlled terminology](terminology.md) owns that object, the values a binding
enforces ([CT-7](terminology.md#ct-7)), and the redundancy of an
`allowed_values` verification beside it ([CT-9](terminology.md#ct-9)).

<a id="sub-2"></a>

### SUB-2. The free-form `metadata` map is never read and never carries a governed field name

The free-form `metadata` map is uninterpreted annotation: no submission field
reads it, it is not merged with submission metadata, and it is never a
fallback. The map must not carry a key this profile governs: root `metadata`
rejects every dataset metadata field name, and a column's `metadata` rejects
every column metadata field name.

Fails: `reserved_metadata_key`.
Evidence: `schema-column-composition`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_reserved_metadata_key_is_refused_before_study_access`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/codelist_binding.json::submission_claims_are_admitted_and_reserved_metadata_has_a_registered_cause`, `rust/crates/yamaa/tests/language_header.rs::a_1_0_root_metadata_reserves_standard`.

A study moves governed facts from the map to submission metadata. A reserved
key fails validation with `reserved_metadata_key`, reporting the key. A
dataset's `standard` is a dataset metadata field, so the root `metadata`
map reserves it.

## Models

<a id="sub-3"></a>

### SUB-3. A specification declares its model, and the implementation guide a document binds it to must belong to that model

A specification names the CDISC model its dataset follows in `model`:
`SDTM`, `SEND` or `ADaM`. `model` is required when the specification
declares root or column `submission` or a project section lists it, and the
`sdtm`, `send` and `adam` sections list only specifications of their model.
When a document composes the specification, the implementation guide
[DEFINE-6](define-xml.md#define-6) binds it to must be one the table below
lists for that model; a guide of another model, or a standard of type `CT`,
fails.

Fails: `missing_required_field`, `standard_model_mismatch`.
Evidence: `sdtm-dm-metadata`, `negative-submission-model-missing`, `negative-section-model-mismatch`, `rust/crates/yamaa/tests/language_header.rs::submission_metadata_needs_a_model`, `rust/crates/yamaa/tests/language_header.rs::a_section_lists_only_specifications_of_its_model`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_specification_declaring_submission_metadata_names_its_model`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_specification_naming_its_model_may_declare_submission_metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_listed_specification_of_another_model_is_refused`, `python/tests/installed/installed_public_define.py::test_missing_artifact_only_when_requested_and_semantic_failure_zero_writes`.

| Model | Standard names |
|---|---|
| `SDTM` | `SDTMIG`, `SDTMIG-AP`, `SDTMIG-MD` |
| `SEND` | `SENDIG`, `SENDIG-AR`, `SENDIG-DART`, `SENDIG-GENETOX` |
| `ADaM` | `ADaMIG`, `ADaMIG-MD` |

`model` stays optional elsewhere, since a language test follows no model.
Declared, it decides the rules of this profile that depend on a model, which
[SUB-23](#sub-23) checks on the specification alone, and a declared
`submission.class` must belong to it ([SUB-10](#sub-10)). A specification
that declares submission metadata without `model` fails validation with
`missing_required_field` at `model`, before any input is read; a document
that composes it reports the same issue.

The section's listing is checked when [Define-XML](define-xml.md) composes
the section, since it reads the project file ([SUB-23](#sub-23)). A listed
specification without `model` fails there with `missing_required_field`, and
one that declares another model with `standard_model_mismatch`, each at
`<section>.specs[<n>].model`; the context names the section in `section`
and its model in `model`, and the mismatch names the declared model in
`value`. Neither dataset composes further.

The table is closed, and `BIMO` is excluded. The guide composition binds a
dataset to is the entry of the section's `standards` that its
`submission.standard` names, or the first
([DEFINE-8](define-xml.md#define-8)). Every entry belongs to the section's
model ([DEFINE-6](define-xml.md#define-6)), and the section lists only
specifications of that model, so a guide outside the specification's model
fails where the section lists it, with `standard_model_mismatch` at
`<section>.standards[<i>].name`, cited to DEFINE-6. A dataset must not be
bound to a standard of type `CT`: controlled terminology qualifies a codelist
and not a dataset, and [Controlled terminology](terminology.md) owns that
binding.

## Dataset metadata

<a id="sub-4"></a>

### SUB-4. Dataset metadata states its label, observation class, record structure and whether it repeats

`label`, `class`, `structure` and `repeating` are required. `class` names the
general observation class in the exact spelling and case the vocabulary
publishes, and `subclass` is declared only when the dataset's implementation
guide defines one for it. `reference_data` defaults to false, and a dataset
declaring `reference_data: true` must declare `repeating: false`.
`has_no_data` defaults to false and records that the submitted dataset has no
data. `non_standard` defaults to false; `true` records that no guide defines
the dataset, such as a custom domain, and excludes `standard`.

Fails: `reference_data_repeating_conflict`, `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::reference_data_that_repeats_is_refused`, `rust/crates/yamaa-core/tests/schema_submission.rs::dataset_defaults_and_comment_survive_original_document_drop`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_non_standard_dataset_is_marked_non_standard_and_names_no_guide`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_non_standard_dataset_declares_no_standard`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_non_standard_dataset_selecting_a_guide_is_refused`.

`label` states what the dataset is. It is the dataset's own description and is
distinct from `domain`: `DM` is the domain and `Demographics` is the label.
`structure` states in prose the level of detail one record represents, such as
`One record per subject`. Keys alone do not fix a dataset's structure: one key
list serves several structures. `repeating` states whether the dataset may
carry more than one record per subject or pool, and `reference_data` whether
the dataset carries reference data rather than subject data. A
`reference_data: true` dataset declaring `repeating: true` fails validation,
reporting the dataset.

`has_no_data` is carried for submission composition. It does not suppress
domain execution or replace row-count checks. `standard` names the guide of
the composing section that binds the dataset, and composition resolves it
([DEFINE-8](define-xml.md#define-8)).

`non_standard` is declared, never inferred: no rule compares a dataset's
domain with the domains a guide publishes, so a domain no guide defines
composes as any other until its specification says so. Only an `SDTM` or
`SEND` dataset declares it ([SUB-5](#sub-5)). A non-standard dataset
binds no guide ([DEFINE-6](define-xml.md#define-6)), and a `standard` beside
`non_standard: true` would name a guide the dataset does not follow. It fails
validation at `submission.standard`, with the declared name in `value` and
`non_standard` true; a document fails it at
`<section>.specs[<n>].submission.standard`.

<a id="sub-5"></a>

### SUB-5. A dataset's domain defaults to the specification's domain and is declared only for SDTM and SEND

`domain` names the domain the dataset belongs to. It defaults to the
specification's `domain`, so an ordinary dataset states the domain once. A
split or supplemental dataset declares the domain; the dataset's own name is
not its parent domain. `domain` is declared only for a dataset whose model is
`SDTM` or `SEND`: the `ADaM` model has no domain, and declaring one there
fails validation, reporting the field.

Fails: `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_adam_column_role_is_not_permitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_column_declares_no_role`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_dataset_declares_no_domain`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_dataset_is_never_declared_non_standard`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_adam_dataset_composed_non_standard_is_refused`.

A domain declared for the `ADaM` model is an error rather than an unused
value. A `role` declared for that model fails the same way under
[SUB-9](#sub-9), and so does `non_standard: true`, at
`submission.non_standard` with `true` in `value`: only a tabulation dataset
is marked non-standard ([SUB-4](#sub-4)). A specification that declares
`model: ADaM` fails at its `submission.domain` on its own
([SUB-23](#sub-23)), with the declared value in `value` and the model in
`model`; a document fails it at `<section>.specs[<n>].submission.domain`.

## Column metadata

<a id="sub-6"></a>

### SUB-6. A column's submission data type defaults from its column type and model, within the closed set that type admits

`data_type` states the type the submission represents the column as. It
defaults from the declared column type and, for a `date` or `datetime`, from
the model ([SUB-25](#sub-25)); each declared type admits the closed set the
table below fixes. `data_type` never changes a value: it states how a value
[Values](../values.md) already typed appears in the document, and no
derivation, key, order term or artifact byte can observe it; only the check of
a date or time type's form ([SUB-26](#sub-26)) reads it. A `data_type` outside
the admitted set fails validation.

Fails: `submission_data_type_not_admitted`.
Evidence: `sdtm-vs-epoch-from-subject-elements`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_integer_data_type_is_not_admitted_for_a_text_column`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::the_ae_coding_benchmark_saves_its_expected_domain_with_its_project`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::the_dm_metadata_benchmark_saves_its_expected_domain_with_its_project`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::the_lb_metadata_benchmark_saves_its_expected_domain_from_its_submission_layer`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::the_vs_epoch_benchmark_saves_its_expected_domain_with_its_project`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_date_is_not_submitted_as_a_date_data_type`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_date_entry_inheriting_its_data_type_leaves_it_to_its_column`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_sdtm_date_is_not_submitted_as_an_integer`.

| Declared type | Default | Also admits |
|---|---|---|
| `str` | `text` | the types listed below |
| `int` | `integer` | none |
| `float` | `float` | none |
| `date` | `date`; `integer` under `ADaM` | none |
| `datetime` | `datetime`; `integer` under `ADaM` | none |

`str` admits `date`, `datetime`, `time`, `partialDate`, `partialTime`,
`partialDatetime`, `incompleteDate`, `incompleteTime`, `incompleteDatetime`,
`durationDatetime`, `intervalDatetime` and `URI`.

A `str` column admits temporal submission types: a submission carries a
partial or incomplete date as text, and [Values](../values.md)
does not admit partial or incomplete dates as `date` or `datetime`. Each of
those types names an ISO 8601 form, which [SUB-26](#sub-26) checks the
column's values against. The two
temporal column types admit only their matching submission type:
[Values](../values.md) defines each temporal type as a
complete value, and the submission types of the same name mean the same thing.
A number's submission type is fixed: [Values](../values.md)
already decides integer or binary64. The model decides a temporal column's
one type: its own under `SDTM` and `SEND`, and `integer` under `ADaM`. A
specification that declares its model admits only that type on its own
([SUB-23](#sub-23)), and so does a document; a specification that declares
none admits a `date` or `datetime` declared `integer` as well as its own type
until a document gives it its section's model. The failure reports the
column, the declared type and the submission type, and on the specification
alone the model in `model` when only the model refuses the type.

<a id="sub-7"></a>

### SUB-7. `length` and `significant_digits` are declared exactly where the submission data type needs them

`length` is the maximum expected value length, as the submission standard
defines the term: a property of the column declaration, not of the data one run
produced, and a positive integer. Unless [SUB-8](#sub-8) derives it, it is
required for an `integer` or `float` data type, and for `text` unless every
section listing the specification requests a transport; it is never declared
otherwise, nor on a `date` or `datetime` column. `significant_digits`, a
non-negative count of digits after the decimal point, is declared exactly for
`float`.

Fails: `submission_length_missing`, `submission_length_not_applicable`, `submission_length_invalid`.
Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_text_column_without_a_length_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_text_column_without_a_length_composes_under_a_transport`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_text_column_without_a_length_publishes_its_longest_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_text_column_without_a_length_passes_the_check_on_its_own`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_integer_column_without_a_length_is_refused_on_its_own`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_entry_inheriting_its_columns_full_metadata_passes_the_check`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_per_value_length_below_any_integer_is_invalid`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_per_value_length_of_zero_is_invalid`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::per_value_significant_digits_below_any_integer_are_invalid`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_per_value_date_data_type_refutes_the_inherited_length`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::per_value_significant_digits_are_inapplicable_to_text`.

A `text` column, or a value-level entry, may omit `length` when every
submission section of the project file that lists its specification requests
a `transport`: its length is then the published length
[SUB-24](#sub-24) measures, and no keyword states it. A declared `length`
keeps its meaning, the bound [SUB-8](#sub-8) enforces and the document's
`Length` without a transport. Whether a section requests a transport is known
only to the project file, so an omitted text `length` is checked when a
document composes the specification ([SUB-23](#sub-23)): a section without a
transport that lists it fails with `submission_length_missing` there, and a
specification checked without its project file reports nothing. An `integer`
or `float` column always declares `length`, and a `float` column
`significant_digits`.

The other submission types carry fixed-form values, so a length for such a type
restates that form. A `date` or `datetime` column has the fixed length its
model gives it, none under `SDTM` and `SEND` and 8 under `ADaM`
([SUB-25](#sub-25)), whatever `data_type` it declares. A `length` that is not a
positive integer, or a negative `significant_digits`, fails validation with
`submission_length_invalid`. A `length` or `significant_digits` absent where
this rule requires it fails with `submission_length_missing`, and one declared
where this rule prohibits it fails with `submission_length_not_applicable`,
reporting the column.

<a id="sub-8"></a>

### SUB-8. A `str` column's length is a bound its completed values must meet, stated once

A `length` on a `str` column is enforced: the column's completed values must
contain at most `length` Unicode scalar values
([VALUE-11](../values.md#value-11)), as [Verification](../verification.md)'s
`max_length` requires. On a `str` column with a `max_length` verification,
`length` is derived from the verification's `max` and need not be declared;
declaring both is accepted when equal and rejected when different. A `length`
on another column type is carried unchecked, and a declared bound is the
document's `Length` only when no transport is requested ([SUB-24](#sub-24)).

Fails: `declared_length_conflict`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_declared_length_counts_unicode_scalar_values`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_entry_length_above_the_columns_max_length_conflicts_with_it`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_submission_length_above_the_columns_max_length_conflicts_with_it`.

The declared length binds rather than describes. `length` and `max_length`
state one bound, and the bound is checked once either way. A `length` and a
`max_length` verification declaring different bounds on the same column fail
validation with `declared_length_conflict`, reporting the column and both
bounds. A `str` column with neither declaration, where [SUB-7](#sub-7) requires
a length, fails as [SUB-7](#sub-7) states. Where [SUB-7](#sub-7) admits
neither, as for a `text` column under a transport, the column has no bound,
only the published length [SUB-24](#sub-24) measures.

The bound counts scalar values and the published length counts bytes, so for
text outside ASCII a published length can exceed the declared `length` that
every value meets ([SUB-24](#sub-24)).

<a id="sub-26"></a>

### SUB-26. A `str` column's date or time data type is an ISO 8601 form its completed values must have

A `str` column whose `data_type` is a date or time type, `date`, `datetime`,
`time` or a partial, incomplete, duration or interval type, has each present
completed value checked against that type's ISO 8601 form, which the table
below fixes. The check runs where [Verification](../verification.md) runs a
column verification, after the column's length and before its terminology,
and a value of another form fails as a column verification fails. A
value-level `data_type` decides the rows its entry selects.

Fails: `data_type_failed`.
Evidence: `sdtm-vs-epoch-from-subject-elements`, `negative-data-type-form`, `rust/crates/yamaa-core/tests/fixtures/submission_data_types.tsv`, `rust/crates/yamaa-application/tests/project_domain_submission.rs::date_and_time_data_types_check_each_completed_value_against_their_form`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_partial_datetime_data_type_refuses_a_space_before_the_time`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_value_level_date_data_type_checks_only_the_rows_its_where_selects`.

| `data_type` | Form | Examples |
|---|---|---|
| `date` | `YYYY-MM-DD` | `2025-03-04` |
| `time` | `hh:mm:ss`, then an optional fraction and an optional zone | `08:30:15`, `08:30:15.5Z` |
| `datetime` | a `date`, `T` and a `time` | `2025-03-04T08:30:15` |
| `partialDate` | `YYYY`, then optionally `-MM` and then `-DD` | `2025`, `2025-03` |
| `partialTime` | `hh`, then optionally `:mm` and then `:ss`, a fraction only after the second, and an optional zone | `08`, `08:30` |
| `partialDatetime` | a `partialDate`, then after a complete date optionally `T` and a `partialTime` | `2025-03`, `2025-03-04T08:30` |
| `incompleteDate` | a `partialDate` whose fields may each be `-`, at least one known | `2025---04`, `--03-04` |
| `incompleteTime` | a `partialTime` whose fields may each be `-`, at least one known | `-:30`, `08:-:15` |
| `incompleteDatetime` | a `partialDatetime` whose fields may each be `-`, at least one known | `2025-03-04T-:30`, `-----T08:30` |
| `durationDatetime` | an ISO 8601 duration ([TIME-13](../temporal.md#time-13)) | `P3D`, `-PT15M` |
| `intervalDatetime` | an ISO 8601 interval ([TIME-13](../temporal.md#time-13)) | `2025-01-01/2025-01-10`, `2025-01-01/P10D` |

Every field has the width and range of [VALUE-15](../values.md#value-15), and
a known day lies within its month, from `01` to `31` when the month is unknown
and to `29` in a February of an unknown year. A fraction is `.` followed by
one or more digits and follows only a second; a zone is `Z` or a signed
`hh:mm` offset and follows only a time. The forms are the ISO 8601 forms the
Define-XML data types name, so `datetime` is complete through the second, as
[Values](../values.md) defines the column type of the same name, and text
collected to the minute is a `partialDatetime`. A partial or incomplete form
may stop after any field. `text` and `URI` have no form to check.

The run report records the check as a `data_type` check row at
`columns.<column>.submission.data_type`, or at
`columns.<column>.submission.value_level.<id>.data_type` for a value-level
entry, and its failure reports `data_type_failed` with the column, the
declared data type, the failure count and the offending keys, at `error`
severity, as the check of a declared length is. A value-level entry whose
data type differs from its column's decides the rows its where clause selects
([SUB-12](#sub-12)), as [SUB-11](#sub-11) decides `mandatory`, and the
column's data type decides every other row. A missing value is not checked;
[SUB-11](#sub-11) decides whether one is allowed. The check reads values and
changes none, so [SUB-6](#sub-6)'s `data_type` still changes no value.

<a id="sub-9"></a>

### SUB-9. `display_format` and `role` describe a column for a reader and never change a value

`display_format` is presentation text carried for a reader: it never changes a
computed value, a rendered artifact value or the `output.decimals` of
[CSV-6](../reference/csv.md#csv-6). `role` states how the column is used within
its dataset and is one of the nine roles the SDTM model publishes, which the
table below lists; any other role fails validation. `role` must not be declared
for a dataset whose model is `ADaM`; a role there fails validation, reporting
the field.

Fails: `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `negative-role-unknown`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_adam_column_role_is_not_permitted`, `rust/crates/yamaa-core/tests/schema_submission.rs::complete_nested_metadata_retains_integer_text_documents_pages_and_order_after_drop`, `rust/crates/yamaa-application/tests/project_domain_submission.rs::a_role_outside_the_nine_fails_where_the_specification_is_read`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_role_named_qualifier_alone_is_not_one_of_the_nine`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_role_written_in_lower_case_is_not_one_of_the_nine`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_analysis_role_is_not_one_of_the_nine`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_column_declares_no_role`.

| Role | The column |
|---|---|
| `Identifier` | identifies the study, the subject, the domain or the record |
| `Topic` | names the focus of the observation |
| `Timing` | describes when the observation happened |
| `Grouping Qualifier` | groups observations within the domain |
| `Result Qualifier` | carries a result of the observation |
| `Synonym Qualifier` | gives another name for a column of the observation |
| `Record Qualifier` | describes the record as a whole |
| `Variable Qualifier` | describes another column of the observation |
| `Rule` | states a condition or algorithm of the trial design |

A role is written exactly as the table spells it, at column or value level
([SUB-12](#sub-12)). The vocabulary is checked when the specification is read
([SUB-23](#sub-23)), so a role outside the table fails with
`value_not_permitted` at its `role` before any data is read, whether or not a
document composes the dataset. The `SEND` model uses the same nine roles. A
role under `ADaM` fails on a specification that declares its model, at its
`role` with the role in `value` and the model in `model`
([SUB-23](#sub-23)), and when a document composes the dataset.
`yaml/vocabulary/cdisc.yaml` holds them with the other closed CDISC
vocabularies.

A numeric column's `display_format` is also its SAS format in a transport file
([XPT-9](xpt.md#xpt-9)), and an `adam` date's is fixed ([SUB-25](#sub-25)). An
`int` column holding seconds since midnight, such as an ADaM `*TM` time from
`time_of_day` ([TIME-20](../temporal.md#time-20)), declares a `TIMEw.` format
such as `TIME8.` like any other numeric display format. A
study that wants different artifact digits declares `output.decimals`, which
the [CSV profile](../reference/csv.md) owns. The two settings are independent,
and this profile does not reconcile them: one is a display a document reports
and the other is a display an artifact carries. A transport carries the
artifact's values, so a specification that a section requesting one composes
declares no `output.decimals` ([JSON-5](dataset-json.md#json-5)) and keeps its
display in `display_format`. The `ADaM` model defines no role vocabulary, so
a role in that model would name nothing.

<a id="sub-10"></a>

### SUB-10. `core` takes its model's designations, and `mandatory` comes from the keys, the declaration, or the class and `core`

`core` is the standard's core designation and is declared for every model:
`Req`, `Exp` or `Perm` for `SDTM` and `SEND`, and `Req`, `Cond` or `Perm` for
`ADaM`. `mandatory` states whether every completed value must be present and
resolves without a study document. A key column is mandatory, and declaring
`mandatory: false` on one fails. Otherwise a declared `mandatory` wins, and
`ADaM` must declare one; without one, a column is mandatory exactly when its
dataset's class is a tabulation class and its `core` is `Req`.

Fails: `core_mandatory_conflict`, `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `negative-required-sex-missing`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_mandatory_column_refuses_a_missing_value_without_a_study_document`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_required_column_of_a_findings_dataset_refuses_a_missing_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_required_column_without_a_class_admits_a_missing_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_required_column_of_an_analysis_dataset_declared_not_mandatory_admits_a_missing_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_key_declared_not_mandatory_is_not_permitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_column_core_the_model_does_not_publish_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_sdtm_column_core_the_model_does_not_publish_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_column_without_mandatory_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_sdtm_required_column_declared_not_mandatory_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_required_key_declared_not_mandatory_reports_the_conflict_once`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_sdtm_class_is_not_an_adam_class`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_column_submission_without_core_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_class_outside_the_model_is_not_permitted`.

For `SDTM` and `SEND`, `Req` is required, `Exp` expected and `Perm`
permissible. For `ADaM`, `Req` is required, `Cond` conditionally required and
`Perm` permissible, as the ADaM implementation guides publish them. An `Exp`
for `ADaM`, or a `Cond` for `SDTM` or `SEND`, names no designation its standard
publishes.

The dataset's `submission.class` names its group, so the domain build
resolves `mandatory` from the specification alone:

| Group | Classes | Models |
|---|---|---|
| tabulation | `EVENTS`, `FINDINGS`, `FINDINGS ABOUT`, `INTERVENTIONS`, `RELATIONSHIP`, `SPECIAL PURPOSE`, `STUDY REFERENCE`, `TRIAL DESIGN` | `SDTM`, `SEND` |
| analysis | `ADAM OTHER`, `BASIC DATA STRUCTURE`, `DEVICE LEVEL ANALYSIS DATASET`, `MEDICAL DEVICE BASIC DATA STRUCTURE`, `MEDICAL DEVICE OCCURRENCE DATA STRUCTURE`, `OCCURRENCE DATA STRUCTURE`, `REFERENCE DATA STRUCTURE`, `SUBJECT LEVEL ANALYSIS DATASET` | `ADaM` |

A dataset that declares no class resolves `mandatory` from its keys and its
declarations alone. A declared class belongs to the group of the dataset's
model, so the group the domain build read is the model's. A specification
that declares its model fails on its own when its class belongs to the other
group ([SUB-23](#sub-23)), with the class in `value` and the model in
`model`; a document fails a dataset whose class belongs to the other group of
its section's model. Each fails with `value_not_permitted`, reporting
`submission.class`.

For a tabulation class, `core` derives `mandatory`: `Req` gives true, and `Exp`
and `Perm` give false. A declared `mandatory: true` on an `Exp` or `Perm`
column is an accepted sponsor restriction. A declared `mandatory: false` on a
`Req` column of an `SDTM` or `SEND` dataset is rejected: the standard requires
the column, and a specification cannot relax its standard by declaration.

For `ADaM`, the ADaM guides state that `Core` assists preparation and allows
nulls unless otherwise specified, and Define-XML has no Core attribute. A `Req`
core requires the variable's inclusion in the dataset but allows nulls;
`mandatory` states whether every row carries a non-missing value, as the
programming specification decides. A `Cond` core therefore tells nothing about
`mandatory`, and a `mandatory: false` on a `Req` column is accepted.

A key column resolves to true whatever its `core`, since
[OUTPUT-3](../output.md#output-3) already requires its values; a declared
`mandatory: false` on a key column, at column or value level, fails validation
with `value_not_permitted`, reporting the field. A value-level entry
([SUB-12](#sub-12)) resolves the same way over its merged metadata, taking
`mandatory` and `core` from the column where it declares neither.

Each of these fails with `core_mandatory_conflict`, reporting the column:

- a `core` absent for any model;
- an `Exp` for `ADaM`;
- a `Cond` for `SDTM` or `SEND`;
- a `mandatory: false` on a `Req` column for `SDTM` or `SEND`;
- a `mandatory` absent for `ADaM`.

A document checks each of them when it composes the dataset, at the column's
`<section>.specs[<n>].columns.<column>.submission` or the entry's. A
specification checks them on its own ([SUB-23](#sub-23)). A designation that
its model does not publish fails at the `core` that declares it, and a
`mandatory: false` on a `Req` column at that `mandatory`, wherever a column or
value-level entry declares it; an entry reports only a conflict its own
`core` or `mandatory` makes, since the column reports what the entry
inherits. An absent `core`, and under `ADaM` an absent `mandatory`, fail at
the column's `submission` when the specification declares dataset metadata;
without it, the column metadata is not yet complete, and a document reports
the absence. A specification that declares no model checks only an absent
`core`. The context names the column, an entry in `entry`, the cause in
`reason` (`core_missing`, `core_not_admitted`, `required_not_mandatory` or
`mandatory_missing`), and the declared `core` and the `model` where they
decide it.

[SUB-11](#sub-11) enforces the resolution, and
[DEFINE-18](define-xml.md#define-18) writes it as `Mandatory`.

<a id="sub-11"></a>

### SUB-11. A mandatory column is checked for missing values, and a `not_missing` beside it is redundant

Every completed value of a column whose resolved `mandatory` is true must be
present. [OUTPUT-3](../output.md#output-3) checks a key column. Any other
mandatory column is checked where [Verification](../verification.md) runs a
column verification and fails as `not_missing` fails. A value-level `mandatory`
decides only the rows its entry's where clause selects. A `not_missing`
verification on a column that is mandatory on every row restates the fact and
fails validation.

Fails: `not_missing_failed`, `redundant_declaration`.
Evidence: `negative-mandatory-arm-missing`, `negative-required-sex-missing`, `negative-value-level-mandatory-missing`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_value_level_mandatory_passes_and_saves_when_its_rows_hold_values`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_column_mandatory_an_entry_relaxes_passes_and_saves_when_every_row_holds_a_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_value_level_mandatory_refuses_a_missing_value_on_its_test_codes_rows`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_column_mandatory_an_entry_relaxes_refuses_a_missing_value_on_the_other_rows`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_not_missing_check_beside_a_mandatory_column_is_redundant`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_not_missing_check_restates_nothing_when_an_entry_relaxes_mandatory`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_not_missing_check_restating_a_mandatory_column_is_redundant`.

The domain build enforces the resolution of [SUB-10](#sub-10) without a study
document. The check runs after the column's declared verifications and before
its terminology checks, and the run report records it as a `not_missing` check
row ([CHECK-23](../verification.md#check-23)). Its failure reports the column,
the failure count and the offending keys, as a failed `not_missing` does, at
the path of the declaration that makes the column mandatory:
`columns.<column>.submission.mandatory`, or
`columns.<column>.submission.core` when `core` derives it, and the same fields
under `columns.<column>.submission.value_level.<id>` for a value-level entry.

A value-level entry that resolves differently from its column decides the rows
its where clause selects ([SUB-12](#sub-12)), and the column's resolution
decides every other row. A value-level `mandatory: false` therefore exempts
its entry's rows from a mandatory column, and a value-level `mandatory: true`
requires values on its entry's rows alone.

The redundant finding reports the verification's path and, in `stated_by`,
the declaration that already states the fact. A `not_missing` on a key column
is redundant under [CHECK-5](../verification.md#check-5). A `not_missing` on a
column that some entry exempts still requires values on that entry's rows, so
it is not redundant.

## Value-level metadata

<a id="sub-12"></a>

### SUB-12. A column's `value_level` entries give the rows their where clause selects their own metadata

A column's `submission` may declare `value_level` entries. Each entry has an
`id` unique within the column, a `where` clause, and metadata fields that
override the column's on the rows the clause selects. `where` maps declared
`str` output columns, other than the column itself, to nonempty lists of
distinct values, and selects a row when every listed column holds one of its
values or, under the comparator `NE` or `NOTIN`, none of them, missing
included. Two entries whose where clauses can select one row fail
validation.

Fails: `value_level_where_invalid`, `duplicate_identifier`, `value_level_overlap`.
Evidence: `sdtm-lb-metadata`, `adam-adlb-metadata`, `sdtm-suppdm-metadata`, `schema-value-level-not-equal`, `schema-value-level-not-in`, `negative-value-level-overlap`, `negative-value-level-excluded-overlap`, `negative-value-level-comparator-count`, `negative-value-level-where-column`, `negative-numeric-test-code`, `negative-value-level-where-not-in-codelist`, `rust/crates/yamaa/tests/language_header.rs::a_value_level_entry_selects_the_rows_that_hold_none_of_its_values`, `rust/crates/yamaa-core/src/submission_values.rs::an_exclusion_shares_the_values_it_does_not_list_and_the_missing_value`, `rust/crates/yamaa-core/src/submission_values.rs::ne_takes_one_value_and_notin_two_or_more`, `rust/crates/yamaa-core/src/submission_values.rs::static_findings_name_the_entry_column_and_fault`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_value_level_entry_whose_clause_holds_another_s_rows_overlaps_it`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_value_level_where_column_listing_no_values_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_value_level_entry_selecting_on_a_column_that_is_not_output_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_value_level_entry_selecting_on_a_column_that_is_not_text_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_value_level_entry_selecting_on_its_own_column_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::value_level_clauses_over_columns_neither_of_which_holds_the_other_compose`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::where_values_outside_the_where_columns_codelists_fail_composition`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::where_values_neither_codelist_admits_are_refused_in_the_order_the_clause_lists_them`, `rust/crates/yamaa-core/tests/schema_submission.rs::entries_inherit_complete_metadata_and_preserve_written_occurrences`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_where_value_outside_the_where_columns_codelist_fails_before_data`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_sponsor_addition_to_the_where_columns_extensible_list_is_admitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_where_value_of_a_list_an_entry_of_the_where_column_binds_is_admitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_external_dictionary_on_an_entry_of_the_where_column_admits_every_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_where_column_without_a_codelist_admits_every_value`, `rust/crates/yamaa/tests/project_binding.rs::a_where_value_is_never_left_unchecked_against_its_codelist`.

The column-level `submission` stays the shared declaration for the column,
and each entry supplies the metadata of the rows its where clause selects. An
ADaM `AVAL` can thus state each parameter's precision by `PARAMCD`, a
supplemental qualifier dataset each qualifier's codelist by `QNAM`, and a
findings domain each test's origin by `<DOMAIN>TESTCD`, without repeating the
shared metadata.

An entry declares any column metadata field except `value_level`: `core`,
`mandatory`, `role`, `data_type`, `length`, `significant_digits`,
`display_format`, `codelist`, `inventory_vocabulary`, `origins`, `method` and
`comment`, and `label`, the description of its definition
([DEFINE-27](define-xml.md#define-27)), which defaults to the column's
`label`. The entry's metadata merges field by field over the column's: a
field the entry declares replaces the column's whole, nested classes and the
`origins` list included, and every rule of this profile applies to the merged
metadata. An entry is reported at
`columns.<column>.submission.value_level.<id>`.

One value in a where clause selects the rows holding it, several values the
rows holding any of them, and several columns the rows meeting every column.
A where column written `{comparator: NE, values: [X]}` selects the rows that
do not hold `X`, and `{comparator: NOTIN, values: [X, Y]}` the rows that hold
none of its values; a row whose value is missing is among the rows either
selects. `NE` takes exactly one value and `NOTIN` two or more. A comparison
class without a comparator restates its list and fails as
`redundant_long_form` ([SCHEMA-15](../reference/schema-language.md#schema-15)).
A where column is compared on its completed value under [Values](../values.md)
equality, and a missing value matches no listed value. Listed values are
strings, which a value of another type never equals under
[VALUE-19](../values.md#value-19), so a where column must be `str`. A where
clause that names no column, or a where column that is the column itself, is
not a declared column, is not in `output.columns`, is not of type `str`, lists
no value, lists a number of values its comparator does not take, lists one
value twice or lists a value its codelists do not admit, fails with
`value_level_where_invalid` at `<entry>.where.<where column>`, or at
`<entry>.where` for a clause without columns, reporting the column, the entry,
the where column and the `reason`: `no_columns`, `self`, `undeclared`,
`not_output`, `not_str`, `no_values`, `value_count`, `duplicate_value` or
`not_in_codelist`. With `value_count` it also reports the comparator, `NE`
or `NOTIN`, in `comparator`. With `not_in_codelist` it also reports, in
`values`, the listed values that none of the where column's codelists
admits, in their declared order.
A schema-valid `id` is an identifier; two entries of one column with the same
`id` fail with `duplicate_identifier` at `<entry>.id`.

The codelists of a where column are its own `codelist` and the `codelist` of
each of its own entries, the lists that decide its rows under
[CT-7](terminology.md#ct-7). They admit their items, an extensible list's
`extended: true` items included. A where column that declares no `codelist` of
its own, or one of whose codelists is `external`, admits every value. A domain
build with a project and a composed document read the codelists. A
specification checked or run without a project file fails at each codelist it
binds ([CT-13](terminology.md#ct-13)), so a specification that passes
validation has checked every listed value against its where column's
codelists.

Validation decides that two where clauses can select one row when one
clause's columns are all columns of the other and every column they share
admits a common value: two lists that meet, a list holding a value an `NE` or
`NOTIN` column does not list, or any two `NE` or `NOTIN` columns, which both
select the missing value. The clauses then select at least one common row
whatever the data. `{comparator: NE, values: [X]}` and `[X]` on one column
admit no common value, so the rows of an ADaM dataset whose `DTYPE` is
missing and those it derives can each have their own entry. The failure is `value_level_overlap`, naming both where
clauses in declaration order and reporting the column and both entries. Two
clauses over columns neither of which contains the other's are admitted, and
[SUB-13](#sub-13) fails a row both select.

`value_level` is a list, so a later layer replaces it whole
([LAYER-6](../inheritance.md#layer-6)). An entry's `id` names its definitions
in a document independently of entry order ([DEFINE-9](define-xml.md#define-9)).

<a id="sub-13"></a>

### SUB-13. Each row matches at most one entry of a column, an entry that matches no row warns, and an entry's origin is refuted on the rows it selects

Every build of a specification declaring value-level entries checks the
completed rows before publication: a row two entries of one column select
fails, reporting its keys and the entries' ids, and an entry that selects no
row is a `warning`. An entry's own origins are refuted under
[SUB-19](#sub-19) against every derivation of the column that can produce a
row its where clause selects, and against every derivation of the column when
that cannot be known.

Fails: `value_level_multiple_matches`, `value_level_unmatched`, `refuted_value_origin`.
Evidence: `negative-value-level-ambiguous-row`, `rust/crates/yamaa-application/tests/project_domain_submission.rs::a_row_two_entries_select_fails_with_its_keys_and_both_entries`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_row_two_value_level_entries_select_fails_with_both_entries`, `rust/crates/yamaa-application/tests/project_domain_submission.rs::an_entry_that_matches_no_row_is_a_warning_and_scoped_checks_read_their_rows`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_value_level_predecessor_whose_rows_read_another_column_is_refuted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_literal_refutes_the_derived_origin_of_the_rows_an_entry_selects`, `rust/tests/host-fixtures/cases.json::value_level_report`.

Both checks run where [Verification](../verification.md) runs a column
verification, after the column's declared verifications and before its other
submission checks, and the run report records each as a `check` row named
`value_level` ([CHECK-23](../verification.md#check-23)). The check at
`columns.<column>.submission.value_level` fails with
`value_level_multiple_matches` when any row is selected by two entries,
reporting the column, the failure count, the offending keys and, aligned with
them, the ids of the entries each row matches.

The check of one entry, at the entry's path, has `warning` severity with the
semantics of [CHECK-2](../verification.md#check-2): it fails nothing and
changes no row. It examines the artifact as one unit, and an entry that
selects no row is violated with `value_level_unmatched`, reporting the column
and the entry. The violation is not an issue: it is a row of every run's
report, with `COUNT` and `TOTAL` `1` and `KEYS` `[{}]`
([CHECK-24](../verification.md#check-24)). Declaring entries does not require
`output.report` ([CHECK-22](../verification.md#check-22)); the file carries
the row when the specification declares one.

A construction is a row template, or the construction from column derivations
of a specification without row templates. A construction cannot produce a row
an entry selects when it derives a where column as a literal the column's
term does not select: one outside its values, a missing literal included, or
under `NE` and `NOTIN` one among them. Every row it produces is
selected when it derives every where column as a literal its term selects,
which under `NE` and `NOTIN` a missing literal is. Each of
an entry's own origins is refuted against the column's derivation in every
construction that can produce its rows, with the table and the copy check of
[SUB-19](#sub-19) and [SUB-20](#sub-20); the column's own origins answer for
every construction no entry declaring origins of its own selects completely.
An entry without origins of its own takes the column's
([DEFINE-27](define-xml.md#define-27)), which the column's refutation covers.
Each refuted origin of an entry fails with `refuted_value_origin` at the
entry's path, reporting the column, the entry, the refuting derivation and the
declared type.

## Origin

<a id="sub-14"></a>

### SUB-14. Every column with submission metadata declares an origin from closed type and source vocabularies

`origins` is required for every column carrying submission metadata and
lists one or more origins. Each origin's `type` is one of `Assigned`,
`Collected`, `Derived`, `Not Available`, `Other`, `Predecessor` or
`Protocol`, and its `source` one of `Investigator`, `Sponsor`, `Subject` or
`Vendor`. Both vocabularies are closed, and neither is extended by a
specification. Two origins with the same type and source, or a `Predecessor`
or `Not Available` origin beside another, fail, as does a column carrying
submission metadata without `origins`.

Fails: `origin_missing`, `conflicting_origins`.
Evidence: `sdtm-dm-metadata`, `schema-column-origins`, `negative-conflicting-origins`, `rust/crates/yamaa/tests/language_header.rs::a_column_declares_several_origins`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_column_submission_without_origins_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::two_origins_of_one_type_and_source_conflict_at_the_later`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::an_origin_beside_a_not_available_one_conflicts`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::an_origin_beside_a_predecessor_one_conflicts`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_value_level_entry_origins_conflict_naming_the_entry`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::distinct_origins_do_not_conflict`, `rust/crates/yamaa-core/tests/schema_submission.rs::portable_choice_and_identifier_failures_use_only_applicable_current_causes`.

A submission document states where every value came from, and a column with no
declared origin has no place in a submission document. An empty `origins`
fails as `origin_missing`, at the column's `submission`. A value-level entry
declares `origins` too, and one without origins of its own takes the column's
([SUB-13](#sub-13)). The origins of one column or entry are checked before
any input is read: each origin with the same type and source as an earlier
one, and each origin beside a `Predecessor` or `Not Available` one, fails
with `conflicting_origins` at its own `origins[<i>]`, naming the column, an
entry in `entry`, its type in `origin_type`, a declared source in
`origin_source`, and in `reason` either `same_type_and_source` or
`exclusive_type`. Which pairs a model admits, `Not Available` among them, is
[SUB-15](#sub-15)'s.

<a id="sub-15"></a>

### SUB-15. Each model admits its own origin type and source pairs

Which origin type and source pairs are admitted depends on the model. For
`SDTM`, the table below fixes them. For `SEND`, every type is admitted, and
`source` is either absent or a source the `SDTM` table admits for the type.
For `ADaM`, the admitted types are `Derived`, `Assigned`, `Predecessor` and
`Other`, and `source` is derived rather than declared: `Sponsor` for
`Derived`, `Assigned` and `Other`, and absent for `Predecessor`. A pair the
model does not admit fails.

Fails: `origin_source_not_admitted`.
Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_lb_value_entry_inheriting_the_original_investigator_source_is_refused_with_zero_writes`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_not_available_origin_is_not_admitted_under_sdtm`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_not_available_origin_with_a_source_is_not_admitted_under_sdtm`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_assigned_origin_from_the_investigator_is_not_admitted_under_send`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_not_available_origin_with_a_source_is_not_admitted_under_send`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::send_keeps_an_unsourced_not_available_origin_that_says_why`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::send_keeps_an_unsourced_assigned_origin`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_collected_origin_from_the_subject_is_not_admitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_sdtm_collected_origin_without_its_source_is_not_admitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_assigned_origin_from_the_investigator_is_not_admitted`.

| Type | Admitted sources for `SDTM` |
|---|---|
| `Collected` | `Subject`, `Investigator`, `Vendor` |
| `Derived` | `Vendor`, `Sponsor` |
| `Assigned` | `Vendor`, `Sponsor` |
| `Protocol` | `Sponsor` |
| `Other` | any |
| `Predecessor` | none; `source` must be absent |

`Not Available` is not admitted for `SDTM` or `ADaM`; a `SEND` origin keeps
it, for legacy data, without a source and with the description
[SUB-16](#sub-16) requires, which says why the value is not available. For
`ADaM`, a declared `source` is rejected, and `Collected`, `Protocol` and
`Not Available` are not admitted. A `SEND` source is optional:
the model identifies responsibility through its own trial-summary and
findings columns, and a declared source states it with the pairs the `SDTM`
table admits, so a `SEND` `Predecessor` or `Not Available` declares none. An
origin type and source pair the model does not admit, a `source` present where
these rules require absence, or one absent where they require presence fails
with `origin_source_not_admitted`.

A specification that declares its model checks the pairs on its own
([SUB-23](#sub-23)), for each origin a column or value-level entry declares,
at that origin's `origins[<i>]`; the context names the column, an entry in
`entry`, the origin's type in `origin_type`, its declared source in
`origin_source`, and the model in `model`. A document checks them when it
composes the dataset, for each origin, at its `origins[<i>]`.

<a id="sub-16"></a>

### SUB-16. A Predecessor, Other or Not Available origin carries a description

An origin's `description` is required when its `type` is `Predecessor`, and
names the dataset and column the value was copied from as
`<dataset>.<column>`, optionally followed by ` where ` and a condition
written for reviewers. The description is also required when its `type` is
`Other` or `Not Available`, and is optional elsewhere. A description absent where this rule requires it, or a
`Predecessor` description of another form, fails.

Fails: `origin_description_missing`.
Evidence: `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_other_origin_without_a_description_is_refused`, `rust/crates/yamaa-core/src/submission_origin.rs::predecessor_descriptions_name_one_column_and_an_optional_condition`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::predecessor_columns_copying_the_columns_their_descriptions_name_pass_the_check`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_predecessor_description_naming_no_column_is_not_the_form`.

Neither `Other` nor `Not Available` states anything alone. The dataset and the
column are identifiers joined by one period, and the condition after the
lowercase ` where ` names the records the predecessor column contributes, as
in `SUPPDM.QVAL where QNAM = 'RACEOTH'`. No rule parses the condition; a
generated document carries the whole description, and [SUB-20](#sub-20)
requires the copy to declare a `filter` when a condition is written. Each
origin is checked on its own. A description absent where this rule requires
it fails with `origin_description_missing`, reporting the column and the
origin's type, at the origin's `origins[<i>]`; a `Predecessor` description
of another form fails the same way at the origin's `description`.

<a id="sub-17"></a>

### SUB-17. A value collected from a person references the annotated case report form, with a page under `Submission`

A column with a `Collected` origin from `Investigator` or `Subject` references
the study's annotated case report form, and [Define-XML](define-xml.md)
rejects a reference naming any other document. Under `context: Submission`,
the column declares `documents` with at least one `pages` reference into the
form. Under `context: Other`, the reference is derived when `documents` is
omitted. Such an origin with no document reference, or under `Submission` with
no page reference, fails.

Fails: `origin_document_missing`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_collected_origin_citing_another_document_misses_the_annotated_crf`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_without_its_form_is_refused_under_submission`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_citing_its_form_without_a_page_is_refused_under_submission`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_without_its_form_cites_the_form_without_a_page_under_other`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_collected_origin_citing_its_form_without_a_page_is_admitted_under_other`, `python/tests/installed/installed_public_define.py::test_defaults_selection_bound_lists_and_required_acrf_aliases`.

A value collected from a person is traceable to the form it was collected on.
The annotated case report form is the only document the reference can name,
and every declared reference names it. A derived reference carries no page, so
it is admitted only under `Other`; a regulatory submission states the page a
reviewer opens, and [DEFINE-4](define-xml.md#define-4) lists the requirement.
Under `Submission`, omitted `documents`, or declared references none of which
carries `pages`, fail with `origin_document_missing`, reporting the column.
Each such origin of a column or entry is checked on its own, at its
`origins[<i>]`.

<a id="sub-18"></a>

### SUB-18. A Derived origin declares its method

A column with a `Derived` origin must declare `method`. A `Derived` origin
with no `method` fails.

Fails: `method_missing`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_derived_origin_from_a_bare_source_still_needs_its_method`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_derived_origin_without_a_method_is_refused`.

The algorithm is what a derived origin claims exists; a claim with no
algorithm attached is not traceable. One `Derived` origin among several
needs the method too, and the column or entry reports it once.

## What the derivation refutes

<a id="sub-19"></a>

### SUB-19. A declared origin the column's derivation contradicts is rejected, and an origin is never inferred

A declared origin that the [Column derivation](../columns.md) graph
contradicts is rejected, and an origin is never inferred. A bare read refutes
no type: a reference or a structured `source`, an `odm` read or an `aggregate`
whose `expr` is exactly `ONLY` of one variable, read directly or through a
plain lookup. A literal refutes `Derived` and `Predecessor`, a recode of
collected values `Protocol` and `Predecessor`, and any other derivation
`Collected`, `Protocol` and `Predecessor`. A row-derived column keeps only the
types all its `rows` entries admit, and no shared type fails.

Fails: `origin_contradicts_derivation`.
Evidence: `sdtm-dm-metadata`, `sdtm-lb-metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_assembled_identifier_refutes_a_protocol_origin`, `rust/crates/yamaa-application/tests/file_resources.rs::a_plain_dictionary_lookup_refutes_no_collected_origin_and_a_deriving_lookup_does`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::recodes_literals_and_stored_values_keep_the_origins_a_study_declares`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::not_available_is_admitted_for_arithmetic`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::not_available_is_admitted_for_a_case_over_output_columns`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_printed_unit_admits_a_protocol_origin`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_value_computed_upstream_and_read_as_stored_admits_a_collected_origin`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_case_reading_a_declared_input_dataset_is_a_recode_that_admits_collected`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::arithmetic_refutes_a_collected_origin`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_case_reading_an_output_column_refutes_a_collected_origin`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_recode_refutes_a_protocol_origin`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::duration_text_is_a_recode_that_refutes_a_protocol_origin`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::date_text_is_a_recode_that_refutes_a_protocol_origin`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_literal_refutes_a_derived_origin`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_only_pivot_and_a_plain_lookup_are_bare_reads_that_admit_collected`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_only_pivot_in_its_shorthand_admits_collected`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_reduction_other_than_only_refutes_a_collected_origin`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_lookup_that_derives_fields_refutes_a_collected_origin`, `rust/crates/yamaa-core/src/submission_origin.rs::every_derivation_row_refutes_exactly_its_published_types`, `rust/crates/yamaa-core/src/submission_origin.rs::only_the_reduction_of_one_variable_to_its_one_value_is_a_pivot`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_literal_derivation_refutes_a_derived_origin`.

| Derivation | Admitted `origin.type` | Refuted |
|---|---|---|
| a bare read: a `source`, an `odm` read or an `ONLY` pivot, directly or through a plain lookup | every type | none |
| a literal | `Assigned`, `Protocol`, `Collected`, `Other`, `Not Available` | `Derived`, `Predecessor` |
| a recode: `mapping`, `case`, `first_available`, `str_case`, `str_pad`, `str_substr`, `str_replace`, `str_extract`, `str_template`, `to_date`, `date_text`, `dtc` or `duration_text`, reading only as the paragraphs below state | `Collected`, `Derived`, `Assigned`, `Other`, `Not Available` | `Protocol`, `Predecessor` |
| anything else | `Derived`, `Assigned`, `Other`, `Not Available` | `Collected`, `Protocol`, `Predecessor` |

An `ONLY` pivot is an `aggregate` whose `expr` is exactly `ONLY(<variable>)`,
with no arithmetic around or inside the reduction, and which declares no
`derivations`. It turns long records, such as one ODM record per collected
item, into the one value its group or relation holds
([FORMULA-18](../formulas.md#formula-18)): its `filter`, `key` and `between`
choose the record, and the value is the variable as stored. A plain lookup is
a named intermediate that reads a dataset `input` declares and declares no
`derivations`: its `key`, `between`, `filter` and selection choose a record,
and every field read through it is that record's stored value. Either is a
bare read wherever this rule reads one.

A recode copies, recodes, reformats or assembles collected values. Each
variable it names is a bare read, never one through a named intermediate other
than a plain lookup, and each expression it nests in a `case` result
([EXPR-3](../expressions.md#expr-3)) is a literal, a bare read, or a recode.
Every condition and result of a `case` reads only declared input values or
literals: each identifier of a `when` predicate, and each variable a result
reads, is qualified by a dataset `input` declares or by a plain lookup of one,
or is an `odm` read. A recode that reads otherwise, through another named
intermediate or, in a `case`, from an output column, is anything else.

Anything else computes its value: arithmetic and rounding (text in an
expression field that computes, `round_half_away_from_zero` and `str_decimal`),
every aggregation but an `ONLY` pivot, windows, temporal computation (the date
and datetime operations other than `to_date`, `date_text`, `dtc` and
`duration_text`), `flag`, `cut`, `str_length`, `greatest` and `least`,
`function`, and every read through a named intermediate that is not a plain
lookup: one that declares `derivations`, whose fields it computes, or one over
`SELF`, the completed output rows. Each type it refutes asserts the value
arrived as it stands, and the specification shows it did not.

The graph cannot tell an investigator-recorded value from a vendor-transmitted
or protocol-fixed value, and it refutes only what the derivation proves. An
SDTM recode of collected terminology (`SEX` from `Male` to `M` through
`mapping`), a case normalization (`str_case`), an ISO 8601 assembly of
collected date parts (`dtc`, `str_template` or `to_date`), a collected date
read from its declared form into ISO 8601 text (`date_text`) and a collected
duration written as ISO 8601 duration text (`duration_text`) admit `Collected`.
A value printed on the case report form, such as a unit or a test name, is
written as a literal and admits `Collected`; a literal reads no input, so it
refutes `Derived`. A vendor- or EDC-computed value read as stored, such as an
eGFR or a BMI, is a bare `source` and admits `Derived`, with the method
[SUB-18](#sub-18) requires. A laboratory result pivoted with `ONLY` from long
ODM records admits `Collected`, as the same result read from a wide record
would. `Not Available` claims nothing a derivation can contradict, so every
derivation admits it.

A column derived by a bare `source` copies one value of one declared dataset,
a bare `odm` read copies the `Value` of the one ODM record it identifies
([NAME-13](../names.md#name-13)), an `ONLY` pivot copies the one value its
group or relation holds, and a read through a plain lookup copies a stored
field of the record the lookup selects: nothing was computed. Whether that
stored value was collected, assigned, fixed by the protocol, computed upstream
or copied from a predecessor dataset is a fact about the source and not about
this specification; [SUB-20](#sub-20) checks the copy a `Predecessor` claims,
which only a bare `source` of the predecessor's input dataset, or a read
through a plain lookup of that dataset, makes.

When the entries of a row-derived column disagree, no shared type is an error,
not a silently admitted origin. A row template every row of which a value-level
entry declaring origins of its own selects answers to that entry instead
([SUB-13](#sub-13)). A column with several origins has each declared type
checked against its derivation, as one origin is, so every type it declares
is one the derivation admits. A refuted origin fails with
`origin_contradicts_derivation`, reporting the column, the declared type and
the derivation that refutes it.

<a id="sub-20"></a>

### SUB-20. A Predecessor column is a bare copy of the column its description names

A column with a `Predecessor` origin copies its values unchanged from the
predecessor dataset and column the origin's `description` names, so its
derivation must read that column of that dataset, directly or through a plain
lookup of it, with a `filter` when the description writes a where clause; a
`copy` entry declares such columns. When the column's name equals
the predecessor column's name and a specification some submission section of
the project file lists declares that column, their labels must be equal; a
renamed column keeps its own label.

Fails: `origin_contradicts_derivation`, `refuted_value_origin`, `predecessor_label_mismatch`.
Evidence: `negative-mandatory-arm-missing`, `schema-copy-producer-columns`, `rust/crates/yamaa/tests/copy_entry.rs::each_copy_refusal_names_what_it_refuses`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::predecessor_columns_copying_the_columns_their_descriptions_name_pass_the_check`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_value_level_predecessor_copying_the_column_its_description_names_passes_the_check`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_predecessor_column_reading_another_column_is_refuted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_predecessor_column_without_its_descriptions_filter_is_refuted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_computed_predecessor_column_is_refuted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_only_pivot_and_a_plain_lookup_are_bare_reads_that_admit_collected`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_plain_lookup_over_the_predecessors_dataset_copies_its_column`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_filtered_plain_lookup_copies_a_predecessor_its_description_selects`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_plain_lookup_over_another_dataset_is_no_predecessor_copy`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_plain_lookup_answering_absence_with_a_value_is_no_predecessor_copy`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_plain_lookup_without_the_descriptions_filter_is_refuted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::predecessor_labels_and_the_dataset_inventory_span_every_section`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_predecessor_column_keeps_its_predecessors_label`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_predecessor_column_labelled_otherwise_than_its_predecessor_is_refused`.

The derivation reads the predecessor through the input dataset declared under
the predecessor's name: a `description` of `DM.AGE` requires the bare reference
`DM.AGE`, and `SUPPDM.QVAL where QNAM = 'RACEOTH'` requires a structured
`source` of `SUPPDM.QVAL` that declares a `filter`. A plain lookup of the
predecessor's input, a named intermediate over that declared input with no
`derivations` ([SUB-19](#sub-19)), copies the column too: with
`intermediates: [{id: SUBJ, dataset: ADSL, key: {USUBJID: AE.USUBJID}}]`, the
read `SUBJ.AGE` copies `ADSL.AGE`, and a written condition then requires the
lookup, or the structured `source` reading through it, to declare a `filter`.
A pair relation reads two datasets and copies through neither, and a lookup
whose `absent` answers with a value other than null writes a value no record
stores. No rule
compares the filter with the written condition. Any other derivation computes
rather than copies, and the refutation of [SUB-19](#sub-19) already rejects a
computed column that claims otherwise, as it does a read through a lookup that
declares `derivations`. An `ONLY` pivot is a bare read under
[SUB-19](#sub-19), but it is no `source` of the predecessor's input dataset. A
bare `source` of another dataset or column, such a pivot, a read through a
lookup of another dataset, or a read without the `filter` a written condition
requires, fails with `origin_contradicts_derivation`, reporting the column,
the predecessor and the reason. A value-level `Predecessor` entry is checked against the
derivations that can produce the rows it selects and fails with
`refuted_value_origin`, as [SUB-13](#sub-13) states.

A `copy` entry ([SPEC-9](../specification.md#spec-9)) gives each copied column
an origin whose `description` names the producer's `domain`, and a derivation
that reads through the input the entry names. A copy through an input whose
name is not its producer's `domain` therefore reads another dataset than its
description names, and fails with `origin_contradicts_derivation`: the input
is named after the producer's domain instead.

The copy is checked when the specification is validated on its own. The
labels are compared when a document composes the specification: composition
reads the specifications every submission section of the project file
lists, and a predecessor column no listed specification declares, or declares
without a label, is not compared. A copy that keeps the name keeps the label,
while a renamed column such as `PARAMCD` from `LBTESTCD` describes its own
content. A label that differs fails with `predecessor_label_mismatch` at the
column's `label`, reporting the predecessor and both labels.

## Methods, comments and documents

<a id="sub-21"></a>

### SUB-21. A method states its algorithm in prose and may add a name, a kind, code and documents

`method` states the algorithm that produces a column's value. Written as a
string, it is the algorithm's prose description; written as a mapping, it adds
a name, a kind, machine-readable code and supporting documents, and
`description` is required. `type` defaults to `Computation`; a method that
replaces a missing value with a substitute declares `Imputation`. `expression`
carries code as text under a named `context`, which this language neither
parses nor evaluates.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/schema_submission.rs::complete_nested_metadata_retains_integer_text_documents_pages_and_order_after_drop`, `rust/crates/yamaa-core/tests/schema_submission.rs::all_core_origin_source_and_method_choices_are_retained`.

A generated document carries the code unchanged. No rule checks the code
against the specification's derivation: an expression is sponsor-supplied
documentation, not a second definition of the column. No `expression` is
generated from a derivation, which has no canonical written form in this
language; the field is declared or absent.

<a id="sub-22"></a>

### SUB-22. Comments and document references are declared where they are used and name declared documents

`comment` carries a sponsor note about a dataset or a column. Written as a
string, it is the note's text; written as a mapping, it adds the documents the
note refers to. Comments are declared where they are used, and one comment is
not shared between two datasets or two columns. A document reference names a
document the study document declares, and optionally the place within it: as a
string the reference is the identifier, and as a mapping it adds `pages`.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/schema_submission.rs::complete_nested_metadata_retains_integer_text_documents_pages_and_order_after_drop`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_origin_citing_an_undeclared_document_is_refused`.

[Define-XML](define-xml.md) mints each comment an identifier from the place it
is declared, so a shared comment would need an identifier space the profile
does not open. A `pages` reference states whether the `refs` are physical page
numbers or named destinations and carries the `refs` as a space-separated
list.

The mapping forms are written only for what they add: a comment mapping
holding only `text`, a method mapping holding only `description` and a
document reference holding only `document` restate their string and fail as
`redundant_long_form` under
[SCHEMA-15](../reference/schema-language.md#schema-15).

## When the profile is checked

<a id="sub-23"></a>

### SUB-23. Rules needing no project file are checked on the specification alone, and the rest when a document composes it

Every rule of this profile that needs no project file is checked when the
specification is validated on its own, including each rule that depends on
the model the specification declares ([SUB-3](#sub-3)), such as core
designations, origin pairs and date types, and the resolution of
`mandatory`. A rule that reads the project file, such as the guide a section
binds, the documents a section declares or the labels of a predecessor
another listed specification declares, is checked when
[Define-XML](define-xml.md) composes the specification into a document.

Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_reserved_metadata_key_is_refused_before_study_access`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_lb_value_entry_inheriting_the_original_investigator_source_is_refused_with_zero_writes`, `rust/crates/yamaa-application/tests/file_resources.rs::a_published_unit_outside_its_value_level_codelist_is_refused_with_zero_writes`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_specification_declaring_its_model_metadata_passes_the_check`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_sdtm_specification_declaring_its_model_metadata_passes_the_check`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_column_core_the_model_does_not_publish_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_sdtm_collected_origin_without_its_source_is_not_admitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_sdtm_date_is_not_submitted_as_an_integer`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_column_without_mandatory_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_column_without_core_or_mandatory_passes_without_dataset_metadata`.

Checked when the specification is validated on its own:

- the vocabularies;
- the required and prohibited field combinations, `model` among them
  ([SUB-3](#sub-3));
- the length binding of [SUB-8](#sub-8) and the date and time forms of
  [SUB-26](#sub-26);
- the refusal of a `datetime` column under a tabulation class
  ([SUB-27](#sub-27));
- the graph refutations of [SUB-19](#sub-19), the form of a `Predecessor`
  description ([SUB-16](#sub-16)) and the copy it claims
  ([SUB-20](#sub-20));
- the value-level declaration rules of [SUB-12](#sub-12) and the entry
  origin refutation of [SUB-13](#sub-13);
- the presence of `method` for a declared `Derived` origin
  ([SUB-18](#sub-18));
- the resolution of `mandatory` from the keys, its declaration and the class's
  group, a key column's declared `mandatory: false`, and the presence check
  and redundant `not_missing` of [SUB-10](#sub-10) and [SUB-11](#sub-11);
- with a declared model, the core designations and `core`/`mandatory`
  conflicts of [SUB-10](#sub-10), the agreement of the class's group with the
  model, the date types of [SUB-6](#sub-6) and [SUB-25](#sub-25), the origin
  pairs of [SUB-15](#sub-15), and the prohibitions of [SUB-5](#sub-5) and
  [SUB-9](#sub-9).

Checked when a document composes the specification:

- the section's listing and the guide it binds ([SUB-3](#sub-3));
- the `length` a `text` column or value-level entry omits ([SUB-7](#sub-7)),
  which the section's transport supplies;
- the annotated case report form references of [SUB-17](#sub-17), which name
  the section's documents and depend on its `context`, and the predecessor
  labels of [SUB-20](#sub-20), which read every section's specifications;
- the rules that depend on the model again, under the section's model, which
  [SUB-3](#sub-3) makes the specification's.

The row checks of [SUB-13](#sub-13) run in every build of the specification,
before publication. The transport lengths of [SUB-24](#sub-24) are measured
after composition, from the published artifacts, when a transport is
requested.

The specification names its model, and only the study document names the
standard its identifier refers to. A specification that is never composed is
not yet part of a submission.

## Transport lengths and dates

<a id="sub-24"></a>

### SUB-24. With a transport, a character column is as long as the longest value the section's datasets carry

When the selected section requests a transport, a character column's length is
the longest published value, in bytes and at least 1, across every composed
dataset of the section that carries a column of that name; a SUPP-- dataset is
measured on its own. A value-level definition's length is the longest value
over the rows its where clause selects. A `text` column's document `Length` and
Dataset-JSON `length` are that length, and its SAS Transport variable that
wide; the declared `length` stays the bound [SUB-8](#sub-8) enforces.

Evidence: `sdtm-dm-metadata`, `sdtm-dm-xpt`, `adam-adsl-xpt`, `sdtm-suppdm-metadata`, `rust/crates/yamaa-application/tests/submission_transport.rs::shared_column_lengths_span_datasets_and_supplemental_datasets_stand_alone`.

A character column is a `str` column, and a `date` outside `adam`, which
travels as its canonical text ([XPT-6](xpt.md#xpt-6)); only a `text`
column or value-level definition carries a `Length`. A SUPP-- dataset is one
whose name begins with `SUPP`: its lengths come from its own values alone and
do not lengthen another dataset's column. A dataset declared to have no data
contributes no value, and a missing value occupies no bytes. An `int` or
`float` column keeps its declared `length`. A `text` column that omits
`length` ([SUB-7](#sub-7)) has no bound, only its published length. Without a
transport no artifact is read, and the declared `length` is the document's
`Length`.

The published length counts UTF-8 bytes, and the bound [SUB-8](#sub-8)
enforces counts Unicode scalar values, so the two counts of a value differ
only for text outside ASCII. Under `transport: xpt`, and under any transport
with `context: Submission`, every published text value is printable ASCII
([XPT-7](xpt.md#xpt-7)), one byte to each scalar value. Under
`context: Other` a Dataset-JSON file carries any text, and a column's document
`Length` and Dataset-JSON `length` can exceed its declared `length` while
every value meets that bound: three ASCII letters and `U+00E9` are four scalar
values within a `length` of 4, and five bytes of published length.

<a id="sub-25"></a>

### SUB-25. Under `ADaM` a date or datetime is submitted as a SAS numeric date with a fixed display format and length

Under the `ADaM` model, a `date` column is submitted as SAS days since
1960-01-01 with display format `DATE9.`, and a `datetime` column as seconds
since 1960-01-01T00:00:00 with `DATETIME20.`. The document describes each as
`DataType` `integer`, `Length` 8 and that `def:DisplayFormat`. A declared
`data_type` other than `integer`, or a declared `display_format` other than that
format, fails when a document composes the specification. Under `SDTM` and
`SEND` a `date` keeps its own type and canonical text, and
[SUB-27](#sub-27) refuses a `datetime`.

Fails: `submission_data_type_not_admitted`, `value_not_permitted`.
Evidence: `adam-adsl-xpt`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::an_adam_date_with_another_display_format_is_not_permitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_adam_date_takes_the_fixed_display_format`.

Dataset-JSON keeps the ISO value and asks for the integer with
`targetDataType` ([JSON-10](dataset-json.md#json-10)), and a SAS Transport file
writes the number ([XPT-6](xpt.md#xpt-6)). A conflicting `display_format` fails
with `value_not_permitted` at the column's `display_format`; a conflicting
`data_type` fails as [SUB-6](#sub-6) states. No `length` is declared on such a
column ([SUB-7](#sub-7)). A specification that declares `model: ADaM` fails
both on its own ([SUB-23](#sub-23)), wherever a column or value-level entry
declares them; the `display_format` failure names the column, the declared
format in `value` and the fixed one in `permitted`.

<a id="sub-27"></a>

### SUB-27. An `SDTM` or `SEND` dataset carries its dates and times as text, never in a `datetime` column

A dataset whose `submission.class` is a tabulation class of
[SUB-10](#sub-10), so of the `SDTM` or `SEND` model, must not carry a
`datetime` column in `output.columns`: such a column writes the seconds that
the canonical text of [VALUE-17](../values.md#value-17) always shows, collected
or not. Its dates and times are `str` columns with a date or time `data_type`
([SUB-26](#sub-26)). A `datetime` output column fails validation at its
`type`; an internal one is not refused.

Fails: `value_not_permitted`.
Evidence: `negative-sdtm-datetime-column`, `sdtm-vs-epoch-from-subject-elements`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_findings_dataset_refuses_a_datetime_output_column`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_findings_dataset_keeps_a_datetime_column_it_does_not_output`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_analysis_dataset_outputs_a_datetime_column`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_dataset_without_a_class_outputs_a_datetime_column`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_sdtm_document_refuses_a_datetime_output_column`.

The model's group is read from the dataset's class, as the resolution of
`mandatory` reads it, so the refusal needs no study document; a dataset that
declares no class is not checked, and a document composing an `SDTM` or `SEND`
dataset refuses such a column the same way. The failure reports `value_not_permitted`
at `columns.<column>.type` with the column, the value `datetime`, the class and
the permitted column types `str` and `date`, or at
`<section>.specs[<n>].columns.<column>.type` when a document composes the
dataset. A `date` column is admitted: its
canonical text carries the day alone. An internal `datetime`, such as the
moment an element lookup compares, never reaches the artifact. Text collected
to the minute stays `2025-03-04T08:30` in a `str` column, where a `datetime`
column would write `2025-03-04T08:30:00`.
