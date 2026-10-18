# Submission metadata

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
Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionColumnNotOutput`, `rust/crates/yamaa-core/tests/schema_submission.rs::absent_submission_remains_absent_and_preserves_ordinary_column_declarations`.
Replaces: REQ-0855, REQ-0856, REQ-0886, REQ-0909, REQ-1131, REQ-1148.

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
Evidence: `schema-column-composition`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::metadata_admission_has_exact_causes_before_study_access`, `rust/crates/yamaa-core/tests/project_execution.rs::submission_claims_are_admitted_and_reserved_metadata_has_a_registered_cause`.
Replaces: REQ-0857, REQ-0858, REQ-0910.

A study moves governed facts from the map to submission metadata. A reserved
key fails validation with `reserved_metadata_key`, reporting the key.

## Standard families

<a id="sub-3"></a>

### SUB-3. A dataset's family is read from the implementation guide the study document binds it to

Every dataset follows one foundational standard, and the study document binds
it: [DEFINE-6](define-xml.md#define-6) resolves each dataset to a declared
standard of type `IG`. A specification does not name the standard. This
profile reads the bound standard's published name as one of three closed
families, as the table below fixes. A dataset bound to an `IG` standard outside
the table, or to a standard of type `CT`, fails.

Fails: `unknown_standard_family`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionStandardFamily`, `python/tests/installed/installed_public_define.py::test_missing_artifact_only_when_requested_and_semantic_failure_zero_writes`.
Replaces: REQ-0859, REQ-0860, REQ-0861, REQ-0924.

| Family | Standard names |
|---|---|
| `sdtm` | `SDTMIG`, `SDTMIG-AP`, `SDTMIG-MD` |
| `send` | `SENDIG`, `SENDIG-AR`, `SENDIG-DART`, `SENDIG-GENETOX` |
| `adam` | `ADaMIG`, `ADaMIG-MD` |

The table is closed, and `BIMO` is excluded. A dataset must not be bound to a
standard of type `CT`: controlled terminology qualifies a codelist and not a
dataset, and [Controlled terminology](terminology.md) owns that binding. The
failure reports the dataset and the standard;
[DEFINE-6](define-xml.md#define-6) owns the binding that fails.

## Dataset metadata

<a id="sub-4"></a>

### SUB-4. Dataset metadata states its label, observation class, record structure and whether it repeats

`label`, `class`, `structure` and `repeating` are required. `class` names the
general observation class in the exact spelling and case the vocabulary
publishes, and `subclass` is declared only when the dataset's implementation
guide defines one for it. `reference_data` defaults to false, and a dataset
declaring `reference_data: true` must declare `repeating: false`.
`has_no_data` defaults to false and records that the submitted dataset has no
data.

Fails: `reference_data_repeating_conflict`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionReferenceRepeating`, `rust/crates/yamaa-core/tests/schema_submission.rs::dataset_defaults_and_comment_survive_original_document_drop`.
Replaces: REQ-0862, REQ-0863, REQ-0864, REQ-0865, REQ-0925, REQ-1130, REQ-1146, REQ-1147, REQ-1296.

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
domain execution or replace row-count checks.

<a id="sub-5"></a>

### SUB-5. A dataset's domain defaults to the specification's domain and is declared only for sdtm and send

`domain` names the domain the dataset belongs to. It defaults to the
specification's `domain`, so an ordinary dataset states the domain once. A
split or supplemental dataset declares the domain; the dataset's own name is
not its parent domain. `domain` is declared only for a dataset whose family is
`sdtm` or `send`: the `adam` family has no domain, and declaring one there
fails validation, reporting the field.

Fails: `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionAdamField`.
Replaces: REQ-0866, REQ-0867, REQ-0917.

A domain declared for the `adam` family is an error rather than an unused
value. A `role` declared for that family fails the same way under
[SUB-9](#sub-9).

## Column metadata

<a id="sub-6"></a>

### SUB-6. A column's submission data type defaults from its column type and family, within the closed set that type admits

`data_type` states the type the submission represents the column as. It
defaults from the declared column type and, for a `date` or `datetime`, from
the family ([SUB-25](#sub-25)); each declared type admits the closed set the
table below fixes. `data_type` never changes a value: it states how a value
[Values](../values.md) already typed appears in the document, and no
derivation, key, order term or artifact byte can observe it; only the check of
a date or time type's form ([SUB-26](#sub-26)) reads it. A `data_type` outside
the admitted set fails validation.

Fails: `submission_data_type_not_admitted`.
Evidence: `sdtm-vs-epoch-from-subject-elements`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionDataType`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::submission_fields_do_not_change_any_original_domain_artifact`.
Replaces: REQ-0868, REQ-0869, REQ-0870, REQ-0914, REQ-1145.

| Declared type | Default | Also admits |
|---|---|---|
| `str` | `text` | the types listed below |
| `int` | `integer` | none |
| `float` | `float` | none |
| `date` | `date`; `integer` under `adam` | none |
| `datetime` | `datetime`; `integer` under `adam` | none |

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
already decides integer or binary64. The family decides a temporal column's
one type, so the specification alone admits a `date` or `datetime` declared
`integer` as well as its own type, and a document admits only the type its
family submits: its own under `sdtm` and `send`, and `integer` under `adam`.
The failure reports the column, the declared type and the submission type.

<a id="sub-7"></a>

### SUB-7. `length` and `significant_digits` are declared exactly where the submission data type needs them

`length` is the maximum expected value length, as the submission standard
defines the term: a property of the column declaration, not of the data one run
produced, and a positive integer. It is required when `data_type` resolves to
`text`, `integer` or `float`, unless [SUB-8](#sub-8) derives it, and must not
be declared otherwise, nor ever on a `date` or `datetime` column.
`significant_digits` counts digits after the decimal point, is a non-negative
integer, and is required when `data_type` resolves to `float` and must not be
declared otherwise.

Fails: `submission_length_missing`, `submission_length_not_applicable`, `submission_length_invalid`.
Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionLengthMissing`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::effective_value_metadata_rejects_invalid_bounds_types_and_explicit_length_conflicts_before_data`.
Replaces: REQ-0871, REQ-0872, REQ-0873, REQ-0911, REQ-0912.

The other submission types carry fixed-form values, so a length for such a type
restates that form. A `date` or `datetime` column has the fixed length its
family gives it, none under `sdtm` and `send` and 8 under `adam`
([SUB-25](#sub-25)), whatever `data_type` it declares. A `length` that is not a
positive integer, or a negative `significant_digits`, fails validation with
`submission_length_invalid`. A `length` or `significant_digits` absent where
this rule requires it fails with `submission_length_missing`, and one declared
where this rule prohibits it fails with `submission_length_not_applicable`,
reporting the column.

<a id="sub-8"></a>

### SUB-8. A `str` column's length is a bound its completed values must meet, stated once

A `length` on a `str` column is enforced: the column's completed values must
contain at most `length` [Values](../values.md) scalar values, as
[Verification](../verification.md)'s `max_length` requires. On a `str` column
with a `max_length` verification, `length` is derived from the verification's
`max` and need not be declared; declaring both is accepted when equal and
rejected when different. A `length` on another column type is carried
unchecked, and a declared bound is the document's `Length` only when no
transport is requested ([SUB-24](#sub-24)).

Fails: `declared_length_conflict`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::declared_length_is_checked_once_over_unicode_scalar_values`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::metadata_admission_has_exact_causes_before_study_access`.
Replaces: REQ-0874, REQ-0875, REQ-0876, REQ-0913.

The declared length binds rather than describes. `length` and `max_length`
state one bound, and the bound is checked once either way. A `length` and a
`max_length` verification declaring different bounds on the same column fail
validation with `declared_length_conflict`, reporting the column and both
bounds. A `str` column with neither declaration, where [SUB-7](#sub-7) requires
a length, fails as [SUB-7](#sub-7) states.

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
Evidence: `sdtm-vs-epoch-from-subject-elements`, `negative-data-type-form`, `rust/crates/yamaa-core/tests/fixtures/submission_data_types.tsv`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::date_and_time_data_types_check_each_completed_value_against_their_form`.

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
its dataset, in the vocabulary the dataset's standard publishes, which this
language does not close. `role` must not be declared for a dataset whose family
is `adam`; a role there fails validation, reporting the field.

Fails: `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionAdamField`, `rust/crates/yamaa-core/tests/schema_submission.rs::complete_nested_metadata_retains_integer_text_documents_pages_and_order_after_drop`.
Replaces: REQ-0877, REQ-0878, REQ-0879.

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
display in `display_format`. Each implementation guide
publishes and versions its own role vocabulary. The `adam` family defines no
role vocabulary, so a role in that family would name nothing.

<a id="sub-10"></a>

### SUB-10. `core` uses its family's designations, and `mandatory` resolves from the keys, its declaration and the dataset's class

`core` is the standard's core designation and is declared for every family:
`Req`, `Exp` or `Perm` for `sdtm` and `send`, and `Req`, `Cond` or `Perm` for
`adam`. `mandatory` states whether every completed value must be present and
resolves without a study document. A key column is mandatory, and declaring
`mandatory: false` on one fails. Otherwise a declared `mandatory` wins, and
`adam` must declare one; without one, a column is mandatory exactly when its
dataset's class is a tabulation class and its `core` is `Req`.

Fails: `core_mandatory_conflict`, `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `negative-required-sex-missing`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::mandatory_columns_are_checked_without_a_study_document`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::restated_facts_and_relaxed_keys_fail_validation_before_data`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionCoreMandatory`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionClassFamily`.
Replaces: REQ-0880, REQ-0881, REQ-0882, REQ-0883, REQ-0884, REQ-0915, REQ-1141.

For `sdtm` and `send`, `Req` is required, `Exp` expected and `Perm`
permissible. For `adam`, `Req` is required, `Cond` conditionally required and
`Perm` permissible, as the ADaM implementation guides publish them. An `Exp`
for `adam`, or a `Cond` for `sdtm` or `send`, names no designation its standard
publishes.

The dataset's `submission.class` names its family group, so the domain build
resolves `mandatory` from the specification alone:

| Group | Classes | Families |
|---|---|---|
| tabulation | `EVENTS`, `FINDINGS`, `FINDINGS ABOUT`, `INTERVENTIONS`, `RELATIONSHIP`, `SPECIAL PURPOSE`, `STUDY REFERENCE`, `TRIAL DESIGN` | `sdtm`, `send` |
| analysis | `ADAM OTHER`, `BASIC DATA STRUCTURE`, `DEVICE LEVEL ANALYSIS DATASET`, `MEDICAL DEVICE BASIC DATA STRUCTURE`, `MEDICAL DEVICE OCCURRENCE DATA STRUCTURE`, `OCCURRENCE DATA STRUCTURE`, `REFERENCE DATA STRUCTURE`, `SUBJECT LEVEL ANALYSIS DATASET` | `adam` |

A dataset that declares no class resolves `mandatory` from its keys and its
declarations alone. A document that composes a dataset whose class belongs to
the other group fails with `value_not_permitted`, reporting `submission.class`:
the group the domain build read must be the family the study document binds.

For a tabulation class, `core` derives `mandatory`: `Req` gives true, and `Exp`
and `Perm` give false. A declared `mandatory: true` on an `Exp` or `Perm`
column is an accepted sponsor restriction. A declared `mandatory: false` on a
`Req` column is rejected when a document composes the dataset: the standard
requires the column, and a specification cannot relax its standard by
declaration.

For `adam`, the ADaM guides state that `Core` assists preparation and allows
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

Each of these fails with `core_mandatory_conflict` when a document composes
the dataset, reporting the column:

- a `core` absent for any family;
- an `Exp` for `adam`;
- a `Cond` for `sdtm` or `send`;
- a `mandatory: false` on a `Req` column for `sdtm` or `send`;
- a `mandatory` absent for `adam`.

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
Evidence: `negative-mandatory-arm-missing`, `negative-required-sex-missing`, `negative-value-level-mandatory-missing`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::value_level_mandatory_applies_only_to_the_rows_of_its_test_code`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::restated_facts_and_relaxed_keys_fail_validation_before_data`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionMandatoryRedundant`.
Replaces: REQ-0885, REQ-0916.

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
override the column's on the rows the clause selects; a field it leaves absent
inherits the column's. `where` maps declared `str` output columns, other than
the column itself, to nonempty lists of distinct values, and selects a row
when every listed column holds one of its values. Two entries whose where
clauses can select one row fail validation.

Fails: `value_level_where_invalid`, `duplicate_identifier`, `value_level_overlap`.
Evidence: `sdtm-lb-metadata`, `adam-adlb-metadata`, `sdtm-suppdm-metadata`, `negative-value-level-overlap`, `negative-value-level-where-column`, `negative-numeric-test-code`, `rust/crates/yamaa-core/src/submission_values.rs::static_findings_name_the_entry_column_and_fault`, `rust/crates/yamaa-core/tests/schema_submission.rs::each_independent_value_admission_control_has_the_required_condition_and_entry_path`, `rust/crates/yamaa-core/tests/schema_submission.rs::entries_inherit_complete_metadata_and_preserve_written_occurrences`.
Replaces: REQ-1162, REQ-1163, REQ-1164, REQ-1165, REQ-1166.

The column-level `submission` stays the shared declaration for the column,
and each entry supplies the metadata of the rows its where clause selects. An
ADaM `AVAL` can thus state each parameter's precision by `PARAMCD`, a
supplemental qualifier dataset each qualifier's codelist by `QNAM`, and a
findings domain each test's origin by `<DOMAIN>TESTCD`, without repeating the
shared metadata.

An entry declares any column metadata field except `value_level`: `core`,
`mandatory`, `role`, `data_type`, `length`, `significant_digits`,
`display_format`, `codelist`, `inventory_vocabulary`, `origin`, `method` and
`comment`, and `label`, the description of its definition
([DEFINE-27](define-xml.md#define-27)), which defaults to the column's
`label`. The entry's metadata merges field by field over the column's: a
field the entry declares replaces the column's whole, nested classes such as
`origin` included, and every rule of this profile applies to the merged
metadata. An entry is reported at
`columns.<column>.submission.value_level.<id>`.

One value in a where clause selects the rows holding it, several values the
rows holding any of them, and several columns the rows meeting every column.
A where column is compared on its completed value under [Values](../values.md)
equality, and a missing value matches no listed value. Listed values are
strings, which a value of another type never equals under
[VALUE-19](../values.md#value-19), so a where column must be `str`. A where
clause that names no column, or a where column that is the column itself, is
not a declared column, is not in `output.columns`, is not of type `str`, lists
no value or lists one value twice, fails with `value_level_where_invalid` at
`<entry>.where.<where column>`, or at `<entry>.where` for a clause without
columns, reporting the column, the entry, the where column and the `reason`:
`no_columns`, `self`, `undeclared`, `not_output`, `not_str`, `no_values` or
`duplicate_value`. A schema-valid `id` is an identifier; two entries of one
column with the same `id` fail with `duplicate_identifier` at `<entry>.id`.

Validation decides that two where clauses can select one row when one
clause's columns are all columns of the other and every column they share
lists a common value: the clauses then select at least one common row
whatever the data. The failure is `value_level_overlap`, naming both where
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
row is a `warning`. An entry's own origin is refuted under
[SUB-19](#sub-19) against every derivation of the column that can produce a
row its where clause selects, and against every derivation of the column when
that cannot be known.

Fails: `value_level_multiple_matches`, `value_level_unmatched`, `refuted_value_origin`.
Evidence: `negative-value-level-ambiguous-row`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::a_row_two_entries_select_fails_with_its_keys_and_both_entries`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::an_entry_that_matches_no_row_is_a_warning_and_scoped_checks_read_their_rows`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::predecessor_columns_copy_the_column_their_description_names`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionValueOriginRefuted`.
Replaces: REQ-1167, REQ-1168.

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
an entry selects when it derives a where column as a bare `literal` outside
the entry's values, a missing literal included, and every row it produces is
selected when it derives every where column as a literal among them. An
entry's origin is refuted against the column's derivation in every
construction that can produce its rows, with the table and the copy check of
[SUB-19](#sub-19) and [SUB-20](#sub-20); the column's own origin answers for
every construction no entry declaring its own origin selects completely. An
entry without its own origin answers with the column's, which the column's
refutation covers. A refuted entry fails with `refuted_value_origin` at the
entry's path, reporting the column, the entry, the refuting derivation and the
declared type.

## Origin

<a id="sub-14"></a>

### SUB-14. Every column with submission metadata declares an origin from closed type and source vocabularies

`origin` is required for every column carrying submission metadata.
`origin.type` is one of `Assigned`, `Collected`, `Derived`, `Not Available`,
`Other`, `Predecessor` or `Protocol`. `origin.source` is one of
`Investigator`, `Sponsor`, `Subject` or `Vendor`. Both vocabularies are closed,
and neither is extended by a specification. An `origin` absent from a column
carrying submission metadata fails.

Fails: `origin_missing`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionOriginMissing`, `rust/crates/yamaa-core/tests/schema_submission.rs::portable_choice_and_identifier_failures_use_only_applicable_current_causes`.
Replaces: REQ-0887, REQ-0888, REQ-0918, REQ-1132, REQ-1142, REQ-1143.

A submission document states where every value came from, and a column with no
declared origin has no place in a submission document.

<a id="sub-15"></a>

### SUB-15. Each family admits its own origin type and source pairs

Which origin type and source pairs are admitted depends on the family. For
`sdtm`, the table below fixes them. For `send`, every type is admitted, and
`source` is either absent or a source the `sdtm` table admits for the type.
For `adam`, the admitted types are `Derived`, `Assigned`, `Predecessor` and
`Other`, and `source` is derived rather than declared: `Sponsor` for
`Derived`, `Assigned` and `Other`, and absent for `Predecessor`. A pair the
family does not admit fails.

Fails: `origin_source_not_admitted`.
Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::original_lb_source_rejection_and_per_value_published_vocabulary_have_zero_writes`, `rust/crates/yamaa-adapters/tests/file_resources.rs::not_available_and_send_sources_follow_the_family_pairs`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionOriginSource`.
Replaces: REQ-0889, REQ-0890, REQ-0891, REQ-0919.

| Type | Admitted sources for `sdtm` |
|---|---|
| `Collected` | `Subject`, `Investigator`, `Vendor` |
| `Derived` | `Vendor`, `Sponsor` |
| `Assigned` | `Vendor`, `Sponsor` |
| `Protocol` | `Sponsor` |
| `Other` | any |
| `Predecessor` | none; `source` must be absent |
| `Not Available` | none; `source` must be absent |

An `sdtm` `Not Available` origin declares no source and carries the
description [SUB-16](#sub-16) requires, which says why the value is not
available. For `adam`, a declared `source` is rejected, and `Collected`,
`Protocol` and `Not Available` are not admitted. A `send` source is optional:
the family identifies responsibility through its own trial-summary and
findings columns, and a declared source states it with the pairs the `sdtm`
table admits, so a `send` `Predecessor` or `Not Available` declares none. An
origin type and source pair the family does not admit, a `source` present where
these rules require absence, or one absent where they require presence fails
with `origin_source_not_admitted`.

<a id="sub-16"></a>

### SUB-16. A Predecessor, Other or Not Available origin carries a description

`origin.description` is required when `type` is `Predecessor`, and names the
dataset and column the value was copied from as `<dataset>.<column>`,
optionally followed by ` where ` and a condition written for reviewers. The
description is also required when `type` is `Other` or `Not Available`, and is
optional elsewhere. A description absent where this rule requires it, or a
`Predecessor` description of another form, fails.

Fails: `origin_description_missing`.
Evidence: `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionOriginDescription`, `rust/crates/yamaa-core/src/submission_origin.rs::predecessor_descriptions_name_one_column_and_an_optional_condition`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::predecessor_columns_copy_the_column_their_description_names`.
Replaces: REQ-0892, REQ-0920.

Neither `Other` nor `Not Available` states anything alone. The dataset and the
column are identifiers joined by one period, and the condition after the
lowercase ` where ` names the records the predecessor column contributes, as
in `SUPPDM.QVAL where QNAM = 'RACEOTH'`. No rule parses the condition; a
generated document carries the whole description, and [SUB-20](#sub-20)
requires the copy to declare a `filter` when a condition is written. A
description absent where this rule requires it fails with
`origin_description_missing`, reporting the column; a `Predecessor`
description of another form fails the same way at `origin.description`.

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
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionOriginDocument`, `rust/crates/yamaa-adapters/tests/file_resources.rs::collected_origins_name_their_form_page_under_submission`, `python/tests/installed/installed_public_define.py::test_defaults_selection_bound_lists_and_required_acrf_aliases`.
Replaces: REQ-0893, REQ-0894, REQ-0921.

A value collected from a person is traceable to the form it was collected on.
The annotated case report form is the only document the reference can name,
and every declared reference names it. A derived reference carries no page, so
it is admitted only under `Other`; a regulatory submission states the page a
reviewer opens, and [DEFINE-4](define-xml.md#define-4) lists the requirement.
Under `Submission`, omitted `documents`, or declared references none of which
carries `pages`, fail with `origin_document_missing`, reporting the column.

<a id="sub-18"></a>

### SUB-18. A Derived origin declares its method

A column whose `origin.type` is `Derived` must declare `method`. A `Derived`
origin with no `method` fails.

Fails: `method_missing`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::metadata_admission_has_exact_causes_before_study_access`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionMethodMissing`.
Replaces: REQ-0895, REQ-0923.

The algorithm is what a derived origin claims exists; a claim with no
algorithm attached is not traceable.

## What the derivation refutes

<a id="sub-19"></a>

### SUB-19. A declared origin the column's derivation contradicts is rejected, and an origin is never inferred

A declared origin that the [Column derivation](../columns.md) graph
contradicts is rejected, and an origin is never inferred. A bare read refutes
no type: a `source`, an `odm` read or an `aggregate` whose `expr` is exactly
`ONLY` of one variable, read directly or through a plain lookup. A bare
`literal` refutes `Derived` and `Predecessor`, a recode of collected values
`Protocol` and `Predecessor`, and any other derivation `Collected`, `Protocol`
and `Predecessor`. A row-derived column keeps only the types all its `rows`
entries admit, and no shared type fails.

Fails: `origin_contradicts_derivation`.
Evidence: `sdtm-dm-metadata`, `sdtm-lb-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::independent_metadata_refutations_preserve_shared_conditions_and_zero_writes`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::recodes_literals_and_stored_values_admit_their_published_origins`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::computed_values_and_case_reads_of_derived_columns_refute_collected`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::pivots_and_plain_lookups_are_bare_reads_that_admit_collected`, `rust/crates/yamaa-core/src/submission_origin.rs::every_derivation_row_refutes_exactly_its_published_types`, `rust/crates/yamaa-core/src/submission_origin.rs::only_the_reduction_of_one_variable_to_its_one_value_is_a_pivot`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionOriginRefuted`.
Replaces: REQ-0896, REQ-0897, REQ-0898, REQ-0899, REQ-0900, REQ-0922.

| Derivation | Admitted `origin.type` | Refuted |
|---|---|---|
| a bare read: a `source`, an `odm` read or an `ONLY` pivot, directly or through a plain lookup | every type | none |
| a bare `literal` | `Assigned`, `Protocol`, `Collected`, `Other`, `Not Available` | `Derived`, `Predecessor` |
| a recode: `mapping`, `case`, `first_available`, `str_case`, `str_pad`, `str_replace`, `str_extract`, `str_template`, `to_date` or `dtc`, reading only as the paragraphs below state | `Collected`, `Derived`, `Assigned`, `Other`, `Not Available` | `Protocol`, `Predecessor` |
| anything else | `Derived`, `Assigned`, `Other`, `Not Available` | `Collected`, `Protocol`, `Predecessor` |

An `ONLY` pivot is an `aggregate` whose `expr` is exactly `ONLY(<variable>)`,
with no arithmetic around or inside the reduction, and which declares no
`derive` binding. It turns long records, such as one ODM record per collected
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

Anything else computes its value: arithmetic and rounding (`compute`,
`round_half_away_from_zero`), every aggregation but an `ONLY` pivot, windows,
temporal computation (the date and datetime operations other than `to_date`
and `dtc`), `flag`, `cut`, `greatest` and `least`, `function`, and every read
through a named intermediate that is not a plain lookup: one that declares
`derivations`, whose fields it computes, or one over `SELF`, the completed
output rows. Each type it refutes asserts the value arrived as it stands, and
the specification shows it did not.

The graph cannot tell an investigator-recorded value from a vendor-transmitted
or protocol-fixed value, and it refutes only what the derivation proves. An
SDTM recode of collected terminology (`SEX` from `Male` to `M` through
`mapping`), a case normalization (`str_case`) and an ISO 8601 assembly of
collected date parts (`dtc`, `str_template` or `to_date`) admit `Collected`. A value
printed on the case report form, such as a unit or a test name, is written as
a bare `literal` and admits `Collected`; a bare `literal` reads no input, so it
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
which only a bare `source` of the predecessor's input dataset makes.

When the entries of a row-derived column disagree, no shared type is an error,
not a silently admitted origin. A row template every row of which a value-level
entry declaring its own origin selects answers to that entry instead
([SUB-13](#sub-13)). A refuted origin fails with
`origin_contradicts_derivation`, reporting the column, the declared type and
the derivation that refutes it.

<a id="sub-20"></a>

### SUB-20. A Predecessor column is a bare `source` copy of the column its description names

A column whose `origin.type` is `Predecessor` copies its values unchanged from
the predecessor dataset and column `origin.description` names, so its
derivation must be a bare `source` of that dataset and column, declaring a
`filter` when the description writes a where clause. When the column's name
equals the predecessor column's name and a specification some submission
section of the project file lists declares that column, their labels must
be equal; a renamed column keeps its own label.

Fails: `origin_contradicts_derivation`, `refuted_value_origin`, `predecessor_label_mismatch`.
Evidence: `negative-mandatory-arm-missing`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::predecessor_columns_copy_the_column_their_description_names`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::pivots_and_plain_lookups_are_bare_reads_that_admit_collected`, `rust/crates/yamaa-adapters/tests/file_resources.rs::predecessor_labels_and_the_dataset_inventory_span_every_section`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionPredecessorLabel`.
Replaces: REQ-1157.

The `source` reads the predecessor through the input dataset declared under the
predecessor's name: `origin.description: DM.AGE` requires `source: DM.AGE`, and
`SUPPDM.QVAL where QNAM = 'RACEOTH'` requires a structured `source` of
`SUPPDM.QVAL` that declares a `filter`. No rule compares the filter with the
written condition. Any other derivation computes rather than copies, and the
refutation of [SUB-19](#sub-19) already rejects a computed column that claims
otherwise. An `ONLY` pivot and a read through a plain lookup are bare reads
under [SUB-19](#sub-19), but neither is a `source` of the predecessor's input
dataset. A bare `source` of another dataset or column, such a pivot or lookup
read, or a `source` without the `filter` a written condition requires, fails
with `origin_contradicts_derivation`, reporting the column, the predecessor and
the reason. A value-level `Predecessor` entry is checked against the
derivations that can produce the rows it selects and fails with
`refuted_value_origin`, as [SUB-13](#sub-13) states.

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
Replaces: REQ-0901, REQ-0902, REQ-0903, REQ-0904, REQ-1133, REQ-1135, REQ-1139, REQ-1144.

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

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/schema_submission.rs::complete_nested_metadata_retains_integer_text_documents_pages_and_order_after_drop`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionUnknownDocument`.
Replaces: REQ-0905, REQ-0906, REQ-1134, REQ-1136, REQ-1137, REQ-1138, REQ-1140.

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

### SUB-23. Rules needing no family or project file, and the `mandatory` resolution, are checked on the specification alone, and the rest when a document composes it

Every rule of this profile that needs neither a family nor the project file is
checked when the specification is validated on its own, and so is the
resolution and enforcement of `mandatory`, which reads its family group from
the dataset's `class`. Every other rule is checked when
[Define-XML](define-xml.md) composes the specification into a document.

Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::metadata_admission_has_exact_causes_before_study_access`, `rust/crates/yamaa-adapters/tests/file_resources.rs::original_lb_source_rejection_and_per_value_published_vocabulary_have_zero_writes`.
Replaces: REQ-0907, REQ-0908.

Checked when the specification is validated on its own:

- the vocabularies;
- the required and prohibited field combinations;
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
  family group, a key column's declared `mandatory: false`, and the presence
  check and redundant `not_missing` of [SUB-10](#sub-10) and
  [SUB-11](#sub-11).

Checked when a document composes the specification:

- the core designations and `core`/`mandatory` conflicts of
  [SUB-10](#sub-10), and the agreement of the class's group with the family;
- the one data type and display format a family gives a `date` or `datetime`
  ([SUB-6](#sub-6), [SUB-25](#sub-25));
- the remaining family-dependent rules, such as the origin pairs of
  [SUB-15](#sub-15);
- the annotated case report form references of [SUB-17](#sub-17), which name
  the section's documents and depend on its `context`, and the predecessor
  labels of [SUB-20](#sub-20), which read every section's specifications.

The row checks of [SUB-13](#sub-13) run in every build of the specification,
before publication. The transport lengths of [SUB-24](#sub-24) are measured
after composition, from the published artifacts, when a transport is
requested.

Only the study document says which standard the identifier names. A
specification that is never composed carries declarations no family has
judged; it is not yet part of a submission.

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

Evidence: `sdtm-dm-metadata`, `sdtm-dm-xpt`, `adam-adsl-xpt`, `sdtm-suppdm-metadata`, `rust/crates/yamaa-adapters/tests/submission_transport.rs::shared_column_lengths_span_datasets_and_supplemental_datasets_stand_alone`.

A character column is a `str` column, and a `date` outside `adam`, which
travels as its canonical text ([XPT-6](xpt.md#xpt-6)); only a `text`
column or value-level definition carries a `Length`. A SUPP-- dataset is one
whose name begins with `SUPP`: its lengths come from its own values alone and
do not lengthen another dataset's column. A dataset declared to have no data
contributes no value, and a missing value occupies no bytes. An `int` or
`float` column keeps its declared `length`. Without a transport no artifact is
read, and the declared `length` is the document's `Length`.

<a id="sub-25"></a>

### SUB-25. Under `adam` a date or datetime is submitted as a SAS numeric date with a fixed display format and length

Under the `adam` family, a `date` column is submitted as SAS days since
1960-01-01 with display format `DATE9.`, and a `datetime` column as seconds
since 1960-01-01T00:00:00 with `DATETIME20.`. The document describes each as
`DataType` `integer`, `Length` 8 and that `def:DisplayFormat`. A declared
`data_type` other than `integer`, or a declared `display_format` other than that
format, fails when a document composes the specification. Under `sdtm` and
`send` a `date` keeps its own type and canonical text, and
[SUB-27](#sub-27) refuses a `datetime`.

Fails: `submission_data_type_not_admitted`, `value_not_permitted`.
Evidence: `adam-adsl-xpt`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionDisplayFormatConflict`.

Dataset-JSON keeps the ISO value and asks for the integer with
`targetDataType` ([JSON-10](dataset-json.md#json-10)), and a SAS Transport file
writes the number ([XPT-6](xpt.md#xpt-6)). A conflicting `display_format` fails
with `value_not_permitted` at the column's `display_format`; a conflicting
`data_type` fails as [SUB-6](#sub-6) states. No `length` is declared on such a
column ([SUB-7](#sub-7)).

<a id="sub-27"></a>

### SUB-27. An `sdtm` or `send` dataset carries its dates and times as text, never in a `datetime` column

A dataset whose `submission.class` is a tabulation class of
[SUB-10](#sub-10), so of the `sdtm` or `send` family, must not carry a
`datetime` column in `output.columns`: such a column writes the seconds that
the canonical text of [VALUE-17](../values.md#value-17) always shows, collected
or not. Its dates and times are `str` columns with a date or time `data_type`
([SUB-26](#sub-26)). A `datetime` output column fails validation at its
`type`; an internal one is not refused.

Fails: `value_not_permitted`.
Evidence: `negative-sdtm-datetime-column`, `sdtm-vs-epoch-from-subject-elements`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::tabulation_datasets_refuse_a_datetime_output_column`, `rust/crates/yamaa-adapters/tests/file_resources.rs::an_sdtm_document_refuses_a_datetime_output_column`.

The family is read from the dataset's class, as the resolution of `mandatory`
reads it, so the refusal needs no study document; a dataset that declares no
class is not checked, and a document composing an `sdtm` or `send` dataset
refuses such a column the same way. The failure reports `value_not_permitted`
at `columns.<column>.type` with the column, the value `datetime`, the class and
the permitted column types `str` and `date`, or at
`<family>.specs[<n>].columns.<column>.type` when a document composes the
dataset. A `date` column is admitted: its
canonical text carries the day alone. An internal `datetime`, such as the
moment an element lookup compares, never reaches the artifact. Text collected
to the minute stays `2025-03-04T08:30` in a `str` column, where a `datetime`
column would write `2025-03-04T08:30:00`.
