# Submission metadata

This file is a profile: it is optional, and it applies to a specification that
declares `submission` metadata and to the study environment section that
composes such specifications into a [Define-XML](define-xml.md) document and
[Dataset-JSON](dataset-json.md) files. It governs the dataset and column
metadata a submission document is generated from: what each field means,
which combinations a standard admits, and what the derivation refutes. A
specification that declares no submission metadata is unaffected.

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

A column's `codelist` names a codelist the study document declares.
[Controlled terminology](terminology.md) owns that object, the values a binding
enforces, and its agreement with an `allowed_values` verification
([CT-1](terminology.md#ct-1)).

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

### SUB-6. A column's submission data type defaults from its column type, within the closed set that type admits

`data_type` states the type the submission represents the column as. It
defaults from the declared column type, and each declared type admits the
closed set the table below fixes. `data_type` never changes a value: it states
how a value [Types and conversion](../values/types.md) already typed appears in
the document, and no derivation, verification, key, order term or artifact
byte can observe it. A `data_type` outside the admitted set fails validation.

Fails: `submission_data_type_not_admitted`.
Evidence: `sdtm-vs-epoch-from-subject-elements`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionDataType`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::submission_fields_do_not_change_any_original_domain_artifact`.
Replaces: REQ-0868, REQ-0869, REQ-0870, REQ-0914, REQ-1145.

| Declared type | Default | Also admits |
|---|---|---|
| `str` | `text` | the types listed below |
| `int` | `integer` | none |
| `float` | `float` | none |
| `date` | `date` | none |
| `datetime` | `datetime` | none |

`str` admits `date`, `datetime`, `time`, `partialDate`, `partialTime`,
`partialDatetime`, `incompleteDate`, `incompleteTime`, `incompleteDatetime`,
`durationDatetime`, `intervalDatetime` and `URI`.

A `str` column admits temporal submission types: a submission carries a
partial or incomplete date as text, and [Temporal values](../values/temporal.md)
does not admit partial or incomplete dates as `date` or `datetime`. The two
temporal column types admit only their matching submission type:
[Temporal values](../values/temporal.md) defines each temporal type as a
complete value, and the submission types of the same name mean the same thing.
A number's submission type is fixed: [Types and conversion](../values/types.md)
already decides integer or binary64. The failure reports the column, the
declared type and the submission type.

<a id="sub-7"></a>

### SUB-7. `length` and `significant_digits` are declared exactly where the submission data type needs them

`length` is the maximum expected value length, as the submission standard
defines the term: a property of the column declaration, not of the data one run
produced, and a positive integer. It is required when `data_type` resolves to
`text`, `integer` or `float`, unless [SUB-8](#sub-8) derives it, and must not
be declared otherwise. `significant_digits` counts digits after the decimal
point, is a non-negative integer, and is required when `data_type` resolves to
`float` and must not be declared otherwise.

Fails: `submission_length_missing`, `submission_length_not_applicable`, `submission_length_invalid`.
Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionLengthMissing`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::effective_value_metadata_rejects_invalid_bounds_types_and_explicit_length_conflicts_before_data`.
Replaces: REQ-0871, REQ-0872, REQ-0873, REQ-0911, REQ-0912.

The other submission types carry fixed-form values, so a length for such a type
restates that form. A `length` that is not a positive integer, or a negative
`significant_digits`, fails validation with `submission_length_invalid`. A
`length` or `significant_digits` absent where this rule requires it fails with
`submission_length_missing`, and one declared where this rule prohibits it
fails with `submission_length_not_applicable`, reporting the column.

<a id="sub-8"></a>

### SUB-8. A `str` column's length is a bound its completed values must meet, stated once

A `length` on a `str` column is enforced: the column's completed values must
contain at most `length` [Text values](../values/text.md) scalar values, as
[Verification](../execution/verification.md)'s `max_length` requires. On a
`str` column with a `max_length` verification, `length` is derived from the
verification's `max` and need not be declared; declaring both is accepted when
equal and rejected when different. A `length` declared on any other column type
is carried into the document unchecked.

Fails: `declared_length_conflict`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::declared_length_is_checked_once_over_unicode_scalar_values`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::metadata_admission_has_exact_causes_before_study_access`.
Replaces: REQ-0874, REQ-0875, REQ-0876, REQ-0913.

The declared length binds rather than describes. `length` and `max_length`
state one bound, and the bound is checked once either way. A `length` and a
`max_length` verification declaring different bounds on the same column fail
validation with `declared_length_conflict`, reporting the column and both
bounds. A `str` column with neither declaration, where [SUB-7](#sub-7) requires
a length, fails as [SUB-7](#sub-7) states.

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

A study that wants different artifact digits declares `output.decimals`, which
the [CSV profile](../reference/csv.md) owns. The two settings are independent,
and this profile does not reconcile them: one is a display a document reports
and the other is a display an artifact carries. Each implementation guide
publishes and versions its own role vocabulary. The `adam` family defines no
role vocabulary, so a role in that family would name nothing.

<a id="sub-10"></a>

### SUB-10. `core` uses its family's designations, and `mandatory` follows its family's rule

`core` is the standard's core designation and is declared for every family:
`Req`, `Exp` or `Perm` for `sdtm` and `send`, and `Req`, `Cond` or `Perm` for
`adam`. `mandatory` states whether the completed column must carry a value and
is not a second spelling of `core`. For `sdtm` and `send`, `mandatory`
defaults from `core`, and a declared `mandatory: false` on a `Req` column is
rejected. For `adam`, `core` and `mandatory` are both required and neither
derives the other.

Fails: `core_mandatory_conflict`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionCoreMandatory`, `rust/crates/yamaa-core/tests/schema_submission.rs::all_core_origin_source_and_method_choices_are_retained`.
Replaces: REQ-0880, REQ-0881, REQ-0882, REQ-0883, REQ-0884, REQ-0915, REQ-1141.

For `sdtm` and `send`, `Req` is required, `Exp` expected and `Perm`
permissible. For `adam`, `Req` is required, `Cond` conditionally required and
`Perm` permissible, as the ADaM implementation guides publish them. An `Exp`
for `adam`, or a `Cond` for `sdtm` or `send`, names no designation its standard
publishes.

The mapping between `core` and `mandatory` is standard-specific. For `sdtm` and
`send`, `core` is required and `mandatory` defaults from it: `Req` gives true,
and `Exp` and `Perm` give false. A declared `mandatory: true` on an `Exp` or
`Perm` column is an accepted sponsor restriction. A declared
`mandatory: false` on a `Req` column is rejected: the standard requires the
column, and a specification cannot relax its standard by declaration.

For `adam`, the ADaM guides state that `Core` assists preparation, is not
submitted in define.xml, and allows nulls unless otherwise specified. A `Req`
core requires the variable's inclusion in the dataset but allows nulls;
`mandatory` states whether every row carries a non-missing value, as the
programming specification decides. A `Cond` core therefore tells nothing about
`mandatory`, and a `mandatory: false` on a `Req` column is accepted.

Each of these fails with `core_mandatory_conflict`, reporting the column:

- a `core` absent for any family;
- an `Exp` for `adam`;
- a `Cond` for `sdtm` or `send`;
- a `mandatory: false` on a `Req` column for `sdtm` or `send`;
- a `mandatory` absent for `adam`.

<a id="sub-11"></a>

### SUB-11. A mandatory column is enforced by a `not_missing` verification or by `keys`

A column declaring `mandatory: true` must also carry a `not_missing`
verification or appear in `keys`, whose non-missing requirement
[Artifact publication](../storage/publication.md) already states. A
`mandatory: true` column with neither fails.

Fails: `mandatory_not_enforced`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionMandatoryUnenforced`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::effective_value_metadata_rejects_invalid_bounds_types_and_explicit_length_conflicts_before_data`.
Replaces: REQ-0885, REQ-0916.

## Value-level metadata

<a id="sub-12"></a>

### SUB-12. A row template's `submission` map declares metadata for one literal test code of its domain

A `rows` entry may carry a `submission` map keyed by column name. Each entry
declares the submission metadata for one value of the row's discriminator, the
domain's `<DOMAIN>TESTCD` column, and fields it leaves absent inherit from the
column-level `submission`. The domain must declare `<DOMAIN>TESTCD`, and the
row template must derive it as `{literal: <code>}` with a string code. Each key
must name a declared column in `output.columns` that the row template or the
column declaration derives.

Fails: `value_metadata_requires_testcd`, `value_metadata_requires_literal_testcd`, `value_metadata_unknown_column`, `value_metadata_not_output_column`.
Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-core/tests/schema_submission.rs::each_independent_value_admission_control_has_the_required_condition_and_row_path`, `rust/crates/yamaa-core/tests/schema_submission.rs::overrides_inherit_complete_metadata_and_preserve_written_occurrences`.
Replaces: REQ-1162, REQ-1163, REQ-1164, REQ-1165, REQ-1166.

The column-level `submission` stays the shared declaration for the column, and
the row-level entry supplies per-value overrides. A findings domain can thus
give each `--TESTCD` its own codelist, origin or length without repeating the
shared metadata.

A row template carrying `submission` without a declared `<DOMAIN>TESTCD`
column has no discriminator to hang values on, and the declaration fails with
`value_metadata_requires_testcd`. A dynamic expression leaves the value unknown
until data arrives, and value-level metadata is validated without data
([SUB-23](#sub-23)), so a dynamic discriminator fails loudly, with
`value_metadata_requires_literal_testcd`, rather than silently attaching
metadata to an unknown value. A key naming anything other than a declared
column the row template or the column declaration derives fails with
`value_metadata_unknown_column`; it is not an ignored declaration.
[SUB-1](#sub-1) applies per value: metadata for an internal column fails with
`value_metadata_not_output_column` at row level exactly as it does at column
level.

<a id="sub-13"></a>

### SUB-13. Value-level declarations for one column and test code agree, and each is refuted against its own derivation

Two row templates may declare submission metadata for the same column and test
code only with identical declarations: identical declarations describe one
value, and conflicting declarations fail rather than merge. The refutations of
computed, literal and copied values in [SUB-19](#sub-19) apply to each
value-level entry against the derivation that produces the column's value in
that row template.

Fails: `conflicting_value_metadata`, `refuted_value_origin`.
Evidence: `rust/crates/yamaa-core/tests/schema_submission.rs::identical_declarations_at_distinct_nodes_share_one_value_definition`, `rust/crates/yamaa-core/tests/schema_submission.rs::each_independent_value_admission_control_has_the_required_condition_and_row_path`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionValueOriginRefuted`.
Replaces: REQ-1167, REQ-1168.

That derivation is the row template's derivation, or the column-level
derivation when the row template inherits a uniform one. The refutation follows
the entry, and the origin claim is per value. Conflicting declarations fail
with `conflicting_value_metadata`, not a last-wins merge; a value-level origin
the derivation refutes fails with `refuted_value_origin`.

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
`sdtm`, the table below fixes them. For `adam`, the admitted types are
`Derived`, `Assigned`, `Predecessor` and `Other`, and `source` is derived
rather than declared: `Sponsor` for `Derived`, `Assigned` and `Other`, and
absent for `Predecessor`. For `send`, every type is admitted and `source` must
be absent. A pair the family does not admit fails.

Fails: `origin_source_not_admitted`.
Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::original_lb_source_rejection_and_per_value_published_vocabulary_have_zero_writes`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionOriginSource`.
Replaces: REQ-0889, REQ-0890, REQ-0891, REQ-0919.

| Type | Admitted sources for `sdtm` |
|---|---|
| `Collected` | `Subject`, `Investigator`, `Vendor` |
| `Derived` | `Vendor`, `Sponsor` |
| `Assigned` | `Vendor`, `Sponsor` |
| `Protocol` | `Sponsor` |
| `Other` | any |
| `Predecessor` | none; `source` must be absent |
| `Not Available` | not admitted |

For `adam`, a declared `source` is rejected, and `Collected` and `Protocol` are
not admitted. The `send` family identifies responsibility through its own
trial-summary and findings columns rather than through this attribute. An
origin type and source pair the family does not admit, a `source` present where
these rules require absence, or one absent where they require presence fails
with `origin_source_not_admitted`.

<a id="sub-16"></a>

### SUB-16. A Predecessor, Other or Not Available origin carries a description

`origin.description` is required when `type` is `Predecessor`, and names the
dataset and column the value was copied from in the form
`<dataset>.<column>`. The description is also required when `type` is `Other`
or `Not Available`, and is optional elsewhere. A description absent where this
rule requires it fails.

Fails: `origin_description_missing`.
Evidence: `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionOriginDescription`.
Replaces: REQ-0892, REQ-0920.

Neither `Other` nor `Not Available` states anything alone.

<a id="sub-17"></a>

### SUB-17. A value collected from a person references the annotated case report form

A column with a `Collected` origin from `Investigator` or `Subject` references
the study's annotated case report form, and [Define-XML](define-xml.md)
rejects a reference naming any other document. The reference is derived when
`documents` is omitted; a column declares `documents` to say where in the form
the value was collected. Such an origin with no document reference fails.

Fails: `origin_document_missing`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionOriginDocument`, `python/tests/installed/installed_public_define.py::test_defaults_selection_bound_lists_and_required_acrf_aliases`.
Replaces: REQ-0893, REQ-0894, REQ-0921.

A value collected from a person is traceable to the form it was collected on.
The annotated case report form is the only document the reference can name.
Omitting `documents` leaves the reference without a page.

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

The derivation graph of the [Execution lifecycle](../execution/lifecycle.md)
proves some facts about a column value, and a declared origin that contradicts
a proven fact is rejected. The graph never supplies an origin: origin is always
declared and sometimes refuted, never inferred. The table below fixes which
types each derivation admits. For a row-derived column, each `rows` entry's
derivation is checked, and the column keeps only the types all entries admit;
no shared type fails.

Fails: `origin_contradicts_derivation`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::independent_metadata_refutations_preserve_shared_conditions_and_zero_writes`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionOriginRefuted`.
Replaces: REQ-0896, REQ-0897, REQ-0898, REQ-0899, REQ-0900, REQ-0922.

| Derivation | Admitted `origin.type` | Refuted |
|---|---|---|
| anything other than a bare `source`, a bare `odm` or a bare `literal` | `Derived`, `Assigned`, `Other` | `Collected`, `Protocol`, `Predecessor`, `Not Available` |
| a bare `literal` | `Assigned`, `Protocol`, `Other` | `Collected`, `Derived`, `Predecessor`, `Not Available` |
| a bare `source` or a bare `odm` | every type except `Derived` | `Derived` |

The graph cannot tell an investigator-recorded value from a vendor-transmitted
or protocol-fixed value. A column whose derivation is anything other than a
bare `source`, a bare `odm` or a bare `literal` computes its value from other
values: each refuted type asserts the value arrived as it stands, and the
specification shows it did not. A column derived by a bare `literal` takes the
same value in every row regardless of any input: nothing was observed, computed
or copied, and a value the specification states outright is available.

A column derived by a bare `source` copies one value of one declared dataset,
and a column derived by a bare `odm` copies the `Value` of the one ODM record it
identifies ([REQ-1272](../specification/binding.md#req-1272)): nothing was
computed. Whether that stored value was collected, assigned, fixed by the
protocol or copied from a predecessor dataset is a fact about the source and
not about this specification.

When the entries of a row-derived column disagree, no shared type is an error,
not a silently admitted origin. A refuted origin fails with
`origin_contradicts_derivation`, reporting the column, the declared type and
the derivation that refutes it.

<a id="sub-20"></a>

### SUB-20. A Predecessor column is a bare `source` copy of the column its description names

A column whose `origin.type` is `Predecessor` copies its values unchanged from
the predecessor dataset and column `origin.description` names, so its
derivation must be a bare `source` of that dataset and column. When both the
column and the named predecessor column declare submission metadata, their
declared labels must be equal.

Replaces: REQ-1157.

Any other derivation computes rather than copies, and the refutation of
[SUB-19](#sub-19) already rejects a computed column that claims otherwise. A
copy keeps the name, the label and the values of what it copies.

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

## When the profile is checked

<a id="sub-23"></a>

### SUB-23. Family-independent rules are checked on the specification alone, and family-dependent rules when a document composes it

Every rule of this profile that does not depend on a family is checked when the
specification is validated on its own. Every family-dependent rule is checked
when [Define-XML](define-xml.md) composes the specification into a document.

Evidence: `sdtm-lb-metadata`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::metadata_admission_has_exact_causes_before_study_access`, `rust/crates/yamaa-adapters/tests/file_resources.rs::original_lb_source_rejection_and_per_value_published_vocabulary_have_zero_writes`.
Replaces: REQ-0907, REQ-0908.

Checked when the specification is validated on its own:

- the vocabularies;
- the required and prohibited field combinations;
- the length binding of [SUB-8](#sub-8);
- the graph refutations of [SUB-19](#sub-19);
- the value-level declaration rules of [SUB-12](#sub-12) and
  [SUB-13](#sub-13);
- the presence of `method` for a declared `Derived` origin
  ([SUB-18](#sub-18)).

Only the study document says which standard the identifier names. A
specification that is never composed carries declarations no family has
judged; it is not yet part of a submission.
