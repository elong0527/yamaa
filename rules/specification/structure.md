---
id: specification/structure
title: Specification structure
status: normative
---

# Specification structure

## Purpose

Declare identifiers, columns, derivation coverage, and source notation.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Execution lifecycle](../execution/lifecycle.md).
- [Verification](../execution/verification.md).
- [Lookup and joins](../operations/lookup.md).
- [Schema language](../reference/schema-language.md).
- [Name binding](binding.md).
- [Specification composition](composition.md).
- [Submission metadata](../submission/metadata.md).


## Requirements

### The column list is declared

<a id="req-0196"></a>

**REQ-0196.** The artifact's columns come from the specification and from
nothing else. A source carrying more than the specification declares
does not extend the artifact, and a source carrying fewer does not
shorten the artifact. Each member of a numbered family (`SMQ01NAM`, `SMQ01CD`,
`SMQ02NAM`, and onwards, or `CRIT1` beside `CRIT1FL`) is a declared column
like any other, so the count is fixed when the specification is written. A
study whose reference data outgrows that count is re-read against the data
rather than left to fill the places it already has. A second value
competing for one declared place is the ordinary multiple-match failure
[Lookup and joins](../operations/lookup.md) defines, not a new place. The members of one family name their
grouping by position: `SMQ02NAM`, `SMQ02CD`, and `SMQ02SC` belong together.
Each carries the `02`. Nothing in the schema links them beyond the
`02`. A study that wants the grouping checkable records it in the columns'
`metadata`; the schema does not.

### Column coverage

<a id="req-0197"></a>

**REQ-0197.** A column-level derivation is the column's default derivation.
The requirements below make that precise, and they apply to internal columns
exactly as they apply to output ones. A `rows` entry may override the default
for that entry's rows.

<a id="req-0198"></a>

**REQ-0198.** Every declared column must be derived. A column with no
derivation anywhere is an error. Implementations must not fall back to a
same-named source variable; [Name binding](binding.md) forbids that inference.

<a id="req-0199"></a>

**REQ-0199.** A column-level derivation is the column's default derivation.
A `rows` entry naming the column in its `derivations` overrides the default
for that entry's rows only; an entry not naming the column inherits the
default. Pairing a column-level derivation with row-level derivations for
the same column is therefore covered, not duplicated.

<a id="req-0200"></a>

**REQ-0200.** A column with no column-level derivation must be derived in
every `rows` entry. Deriving a column in only some entries leaves other
constructed rows with no value. Partial row coverage is an error, not an
implied missing value. A column with a column-level derivation is covered
whether or not any entry overrides it.

<a id="req-0201"></a>

