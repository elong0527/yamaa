# Specification structure

A specification is the YAML file that derives one output dataset. These rules
fix its root fields, the identifiers it declares, which of its columns must be
derived and where, and the bytes its source may hold.

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
Evidence: `rust/crates/yamaa-core/tests/support/diagnostic/preflight.rs`.

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
Evidence: `schema-structure`, `rust/crates/yamaa-adapters/tests/fixtures/preflight.tsv::declaration-order`.

<a id="spec-5"></a>

### SPEC-5. A column without a column-level derivation is derived in every row template

A column with no column-level derivation must be derived in every `rows`
entry; a column-level derivation covers the entries that do not override it.
Partial coverage fails validation, reporting the entries that omit the
column, and does not imply a missing value. A specification with no `rows`
entry must derive every column at column level. A `rows` derivation must
target a declared column.

Fails: `missing_derivation`, `undeclared_column`.
Evidence: `schema-structure`, `rust/crates/yamaa-adapters/tests/fixtures/preflight.tsv::ordered-missing-rows`.

Without `rows` entries the every-entry requirement is vacuous, and
[SPEC-4](#spec-4) requires a column-level derivation. A key in `derivations`
that names no declared column fails validation with `undeclared_column` at
that row derivation, naming the column.

<a id="spec-6"></a>

### SPEC-6. A row-local column-level derivation is a default that each row template may override

A column-level derivation is row-local unless it uses a lookup, an aggregate
or a window, reads a `SELF` intermediate, or reads a column whose
column-level derivation is not row-local. A row-local column-level derivation
is its column's default when a `rows` entry names the column or a row-phase
context reads it; an entry's own derivation overrides the default for that
entry's rows only, and other entries inherit it. Any other column-level
derivation keeps its column-phase meaning, and a `rows` entry naming its
column fails.

Fails: `duplicate_derivation`.
Evidence: `schema-rows-override`, `rust/crates/yamaa-core/tests/project_specification.rs`, `rust/crates/yamaa-adapters/tests/fixtures/row_intermediate_reference.json`, `rust/crates/yamaa-adapters/tests/execution_row_phase_reads.rs`.

The rule applies to output and internal columns alike. A row-phase context is
one of these:

- a `rows` derivation, windows included;
- a grouped `rows` filter;
- a donor field of a `SELF` intermediate;
- a match variable or `between` value of a named intermediate that a `rows`
  derivation reads ([LOOKUP-2](lookup.md#lookup-2)), and of each further
  intermediate that such a `between` value reads.

A read of a named intermediate backed by an input dataset, including a read
through another row-local column, may be such a default. A column-level read
of a `SELF` intermediate keeps its column-phase meaning under
[LOOKUP-7](lookup.md#lookup-7). A default's own reads of
column-level columns make those derivations defaults too. A qualified field
or a literal that only spells a column's name does not read that column. An
inherited default is evaluated in the inheriting `rows` entry's scope, exactly
as if the derivation were written in that entry. A `rows` entry that names a
column whose column-level derivation is not row-local fails as
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

## Language source

<a id="spec-8"></a>

### SPEC-8. Language source is ASCII

Schema documents, specifications, inheritance layers, project files and
environment files contain only bytes from `0x00` through `0x7F`. A non-ASCII source byte fails
validation, reporting its file and position. A format's ASCII escape notation
may denote a non-ASCII value; the decoded value is then an ordinary string.

Fails: `non_ascii_source`.
Evidence: `rust/crates/yamaa-adapters/tests/file_project.rs::original_decode_diagnostics_remain_bounded_and_owned`, `rust/crates/yamaa-core/tests/support/diagnostic/yaml_source.rs`.

For example, the YAML double-quoted escape `"\u00E9"` denotes the
one-character string U+00E9.

The repository holding the language keeps the same boundary: its conformance
documents, rules, documentation and implementation source are ASCII, and they
name a non-ASCII character by its code point, such as `U+00E9`, rather than
embedding it. Repository validation enforces this.
