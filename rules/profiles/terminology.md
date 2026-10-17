# Controlled terminology

This file is a profile: it is optional, and it applies to a build whose study
environment declares `codelists` and to the specifications that bind them
through `submission.codelist`. A codelist is declared once in the environment,
every binding names it, and a binding enforces the items its list declares. A
build whose environment declares no codelists is unaffected.

## Declaring codelists

<a id="ct-1"></a>

### CT-1. A codelist is declared once in an environment codelist source, and a column binds it by `id`

A domain-build codelist is declared once in an environment codelist source and
carries an `id` that is unique across all captured sources. A column names that
`id` through `submission.codelist`, and columns share one codelist statement.
`name` is the codelist's human-readable name and is unique across all
environment codelist sources. A duplicate codelist `id` or `name`, or a
binding naming no declared codelist, fails validation.

Fails: `duplicate_define_identifier`, `unknown_codelist`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/project_execution.rs::global_duplicates_across_sources_have_no_override`, `rust/crates/yamaa-core/tests/project_execution.rs::compiler_projects_unknown_type_and_redundant_sets_as_static_preflight_causes`.
Replaces: REQ-0926, REQ-0927, REQ-0948, REQ-0953, REQ-1077.

`id` and `name` are separate: `id` is what a specification writes, and `name`
is what a reader sees. [Submission metadata](submission.md)'s `codelist` names
a codelist the study document declares; this profile owns that object, the
values a binding enforces, and the redundancy of an `allowed_values`
verification beside it. A duplicate `id` or `name` fails with
`duplicate_define_identifier`. A binding naming no declared codelist fails with
`unknown_codelist`, reporting the column and the identifier.

<a id="ct-2"></a>

### CT-2. A codelist source's `standard` names its published terminology, and a source without one is sponsor-defined

`standard` belongs to a codelist source and states its terminology name,
publishing set (`ADaM`, `CDASH`, `DEFINE-XML`, `SDTM` or `SEND`) and version
once for every list in that source. A source that omits `standard` is
sponsor-defined rather than unknown; exactly one of published or
sponsor-defined holds for any codelist. A codelist's `alias` carries the
published concept identifier of the codelist itself, and an item's `alias` the
published identifier of that value.

Evidence: `sdtm-dm-metadata`, `sdtm-ae-coding`, `sdtm-vs-epoch-from-subject-elements`.
Replaces: REQ-0928, REQ-0930, REQ-0958.

Both aliases are optional here; [DEFINE-4](define-xml.md#define-4) states when a
document requires an alias. A `standard` naming a declared standard whose type
is not `CT` fails validation.

## Values

<a id="ct-3"></a>

### CT-3. A codelist lists its items or defers to an external dictionary, never both and never neither

A codelist declares either `items` or `external`, and never both and never
neither. An `external` codelist names the `dictionary` and its `version`, with
an optional `href`: large or volatile dictionaries are referenced, and the
version is required. A codelist declaring both `items` and `external`, or
neither, fails validation.

Fails: `codelist_shape_invalid`.
Evidence: `sdtm-dm-metadata`, `sdtm-ae-coding`, `rust/crates/yamaa-core/tests/project_execution.rs::invalid_shapes_and_missing_boolean_fractional_item_types_are_rejected`.
Replaces: REQ-0931, REQ-0936, REQ-0947, REQ-1079.

A reference without a version identifies nothing to check. The failure reports
the codelist.

<a id="ct-4"></a>

### CT-4. Coded values are unique within a codelist, of its data type, and keep their declared order

`data_type` is `text`, `integer` or `float` and defaults to `text`, and every
coded value must be of the declared `data_type`. Coded values are unique within
a codelist under [Values](../values.md) equality for `text` and under
numeric equality otherwise. Items keep their declared order, and
[Define-XML](define-xml.md) carries the declared order into the generated
document. A duplicate coded value, or one not of the codelist's `data_type`,
fails validation.

Fails: `codelist_duplicate_value`, `codelist_shape_invalid`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/project_execution.rs::numeric_equality_is_exact_at_i64_and_binary64_boundaries`, `rust/crates/yamaa-adapters/tests/project_environment.rs::complete_static_report_keeps_semantics_alongside_original_opaque_capture_and_authored_holes`.
Replaces: REQ-0929, REQ-0935, REQ-0949, REQ-0950.

