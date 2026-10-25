# Specification structure

A specification is the YAML file that derives one output dataset. These rules
fix its root fields, the identifiers it declares, which of its columns must be
derived and where, the columns it copies from a producer, and the bytes its
source may hold.

## Root fields and identifiers

<a id="spec-1"></a>

### SPEC-1. A specification derives one output dataset from the root fields it declares

A specification declares `yamaa` and `type: spec`
([SCHEMA-2](reference/schema-language.md#schema-2)), may name its CDISC model
in `model` ([SUB-3](profiles/submission.md#sub-3)), and derives the one output
dataset `domain` names. Its other root fields declare the row identity
(`keys`), the input datasets it reads (`input`), how output rows are built
(`base`, `filter`, `rows`), the values derived for them (`windows`,
`intermediates`, `columns`), the artifact (`output`), the assertions over the
completed dataset (`verifications`) and the metadata carried with it
(`submission`, `metadata`).

Each root field's schema description states its meaning.
[SCHEMA-2](reference/schema-language.md#schema-2) governs the header, and
[Inheritance](inheritance.md) resolves `parent` before validation and
execution. `filter` and `rows` exclude each other under
[Row construction](rows.md). `columns` are declared in the
dependency order [Column derivation](columns.md) requires, and
`output.columns` alone sets the artifact's column order
([OUTPUT-1](output.md#output-1)).

<a id="spec-2"></a>

### SPEC-2. Mapping keys, dataset identifiers, column names and row IDs are each unique

Within one specification a YAML mapping must not repeat a key, and dataset
identifiers, column names and row IDs must each be unique. A duplicate fails.
[Inheritance](inheritance.md) matches identifiers across layers before this
rule applies, so a later layer may refine one inherited declaration; a
duplicate within one layer remains an error, and the resolved specification
holds one declaration for each identifier.

Fails: `duplicate_identifier`.
Evidence: `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::a_column_name_is_declared_once`.

A duplicate within one layer also fails under [LAYER-4](inheritance.md#layer-4).
[Schema language](reference/schema-language.md) owns the corresponding
requirements for the schema bundle. An identifier is a single token; each
identifier field's description states the naming list it belongs to: a
declared dataset, a declared column, a standard, a document, a function or
similar. A row ID identifies one row template and is unique within `rows`.

## Columns

<a id="spec-3"></a>

### SPEC-3. The artifact's columns are declared, never generated from source fields

The artifact's columns come from the specification and from nothing else,
however many fields its sources carry. Each member of a numbered family, such
as `SMQ01NAM`, `SMQ01CD` and `SMQ02NAM`, or `CRIT1` beside `CRIT1FL`, is a
separate declared column. The specification fixes the family's size; a study
whose reference data exceeds it must revise the specification.

Evidence: `schema-structure`, `adam-adae-query-flags`, `negative-query-overflow`.

Competing values for one family member fail under
[Lookup and joins](lookup.md). A shared position suffix groups
family members by name but imposes no schema relationship; column `metadata`
may declare that relationship. A column declares its `name`, `type`, `label`,
`derivation`, `verifications`, `submission` and `metadata`; each field's
schema description states its meaning.

<a id="spec-4"></a>

### SPEC-4. Every declared column is derived, never filled from a same-named source variable

Every declared column must be derived. A column with no derivation anywhere
fails validation, reporting the column name; an implementation must not fall
back to, or infer, a same-named source variable. A column whose value is
intentionally absent is still derived: it is written `derivation: null` rather
than left without a derivation.

Fails: `missing_derivation`.
Evidence: `schema-structure`, `rust/crates/yamaa-application/tests/fixtures/preflight.tsv::declaration-order`.

<a id="spec-5"></a>

### SPEC-5. A column without a column-level derivation is derived in every row template

A column with no column-level derivation must be derived in every `rows`
entry; a column-level derivation covers the entries that do not override it.
Partial coverage fails validation, reporting the entries that omit the
column, and does not imply a missing value. A specification with no `rows`
entry must derive every column at column level. A `rows` derivation must
target a declared column.

Fails: `missing_derivation`, `undeclared_column`.
Evidence: `schema-structure`, `rust/crates/yamaa-application/tests/fixtures/preflight.tsv::ordered-missing-rows`.

Without `rows` entries the every-entry requirement is vacuous, and
[SPEC-4](#spec-4) requires a column-level derivation. A key in `derivations`
that names no declared column fails validation with `undeclared_column` at
that row derivation, naming the column.

<a id="spec-6"></a>

### SPEC-6. A row-local column-level derivation is a default that each row template may override

A column-level derivation is row-local unless it holds an aggregate or a
window operation, reads a `SELF` intermediate, or reads a column whose
column-level derivation is not row-local; a read through the implicit join or
an input-backed named intermediate is row-local. A structured `source` reads
its `between` value. A row-local column-level derivation is its column's
default for every `rows` entry that names the column or whose
row-construction context reads it, and an entry's own derivation overrides it.
Any other keeps its column-derivation meaning; a `rows` entry naming its column
fails.

Fails: `duplicate_derivation`.
Evidence: `schema-rows-override`, `schema-lookup-source-range-default`, `rust/crates/yamaa-core/tests/project_specification.rs`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/rows.json::transitive_nonlocal_defaults_report_one_issue_per_column_with_all_overriding_rows`, `rust/crates/yamaa-application/tests/fixtures/row_intermediate_reference.json`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_row_phase_boundaries.json::a_lookup_ranged_on_a_row_local_column_promotes_its_default`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_row_phase_boundaries.json::a_chained_lookup_promotes_the_default_its_range_values_read`, `rust/crates/yamaa-core/src/specification/phases.rs::a_template_cannot_replace_a_default_that_is_not_row_local`, `rust/crates/yamaa-core/src/specification/phases.rs::an_aggregate_in_a_case_result_is_not_row_local`, `rust/crates/yamaa-core/src/specification/phases.rs::a_row_phase_leaves_an_aggregate_in_a_case_result_to_the_column_phase`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_row_phase_boundaries.json::a_template_naming_a_default_with_an_aggregate_in_a_case_result_duplicates_it`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_row_phase_boundaries.json::a_template_naming_a_default_that_reads_a_self_intermediate_duplicates_it`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_row_phase_boundaries.json::an_ungrouped_template_reading_a_case_result_aggregate_default_is_a_phase_boundary`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_row_phase_boundaries.json::a_grouped_template_reading_a_case_result_aggregate_default_is_a_phase_boundary`, `rust/crates/yamaa-core/src/specification/phases.rs::a_structured_source_s_range_value_brings_its_default_into_the_row_phase`, `rust/crates/yamaa-core/src/specification/phases.rs::a_structured_source_whose_range_value_reads_a_window_is_not_row_local`.

The rule applies to output and internal columns alike. A row-construction
context is one of these:

- a `rows` derivation, windows included;
- a grouped `rows` filter;
- a donor field of a `SELF` intermediate;
- a match variable or `between` value of a named intermediate that a `rows`
  derivation reads ([LOOKUP-2](lookup.md#lookup-2)), and of each further
  intermediate that such a `between` value reads.

An aggregate or a window operation makes its derivation not row-local as the
whole expression or as a `case` result, whichever branch a row selects. A
structured `source`'s `between` value is a read of the derivation that holds
the source, whether the source is that whole derivation or a `case` result in
it. A row-local default that the value names therefore completes in the row
construction of each `rows` entry whose row-construction context holds the
source, and a column-level derivation whose `between` value reads a column that
is not row-local is not row-local either.

An entry's own derivation replaces the default for that entry's rows only, and
the other entries inherit it. A read of a named intermediate backed by an input
dataset, including a read through another row-local column, may be such a
default. A column-level read of a `SELF` intermediate keeps its
column-derivation meaning under [LOOKUP-7](lookup.md#lookup-7). A default's own
reads of column-level columns make those derivations defaults too. A qualified
field or a literal that only spells a column's name does not read that column.
An inherited default is evaluated in the inheriting `rows` entry's scope,
exactly as if the derivation were written in that entry. A `rows` entry that
names a column whose column-level derivation is not row-local fails as
`duplicate_derivation`.

<a id="spec-7"></a>

### SPEC-7. A declared column outside `output.columns` is internal and is evaluated like an output column

A column listed in `output.columns` is part of the artifact; every other
declared column is internal. An internal column is derived, converted,
verified and shared with dependents exactly as an output column is, but is
omitted from the artifact. One dependency graph covers every declared column
regardless of `output`, and an output column may depend on an internal one.

Evidence: `schema-structure`, `sdtm-relrec-links`.

Internal columns hold the working values of a multi-step derivation and do
not change evaluation; [Column derivation](columns.md) builds
the dependency graph. Column verifications may be declared on an internal
column and run normally. Dataset verifications may reference an internal
column; they then assert a property of the derivation rather than of the
artifact.

<a id="spec-9"></a>

### SPEC-9. A `copy` entry declares the listed columns of a schema-linked input under their names, with the producer's type and label

A `columns` entry `copy: {dataset: D, columns: [...]}` declares one column per
listed name, and each name must be an output column of the producer that input
`D` links through `schema` ([INPUT-10](inputs.md#input-10)). Each copied column
reads the same-named column of `D` unchanged and takes the producer's `type`,
`label` and content metadata; its submission origin is `Predecessor`, and an
optional `submission` adds only `core`, `mandatory` and `role`. The entry
expands to ordinary columns in the resolved specification, and a copied column
is published only when `output.columns` lists it.

Fails: `unknown_field`, `duplicate_identifier`, `prohibited_construct`, `producer_workflow_cycle`.
Evidence: `schema-copy-producer-columns`, `negative-copy-unknown-column`, `negative-copy-duplicate-column`, `negative-copy-unlinked-input`, `rust/crates/yamaa/tests/copy_entry.rs`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/producers.json::a_name_a_copy_entry_lists_that_another_column_of_its_layer_declares_is_repeated`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/producers.json::a_copy_entry_dataset_that_names_no_input_is_unknown`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/producers.json::a_copy_entry_dataset_whose_input_has_no_schema_link_refuses_the_copy`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/producers.json::a_copy_entry_dataset_whose_input_links_its_producer_is_admitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/producers.json::a_copied_name_that_is_not_an_output_column_of_the_producer_is_unknown`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/producers.json::a_producer_whose_copies_lead_back_to_a_specification_being_resolved_is_a_cycle`, `rust/crates/yamaa-core/tests/project_function_invocation.rs::a_copy_cycle_names_each_specification_on_it_in_its_message`.

```yaml
input:
  AE: input/ae.csv
  ADSL: {path: adsl.csv, schema: spec_adsl.yaml, key: [STUDYID, USUBJID]}
columns:
  - copy:
      dataset: ADSL
      columns: [AGE, SEX, TRTSDT]
      submission: {core: Perm}
```

Each layer's entries expand before the layers compose
([LAYER-5](inheritance.md#layer-5)): a listed name `X` becomes the column `X`
with the producer's `type` and `label` and the derivation `D.X`, a bare
reference. The input, its `schema` link and the base the link resolves from
are the ones the resolved specification keeps, whichever layer writes them.
The resolved specification is therefore the one the columns written out give:
a later layer overrides a copied column by name, and `clear` addresses one of
its fields, as for any column ([LAYER-9](inheritance.md#layer-9)). A renamed
copy is an ordinary column ([SUB-20](profiles/submission.md#sub-20)).

A copied column that `output.columns` lists carries submission metadata when
the specification declares its `model`: the entry's `core`, `mandatory` and
`role`, the producer's content metadata, and one `Predecessor` origin whose
`description` is `<producer domain>.X`. A copied column that `output.columns`
does not list is internal ([SPEC-7](#spec-7)) and carries none
([SUB-1](profiles/submission.md#sub-1)); like any internal column, it is
pruned when nothing reads it ([LAYER-11](inheritance.md#layer-11)). An entry
that writes `submission` gives its columns submission metadata in either case,
so an internal one that another column reads then fails under SUB-1, and a
specification without `model` under [SUB-3](profiles/submission.md#sub-3).

The producer's content metadata is `data_type`, `length`, `significant_digits`,
`display_format` and `codelist` where it declares them, a `str` column's
`length` derived from its `max_length` verification where none is declared
([SUB-8](profiles/submission.md#sub-8)). A `date` or `datetime` column takes no
`data_type`, `length` or `display_format`: it is submitted as the consumer's
model requires ([SUB-6](profiles/submission.md#sub-6),
[SUB-25](profiles/submission.md#sub-25)). A copied `codelist` is a binding of
the consumer's project ([CT-13](profiles/terminology.md#ct-13)). The entry's
`submission` admits only `core`, `mandatory` and `role`, so `comment`,
`method`, `value_level`, `origins` and every other field fail as
`unknown_field` ([SCHEMA-13](reference/schema-language.md#schema-13)).

The producer is read as its link reads it: a producer that cannot be read
fails at `input.<D>.schema` as the link does, a link to the consuming
specification fails under [INPUT-10](inputs.md#input-10), and a producer whose
own columns copy reads its producers in turn. A copy whose producers lead back
to a specification being resolved fails as `producer_workflow_cycle` at the
`input.<D>.schema` of the link that closes the cycle, its `cycle` naming the
specifications from the first one entered back to itself. While a producer is
read, the entry and each layer up the chain to the last one with a copy entry
that reads it count as being resolved, so an empty child reports the cycle its
parent reports. A copy from an input that does not drive the rows is an
implicit join and needs the input's `key` ([LOOKUP-1](lookup.md#lookup-1)),
and a copy through an input whose name is not the producer's `domain` reads
another dataset than its origin names, which SUB-20 refuses.

An entry has no name, so a path names it by its position, `columns[<i>]`. A
`dataset` that names no declared input fails as `unknown_field` with
`identifier`, and one that names an input without `schema` as
`prohibited_construct` with `dataset`, each at `columns[<i>].copy.dataset`. A
listed name that is not an output column of the producer fails as
`unknown_field` at `columns[<i>].copy.columns[<k>]`, with `identifier`
`D.X`. A name an entry lists that another column of the same layer declares,
or another entry lists, fails as `duplicate_identifier`
([SPEC-2](#spec-2)) at the later of the two, with `identifier`. These
refusals come before the layers compose, so each is reported in the layer
that writes the entry.

## Language source

<a id="spec-8"></a>

### SPEC-8. Language source is ASCII

Schema documents, specifications, inheritance layers, project files, codelist
files and function files contain only bytes from `0x00` through `0x7F`. A non-ASCII source byte fails
validation, reporting its file and position. A format's ASCII escape notation
may denote a non-ASCII value; the decoded value is then an ordinary string.

Fails: `non_ascii_source`.
Evidence: `rust/crates/yamaa-application/tests/fixtures/diagnostics/workflow.json::a_yaml_entry_byte_outside_ascii_names_the_entry_and_its_position`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_yaml_source_byte_outside_ascii_names_the_file_and_its_position`, `rust/crates/yamaa/tests/issue_locations.rs::a_parent_the_reader_refuses_locates_its_issue_in_that_parent`.

For example, the YAML double-quoted escape `"\u00E9"` denotes the
one-character string U+00E9.

The repository holding the language keeps the same boundary: its conformance
documents, rules, documentation and implementation source are ASCII, and they
name a non-ASCII character by its code point, such as `U+00E9`, rather than
embedding it. Repository validation enforces this.
