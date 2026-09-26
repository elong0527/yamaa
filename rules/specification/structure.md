---
id: specification/structure
title: Specification structure
status: normative
---

# Specification structure

## Requirements

### The column list is declared

<a id="req-0196"></a>

**REQ-0196.** The artifact's columns come from the specification and from
nothing else, regardless of how many source fields are present. Each member
of a numbered family (`SMQ01NAM`, `SMQ01CD`, `SMQ02NAM`, and onwards, or
`CRIT1` beside `CRIT1FL`) is a separate declared column. The specification
fixes the family's size; a study whose reference data exceeds it must revise
the specification. Competing values for one member fail under
[Lookup and joins](../operations/lookup.md). A shared position suffix groups
family members by name but imposes no schema relationship; column `metadata`
may declare that relationship.

### Column coverage

<a id="req-0197"></a>

**REQ-0197.** Retired. A column-level derivation is the column's default derivation, stated once in [REQ-0199](#req-0199). This identifier is never reused.

<a id="req-0198"></a>

**REQ-0198.** Every declared column must be derived. A column with no
derivation anywhere fails validation and reports the column name.
Implementations must not fall back to a
same-named source variable; [Name binding](binding.md) forbids that inference.

<a id="req-0199"></a>

**REQ-0199.** A column-level derivation is the column's default derivation.
This applies to output and internal columns. A `rows` derivation overrides a
row-local default for that entry only; other entries inherit it.
[REQ-1260](#req-1260) determines when a column-level derivation remains in the
column phase and cannot be overridden.

<a id="req-0200"></a>

**REQ-0200.** A column with no column-level derivation must be derived in
every `rows` entry. Partial coverage fails validation and reports the entries
that omit the column; it does not imply a missing value. A column-level
derivation covers entries that do not override it.

<a id="req-0201"></a>

**REQ-0201.** A specification with no `rows` entry must derive every column at
column level. [REQ-0200](structure.md#req-0200) is vacuous when there are no entries.

<a id="req-0202"></a>

**REQ-0202.** A `rows` derivation must target a declared column. A key in
`derivations` that names no declared column fails validation.

<a id="req-0204"></a>

**REQ-0204.** A column whose value is intentionally absent is still derived.
Write `literal: null` rather than omitting the derivation.

<a id="req-1260"></a>

**REQ-1260.** A column-level derivation is row-local unless it uses a
lookup, an aggregate, or a window, reads a named intermediate, or reads a
column whose column-level derivation is not row-local. A row-local
column-level derivation is that column's default derivation when at least
one `rows` entry names the column, or when a row-phase context reads the
column: a `rows` derivation (windows included), a grouped `rows` filter, a
donor field of a `SELF` intermediate, or a match variable or `between`
value of a named intermediate that a `rows` derivation reads
([REQ-0126](../operations/lookup.md#req-0126)). A default's own reads of
column-level columns make those derivations defaults too. A qualified field
or a literal that only spells a column's name does not read that column. An
inherited default is evaluated in the inheriting `rows` entry's scope,
exactly as if the derivation were written in that entry. An entry's own
derivation overrides the default for that entry's rows only. Every other
column-level derivation keeps its column-phase meaning, and a `rows` entry
naming its column fails as `duplicate_derivation`.

### Output and internal columns

<a id="req-0206"></a>

**REQ-0206.** Internal columns hold working values a multi-step derivation
does not publish. They do not change evaluation. [Execution lifecycle](../execution/lifecycle.md) builds one dependency
graph over all declared columns regardless of `output`, and an output column
may depend on an internal one.

<a id="req-0207"></a>

**REQ-0207.** Retired; [REQ-0220](../storage/publication.md#req-0220)
governs output keys.

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
under this contract. A non-ASCII source byte fails validation with
`non_ascii_source`; report its file and position.

### Interface behavior

<a id="req-1042"></a>

**REQ-1042.** The `root_class` fields have these meanings:

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

**REQ-1043.** The `column_class` fields have these meanings:

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

**REQ-1044.** The `row_id` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `row_id` | Identifier of a row template, unique within rows. |

<a id="req-1045"></a>

**REQ-1045.** The `module` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `Scope` | [Schema language](../reference/schema-language.md) defines this notation: classes, descriptors, type expressions, and registries. Every field description here documents the field and does not validate it. |

<a id="req-1046"></a>

**REQ-1046.** The `identifier` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `identifier` | Single-token identifier; the field's own description states which naming list it belongs to (a declared dataset, a declared column, a standard, a document, a function, or similar). |

## Error conditions

<a id="req-0229"></a>

**REQ-0229.** Retired; see [REQ-0198](#req-0198).

<a id="req-0230"></a>

**REQ-0230.** Retired; [REQ-1260](#req-1260) governs column and row phase
derivation overlap.

<a id="req-0231"></a>

**REQ-0231.** Retired; see [REQ-0200](#req-0200).

<a id="req-0232"></a>

**REQ-0232.** Retired; see [REQ-0202](#req-0202).

<a id="req-0713"></a>

**REQ-0713.** Retired; see [REQ-0705](#req-0705).