A duplicate coded value within one codelist fails with
`codelist_duplicate_value`. A coded value that is not of the codelist's
`data_type` fails with `codelist_shape_invalid`.

<a id="ct-5"></a>

### CT-5. `decode` and `rank` are declared for every item of a codelist or for none

Each item declares `value`, the coded value itself. `decode` is the text the
value stands for and is optional, and `rank` states an item's ordering
significance relative to the other items. Declaring `decode` for some items of
one codelist and not others fails validation, and so does declaring `rank` for
some items and not others.

Fails: `codelist_partial_item_field`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/project_execution.rs::complete_independent_item_findings_survive_invalid_shape`, `rust/crates/yamaa-core/tests/support/project_environment.rs`.
Replaces: REQ-0932, REQ-0933, REQ-0951, REQ-1078.

A list whose values are already the words a reader needs has no decode to add.
Lists with and without `decode` are different kinds of list rather than a list
with gaps.

<a id="ct-6"></a>

### CT-6. Only an extensible published codelist may carry sponsor-extended items

`extended` marks an item the sponsor added to a published list. It defaults to
false and may be true only on an `extensible` codelist whose source declares
`standard`. An `extended: true` item on a sponsor-defined or non-extensible
codelist fails validation.

Fails: `codelist_extension_not_admitted`.
Evidence: `rust/crates/yamaa-core/tests/project_execution.rs::extensions_require_both_a_published_source_and_an_extensible_list`, `rust/crates/yamaa-core/tests/project_execution.rs::complete_independent_item_findings_survive_invalid_shape`, `rust/crates/yamaa-core/tests/support/project_environment.rs`.
Replaces: REQ-0934, REQ-0952.

Adding a value to a list that admits no additions is a contradiction rather
than an extension. A sponsor addition is an item like any other, so
[CT-7](#ct-7) enforces it with the published items, and
[DEFINE-20](define-xml.md#define-20) marks it `def:ExtendedValue="Yes"`.

## Bindings

<a id="ct-7"></a>

### CT-7. A bound codelist that lists items is the column's value set, and an external dictionary enforces nothing

Binding a codelist that declares `items`, extensible or not, requires every
non-missing value of the column to equal one declared `value`, and a sponsor
addition to an extensible list is declared as an item with `extended: true`.
Missing values pass. The check runs where [Verification](../verification.md)
runs a column verification, over the completed column, and fails the same way.
A value-level binding decides the rows of its test code. Binding an `external`
codelist enforces nothing.

Fails: `allowed_values_failed`.
Evidence: `negative-epoch-outside-extensible-list`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::per_value_codelists_inherit_origins_and_check_completed_discriminators`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::per_value_extensible_lists_are_enforced_on_their_rows_and_dictionaries_replace_the_default`, `rust/crates/yamaa-core/tests/project_execution.rs::owned_checked_environment_executes_fixed_codelists_before_later_cells_in_both_plan_forms`.
Replaces: REQ-0937, REQ-0938, REQ-0939, REQ-0940, REQ-0957.

Missing values pass exactly as [Verification](../verification.md)'s
`allowed_values` treats them; a column that also prohibits absence resolves
`mandatory` to true ([SUB-11](submission.md#sub-11)). The enforcement is the
same check `allowed_values` performs, arriving from the terminology rather than
from a second list beside it ([CT-9](#ct-9)).

An extensible list's declared items, the sponsor's `extended: true` additions
included, are the whole value set a study admits: the published list's
openness is what lets the sponsor declare an addition, and an undeclared value
fails like any other. The admitted values of an `external` codelist live in the
dictionary, and this language does not read the dictionary.

A value-level binding ([SUB-12](submission.md#sub-12)) replaces the column's
binding on the rows whose completed `<DOMAIN>TESTCD` is its test code: those
rows are checked against the value-level list alone, or against nothing when
it is an `external` codelist, and every other row against the column's list.

A non-missing value outside a bound list fails where
[Verification](../verification.md) fails a column verification, reporting the
column, the codelist, the row's key and the value.

<a id="ct-8"></a>

### CT-8. A bound column's declared type admits its codelist's data type

A bound column's declared type must admit the codelist's `data_type`: a `text`
codelist binds to a `str` column, an `integer` codelist to an `int` column, and
a `float` codelist to a `float` column. A binding whose codelist `data_type` the
column's declared type does not admit fails validation.

Fails: `codelist_type_mismatch`.
Evidence: `rust/crates/yamaa-core/tests/project_execution.rs::compiler_projects_unknown_type_and_redundant_sets_as_static_preflight_causes`, `rust/crates/yamaa-core/tests/project_execution.rs::binding_checks_exact_column_type_and_rejects_restated_item_sets`, `rust/crates/yamaa-adapters/tests/file_resources.rs::integer_terminology_items_admit_equal_float_published_values`.
Replaces: REQ-0941, REQ-0954.

A coded value a column could never hold is a defect in the specification rather
than a constraint that never fires.

<a id="ct-9"></a>

### CT-9. An `allowed_values` verification beside a column's codelist of items is redundant

A column-level codelist binding that declares `items` already states the
column's value set ([CT-7](#ct-7)), so an `allowed_values` verification on the
same column restates it and fails validation, whether or not the two sets
agree. Beside an `external` codelist the verification stands alone, and
`allowed_values` without a `codelist` binding remains an ordinary verification.
A value-level binding governs only its own rows and leaves the column's
verification standing.

Fails: `redundant_declaration`.
Evidence: `negative-allowed-values-beside-codelist`, `rust/crates/yamaa-adapters/tests/file_resources.rs::independent_metadata_refutations_preserve_shared_conditions_and_zero_writes`, `rust/crates/yamaa-core/tests/project_execution.rs::extensible_catalogue_is_enforced_and_states_the_value_set_once`.
Replaces: REQ-0942, REQ-0944, REQ-0945, REQ-0955.

The failure reports the verification's path and, in `stated_by`, the binding's
path, with the column and the codelist. A binding's items are known where an
environment supplies the codelists: a domain build with an environment and a
composed document check the redundancy, and a build without one does not read
the binding. Not every constrained column carries published terminology.

## Documents and inventories

<a id="ct-10"></a>

### CT-10. A generated document carries exactly the codelists its output columns bind

Each generated document includes exactly the environment codelists bound by its
represented output columns, in their declaration order, and the
controlled-terminology standards cited by those sources. Unbound codelists may
serve another submission section and are omitted without an
unreferenced-codelist failure.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::unused_terminology_is_omitted_and_missing_document_links_do_not_read_study_files`, `python/tests/installed/installed_public_define.py::test_defaults_selection_bound_lists_and_required_acrf_aliases`.
Replaces: REQ-0946.