**REQ-0201.** A specification with no `rows` entry must derive every column at
column level. [REQ-0200](structure.md#req-0200) is vacuous when there are no entries.

<a id="req-0202"></a>

**REQ-0202.** A `rows` derivation must target a declared column. A key in
`derivations` that names no declared column is an error.

<a id="req-0203"></a>

**REQ-0203.** Mixing placements across columns is normal: a specification
with `rows` typically derives the columns that distinguish its row
templates at row level and all other columns at column level.

<a id="req-0204"></a>

**REQ-0204.** A column whose value is intentionally absent is still derived.
Write `literal: null` rather than omitting the derivation.

<a id="req-1260"></a>

**REQ-1260.** A column-level derivation is that column's default derivation
when at least one `rows` entry names the column, or when a row-phase context
(a `rows` derivation or filter, an intermediate derivation, filter, or
ordering, or a window) references the column, transitively. The reference rule applies only
to row-local derivations; a derivation needing dataset-level operations
(lookup, aggregate, or a qualified intermediate reference) keeps its
column-phase meaning even when referenced. An inherited default is
evaluated in the inheriting `rows` entry's scope, exactly as if the
derivation were written in that entry. An entry's own derivation overrides
the default for that entry's rows only. A column-level derivation that no
`rows` entry names and no row-phase context references keeps its
column-phase meaning.

### Output and internal columns

<a id="req-0206"></a>

**REQ-0206.** Internal columns hold working values a multi-step derivation
does not publish. They do not change evaluation. [Execution lifecycle](../execution/lifecycle.md) builds one dependency
graph over all declared columns regardless of `output`, and an output column
may depend on an internal one.

<a id="req-0207"></a>

**REQ-0207.** `keys` must name output columns only. An internal column in
`keys` is an error. A key identifies rows in the artifact.

<a id="req-0208"></a>

**REQ-0208.** Column verifications may be declared on an internal column and
run normally.

<a id="req-0209"></a>

**REQ-0209.** Dataset verifications may reference an internal column. They
then assert a property of the derivation rather than of the artifact.

### Specification-wide uniqueness

<a id="req-0227"></a>

**REQ-0227.** Within one specification, implementations must reject duplicate
YAML mapping keys. Dataset identifiers, column names, and row IDs must
each be unique. [Schema language](../reference/schema-language.md) owns the corresponding requirements for the schema
bundle.

<a id="req-0228"></a>

**REQ-0228.** [Specification composition](composition.md) matches identifiers across inheritance layers before this
rule applies, so a later layer may refine one inherited declaration.
Duplicate identifiers within a single layer remain an error. The resolved
specification contains one declaration for each identifier.

### Source and data boundary

<a id="req-0705"></a>

**REQ-0705.** Language source is ASCII. Schema documents, specifications,
inheritance layers, project environments, conformance documents, rules,
documentation, and implementation source contain only bytes from `0x00`
through `0x7F`. Names and examples in source use forms such as `U+00E9`
rather than embedding the character. A format's ASCII escape notation may
denote a non-ASCII value; the decoded value is then an ordinary string
under this contract.

### Interface behavior

<a id="req-1042"></a>

**REQ-1042.** The `root_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `root_class.schema_version` | Schema bundle this specification is written against; [Schema language](../reference/schema-language.md) requires an exact match. |
| `root_class.domain` | Name of the output dataset this specification derives. |
| `root_class.keys` | Output columns identifying a row; [Artifact publication](../storage/publication.md) defines the identity they assert. |
| `root_class.input` | Source datasets readable by this specification, each under the name it is read through. |
| `root_class.base` | Input dataset whose records build output rows when rows is absent; [Row construction](../execution/rows.md) states when it is required. |
| `root_class.parents` | Ordered local specification layers resolved under [Specification composition](composition.md) before validation and execution. |
| `root_class.windows` | Complete named window settings under [REQ-1251](../operations/windows.md#req-1251). |
| `root_class.intermediates` | Named dataset lookups several columns read through <lookup-id>.<column>; [Lookup and joins](../operations/lookup.md) defines them. |
| `root_class.output` | Artifact presentation independent of dependency-ordered declarations; [Artifact publication](../storage/publication.md) defines it. |
| `root_class.columns` | Columns in the dependency order [Execution lifecycle](../execution/lifecycle.md) requires; output.columns controls artifact order. |
| `root_class.rows` | Row templates constructing output rows from input records or groups; [Row construction](../execution/rows.md) evaluates them. Mutually exclusive with `root_class.filter`. |
| `root_class.filter` | Predicate selecting base input records for row construction when `rows` is absent; the filter-only row template lifted to root. Mutually exclusive with `root_class.rows`; [Row construction](../execution/rows.md) defines the driver it reads. |
| `root_class.verifications` | Assertions over the completed dataset; [Verification](../execution/verification.md) defines them. |
| `root_class.submission` | Governed dataset metadata a submission document is generated from; [Submission metadata](../submission/metadata.md) defines it. |
| `root_class.metadata` | Free-form annotations carried with the specification; [Submission metadata](../submission/metadata.md) reserves the key names it governs. |

<a id="req-1043"></a>

**REQ-1043.** The `column_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `column_class.name` | Column name this specification declares. |
| `column_class.type` | Declared data type this column must carry in the completed dataset. |
| `column_class.label` | Descriptive text carried with the column for a reader of the dataset. |
| `column_class.derivation` | How the column's value is produced; [Specification structure](structure.md) owns where a column may be derived. |
| `column_class.verifications` | Assertions over this column's completed values; [Verification](../execution/verification.md) defines them. |
| `column_class.submission` | Governed column metadata a submission document is generated from; [Submission metadata](../submission/metadata.md) defines it. |
| `column_class.metadata` | Free-form annotations carried with the column; [Submission metadata](../submission/metadata.md) reserves the key names it governs. |

<a id="req-1044"></a>

**REQ-1044.** The `row_id` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `row_id` | Identifier of a row template, unique within rows. |

<a id="req-1045"></a>

**REQ-1045.** The `module` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `Scope` | [Schema language](../reference/schema-language.md) defines this notation: classes, descriptors, type expressions, and registries. Every field description here documents the field and does not validate it. |

<a id="req-1046"></a>

**REQ-1046.** The `identifier` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `identifier` | Single-token identifier; the field's own description states which naming list it belongs to (a declared dataset, a declared column, a standard, a document, a function, or similar). |

## Error conditions

<a id="req-0229"></a>

**REQ-0229.** A declared column with no derivation: fail and report the
column name.

<a id="req-0230"></a>

**REQ-0230.** A column derived both at column level and in a `rows` entry:
fail.

<a id="req-0231"></a>

**REQ-0231.** A column derived in some `rows` entries but not all: fail and
report the entries that omit it.

<a id="req-0232"></a>

**REQ-0232.** A `rows` derivation naming an undeclared column: fail.

<a id="req-0713"></a>

**REQ-0713.** A non-ASCII byte in language or repository source: fail
validation with `non_ascii_source` and report the file and position.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [sdtm-dm-dates](../../benchmarks/sdtm-dm-dates/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Declare identifiers, columns, derivation coverage, and source notation. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
