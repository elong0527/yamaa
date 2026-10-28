# Controlled terminology

This file is a profile: it is optional, and it applies to a build whose
project file declares `codelists` and to the specifications that bind them
through `submission.codelist`. A codelist is declared in a source the project
file lists, a later source may replace it by `id`, every binding names it, and
a binding enforces the items its list declares. A build whose project declares
no codelists is unaffected, except for a column that declares
`inventory_vocabulary` under [CT-11](#ct-11). A specification
checked or run without a project file that binds a codelist or declares an
inventory vocabulary fails under [CT-13](#ct-13).

## Declaring codelists

<a id="ct-1"></a>

### CT-1. A project's codelists compose by `id` across its sources, and a column binds one by `id`

A domain-build codelist is declared in a project codelist source and carries
an `id`. Sources compose in listed order: a later source's codelist replaces
every earlier codelist of its `id` whole, `name` and items included. A column
names a composed codelist's `id` through `submission.codelist`. `name` is the
human-readable name. An `id` repeated within one source, a `name` repeated
among the composed codelists, or a binding naming no codelist its project file
declares fails validation, as does any binding checked or run without a
project file ([CT-13](#ct-13)).

Fails: `duplicate_define_identifier`, `unknown_codelist`, `project_binding_missing`.
Evidence: `sdtm-dm-metadata`, `schema-codelist-replacement`, `negative-codelist-name-reused`, `rust/crates/yamaa-core/tests/project_execution.rs::a_later_source_replaces_an_earlier_codelist_of_its_id_whole`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/codelist_binding.json::a_binding_of_a_codelist_the_project_does_not_declare_is_unknown_when_prepared`.

`id` and `name` are separate: `id` is what a specification writes, and `name`
is what a reader sees. Columns that name one `id` share one codelist
statement. [Submission metadata](submission.md)'s `codelist` names a codelist
the project file declares; this profile owns that object, the values a
binding enforces, and the redundancy of an `allowed_values` verification
beside it. An `id` a source declares twice fails with
`duplicate_define_identifier` at the later `id`, and a `name` the composed
codelists repeat at the later `name`, both reporting the identifier. A binding
naming no declared codelist fails with `unknown_codelist`, reporting the
column and the identifier. A specification checked or run without a project
file declares no codelist, so each binding fails with
`project_binding_missing` instead ([CT-13](#ct-13)).

Replacement lets the packages of one study share a codelist source and declare
only the codelists a package changes: the package's project file lists the
shared source, then its own source declaring the replacing codelists. A
replacing codelist is its own declaration alone, and nothing of the codelist
it replaces is merged into it. The composed codelists are each source's
codelists that no later source replaces, in source and list order, so a
replacing codelist keeps its place in its own source, is drawn from that
source's `standard` ([CT-2](#ct-2)) and is listed there in a document
([CT-10](#ct-10)). A replaced codelist is still checked on its own, under
[CT-3](#ct-3) to [CT-6](#ct-6) and [CT-12](#ct-12). A replacing codelist may
keep or change the `name` of the codelist it replaces, while a later codelist
that takes a kept codelist's `name` under another `id` fails.

<a id="ct-2"></a>

### CT-2. A codelist source's `standard` names its published terminology, and a source without one is sponsor-defined

`standard` belongs to a codelist source and states its terminology name, one of
the standard names Define-XML publishes such as `CDISC/NCI`, its publishing set
(`ADaM`, `CDASH`, `DEFINE-XML`, `SDTM` or `SEND`) and its version once for
every list in that source. A source that omits `standard` is sponsor-defined
rather than unknown; exactly one of published or sponsor-defined holds for any
codelist. A codelist's `alias` carries the published concept identifier of the
codelist itself, and an item's `alias` the published identifier of that value.

Evidence: `sdtm-dm-metadata`, `sdtm-ae-coding`, `sdtm-vs-epoch-from-subject-elements`, `negative-terminology-standard-unknown`, `rust/crates/yamaa-application/tests/project_domain_submission.rs::a_terminology_standard_name_outside_the_vocabulary_fails_where_the_project_is_read`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_terminology_standard_name_outside_the_vocabulary_is_not_permitted`.

Both aliases are optional here; [DEFINE-4](define-xml.md#define-4) states when a
document requires an alias, and [CT-6](#ct-6) refuses one on a sponsor-extended
item, which has no published identifier.

The name takes the closed list an implementation guide's name takes, the eleven
standard names of Define-XML 2.1, which `yaml/vocabulary/cdisc.yaml` holds. It
is checked wherever the project's codelists are read, before any data and
whether or not a document is generated: a name outside the list fails with
`value_not_permitted` at the source's `standard.name`.

<a id="ct-12"></a>

### CT-12. A codelist's `format_name` begins with `$` exactly when the codelist is `text`

`format_name` is optional and names the SAS format a reader applies to a
codelist's coded values; [DEFINE-20](define-xml.md#define-20) writes it as
`SASFormatName`. A `text` codelist's `format_name` names a character format and
begins with `$`, and an `integer` or `float` codelist's names a numeric format
and does not. A `text` codelist declaring a `format_name` that does not begin
with `$`, or an `integer` or `float` codelist declaring one that does, fails
validation.

Fails: `value_not_permitted`.
Evidence: `negative-format-name-without-dollar`, `negative-numeric-format-name-with-dollar`, `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/project_execution.rs::format_names_begin_with_a_dollar_sign_exactly_for_text_codelists`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/environment.json::a_format_name_begins_with_a_dollar_sign_exactly_when_its_codelist_is_text`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/project_files.json::a_format_name_whose_dollar_sign_disagrees_with_its_data_type_fails_where_written`.

A `text` codelist's `format_name` that does not begin with `$`, or an
`integer` or `float` codelist's that does, fails with `value_not_permitted` at
the codelist's `format_name`, reporting the codelist, the value and the data
type. It is checked wherever the project's codelists are validated, in every
context and before any data is read, as [CT-6](#ct-6)'s checks are: a domain
build refuses it although only a generated document carries the name.

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
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/project_execution.rs::numeric_equality_is_exact_at_i64_and_binary64_boundaries`, `rust/crates/yamaa-application/tests/project_environment.rs::complete_static_report_keeps_semantics_alongside_original_opaque_capture_and_authored_holes`.

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
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/project_execution.rs::complete_independent_item_findings_survive_invalid_shape`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/environment.json::each_independent_finding_of_two_invalid_codelists_is_reported_in_order_at_its_path`.

A list whose values are already the words a reader needs has no decode to add.
Lists with and without `decode` are different kinds of list rather than a list
with gaps.

<a id="ct-6"></a>

### CT-6. Only an extensible published codelist may carry sponsor-extended items, and an extended item declares no alias

`extended` marks an item the sponsor added to a published list. It defaults to
false and may be true only on an `extensible` codelist whose source declares
`standard`. An extended item has no published identifier, so it declares no
`alias`. An `extended: true` item on a sponsor-defined or non-extensible
codelist, or one that declares `alias`, fails validation.

Fails: `codelist_extension_not_admitted`, `codelist_extended_alias`.
Evidence: `sdtm-ta-sponsor-epoch`, `rust/crates/yamaa-core/tests/project_execution.rs::extensions_require_both_a_published_source_and_an_extensible_list`, `rust/crates/yamaa-core/tests/project_execution.rs::extended_items_carry_no_published_alias`, `rust/crates/yamaa-core/tests/project_execution.rs::complete_independent_item_findings_survive_invalid_shape`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/environment.json::each_independent_finding_of_two_invalid_codelists_is_reported_in_order_at_its_path`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/environment.json::an_extended_item_of_a_published_extensible_codelist_declares_no_alias`.

Adding a value to a list that admits no additions is a contradiction rather
than an extension. A sponsor addition is an item like any other, so
[CT-7](#ct-7) enforces it with the published items, and
[DEFINE-20](define-xml.md#define-20) marks it `def:ExtendedValue="Yes"`.

An item's `alias` carries the published identifier of the value
([CT-2](#ct-2)), which [DEFINE-20](define-xml.md#define-20) writes as its NCI
code. A sponsor addition has none, so [DEFINE-4](define-xml.md#define-4)
requires no alias of it under `context: Submission`, and an alias declared on
it would claim a code the published list does not assign. An extension the
list does not admit fails with `codelist_extension_not_admitted` at the item's
`extended`; an alias on an extended item fails with `codelist_extended_alias`
at the item's `alias`, reporting the codelist, the item's value and the alias.
Both are checked wherever the project's codelists are validated, in every
context.

## Bindings

<a id="ct-13"></a>

### CT-13. A project binding needs a project file, and a specification checked or run without one fails validation

A specification that binds a codelist, declares an `inventory_vocabulary` or
calls a project function names something only a project file supplies.
Checked or run without a project file, it fails validation with
`project_binding_missing` at each such binding, before any input is read.
With a project file, a binding to an item the project does not declare fails
as [CT-1](#ct-1) and the [project-function rules](functions.md) state.

Fails: `project_binding_missing`.
Evidence: `negative-codelist-without-project`, `negative-inventory-vocabulary-without-project`, `rust/crates/yamaa/tests/project_binding.rs::each_binding_of_a_specification_without_its_project_fails_before_any_input_is_read`, `rust/crates/yamaa/tests/project_binding.rs::with_its_project_a_binding_names_what_the_project_declares`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::each_binding_of_a_specification_prepared_without_its_project_is_missing`.

The bindings are those of the resolved specification: the `codelist` of a
column or of a value-level entry ([SUB-12](submission.md#sub-12)), an
`inventory_vocabulary: true` on a column or an entry ([CT-11](#ct-11)), where
an entry's `false` binds nothing, and each `function` call a derivation
evaluates ([FUNC-1](functions.md#func-1)). A check and a run fail alike, and
the run reads no input.

Each binding is reported once, at the field that binds it, with `binding`
naming that field: a codelist at its `codelist`, with the `column` and the
`codelist`; an inventory vocabulary at its `inventory_vocabulary`, with the
`column`; a call at its `name`, with the `function`. They are reported with
the specification's other validation findings: the calls where
[FUNC-4](functions.md#func-4) reports an undeclared function, then each
column's codelists and inventory vocabularies, in column order, the column's
own before its entries'.

Nothing that needs the bound item is checked without it: no codelist's items,
data type or restatement ([CT-7](#ct-7), [CT-8](#ct-8), [CT-9](#ct-9)), no
where value against its where column's codelists
([SUB-12](submission.md#sub-12)), no dataset inventory ([CT-11](#ct-11)) and
no call against its function's signature ([FUNC-5](functions.md#func-5)).
The same specification checked or run with its project file checks each of
them, so it passes validation alone only when it binds nothing a project
supplies. A project file that declares no codelists or no functions supplies
none: a binding then fails as [CT-1](#ct-1) or
[FUNC-4](functions.md#func-4) states.

<a id="ct-7"></a>

### CT-7. A bound codelist that lists items is the column's value set, and an external dictionary enforces nothing

Binding a codelist that declares `items`, extensible or not, requires every
non-missing value of the column to equal one declared `value`, and a sponsor
addition to an extensible list is declared as an item with `extended: true`.
Missing values pass. The check runs where [Verification](../verification.md)
runs a column verification, over the completed column, and fails the same way.
A value-level binding decides the rows its entry selects. Binding an `external`
codelist enforces nothing.

Fails: `allowed_values_failed`.
Evidence: `negative-epoch-outside-extensible-list`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::per_value_codelists_check_the_rows_their_where_column_completes`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_external_dictionary_entry_replaces_the_columns_list_on_its_rows_and_saves`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_per_value_extensible_list_checks_its_own_rows_alone`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::a_value_outside_the_default_list_beside_an_extensible_entry_fails_its_allowed_values`, `rust/crates/yamaa-core/tests/project_execution.rs::owned_checked_environment_executes_fixed_codelists_before_later_cells_in_both_plan_forms`.

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
binding on the rows its entry's where clause selects: those rows are checked
against the entry's list alone, or against nothing when it is an `external`
codelist, and every row no entry declaring a codelist selects against the
column's list.

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
Evidence: `rust/crates/yamaa-core/tests/fixtures/diagnostics/codelist_binding.json::a_codelist_whose_data_type_its_column_does_not_admit_is_refused_when_prepared`, `rust/crates/yamaa-core/tests/project_execution.rs::binding_checks_exact_column_type_and_rejects_restated_item_sets`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::integer_terminology_items_admit_equal_float_published_values`.

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
Evidence: `negative-allowed-values-beside-codelist`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::an_allowed_values_check_beside_its_codelist_is_redundant`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/codelist_binding.json::an_extensible_catalogue_states_the_value_set_once`.

The failure reports the verification's path and, in `stated_by`, the binding's
path, with the column and the codelist. A binding's items are known where a
project supplies the codelists: a domain build with a project and a composed
document check the redundancy, and a specification checked or run without a
project file fails at the binding instead ([CT-13](#ct-13)). Not every
constrained column carries published terminology.

## Documents and inventories

<a id="ct-10"></a>

### CT-10. A generated document carries exactly the codelists its output columns bind

Each generated document includes exactly the project codelists bound by its
represented output columns, in their declaration order, and the
controlled-terminology standards cited by those sources. Unbound codelists may
serve another submission section and are omitted without an
unreferenced-codelist failure.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::unused_terminology_is_omitted_and_missing_document_links_do_not_read_study_files`, `python/tests/installed/installed_public_define.py::test_defaults_selection_bound_lists_and_required_acrf_aliases`.

<a id="ct-11"></a>

### CT-11. An inventory-vocabulary column holds only names of datasets any submission section of the project declares

A column carrying submission metadata may declare `inventory_vocabulary`. Every
non-missing value of such a column must equal the name of a dataset a
specification some submission section of the project file lists declares,
whichever section composes the column. Missing values pass, exactly as
[CT-7](#ct-7) treats missing values, and enforcement runs where
[Verification](../verification.md) runs a column verification, over the
completed column, and fails the same way.

Fails: `allowed_values_failed`.
Evidence: `rust/crates/yamaa-application/tests/project_domain_submission.rs::inventory_uses_inherited_declared_dataset_names_and_retained_original_metadata_in_both_hosts`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::the_inventory_names_an_inherited_dataset_for_python`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::the_inventory_names_an_inherited_dataset_for_r`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::the_inventory_of_a_specs_only_section_names_an_inherited_dataset_for_python`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::the_inventory_of_a_specs_only_section_names_an_inherited_dataset_for_r`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_inventory_of_a_project_without_datasets_admits_a_missing_value`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::an_inventory_of_a_project_without_datasets_admits_only_missing_values`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/submission.json::the_inventory_names_the_datasets_of_every_submission_section`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::predecessor_labels_and_the_dataset_inventory_span_every_section`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::a_published_name_outside_the_study_inventory_fails_its_recheck`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/shared_define.json::without_the_sdtm_section_a_predecessor_names_no_dataset_of_the_study`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_published_value_outside_the_dataset_inventory_fails`.

The admitted values are the study's dataset inventory, which no static list can
enumerate; that distinguishes this vocabulary from a codelist, whose values
[CT-3](#ct-3) requires be listed or deferred to a dictionary. The inventory is
the `domain` of every specification the `sdtm`, `adam` and `send` sections of
the project file list, so an ADaM `SRCDOM` may name the SDTM `AE` domain the
`sdtm` section lists. A domain build reads it from the project it is given; a
specification checked or run without a project file fails validation at each
`inventory_vocabulary` it declares ([CT-13](#ct-13)). Composition reads the
other sections' specifications for it, and Dataset-JSON's recheck of
published values uses the same inventory. A column that also prohibits
absence resolves `mandatory` to true ([SUB-11](submission.md#sub-11)). The
ADaM `SRCDOM` column, whose values name the source SDTM domain or ADaM
dataset, binds this vocabulary.