<a id="ct-11"></a>

### CT-11. An inventory-vocabulary column holds only names of datasets any submission section of the study environment declares

A column carrying submission metadata may declare `inventory_vocabulary`. Every
non-missing value of such a column must equal the name of a dataset a
specification some submission section of the study environment lists declares,
whichever section composes the column. Missing values pass, exactly as
[CT-7](#ct-7) treats missing values, and enforcement runs where
[Verification](../verification.md) runs a column verification, over the
completed column, and fails the same way.

Fails: `allowed_values_failed`.
Evidence: `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::inventory_uses_inherited_declared_dataset_names_and_retained_original_metadata_in_both_hosts`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::absent_study_inventory_checks_nonmissing_values_and_keeps_missing_values_admitted`, `rust/crates/yamaa-adapters/tests/project_domain_submission.rs::inventory_names_the_datasets_of_every_submission_section`, `rust/crates/yamaa-adapters/tests/file_resources.rs::predecessor_labels_and_the_dataset_inventory_span_every_section`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionInventoryFailed`.
Replaces: REQ-1156.

The admitted values are the study's dataset inventory, which no static list can
enumerate; that distinguishes this vocabulary from a codelist, whose values
[CT-3](#ct-3) requires be listed or deferred to a dictionary. The inventory is
the `domain` of every specification the `sdtm`, `adam` and `send` sections of
the study environment list, so an ADaM `SRCDOM` may name the SDTM `AE` domain
the `sdtm` section lists. A domain build reads it from the environment it is
given and admits no name without one; composition reads the other sections'
specifications for it, and Dataset-JSON's recheck of published values uses
the same inventory. A column that also prohibits absence resolves `mandatory`
to true ([SUB-11](submission.md#sub-11)). The ADaM `SRCDOM` column, whose
values name the source SDTM domain or ADaM dataset, binds this vocabulary.
