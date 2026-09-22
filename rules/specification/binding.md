---
id: specification/binding
title: Name binding
status: normative
---

# Name binding

## Purpose

Resolves input datasets, current-output columns, and contextual ODM references.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Local handlers](../execution/handlers.md).
- [Execution lifecycle](../execution/lifecycle.md).
- [Aggregation](../operations/aggregation.md).
- [Expression evaluation](../operations/expressions.md).
- [Lookup and joins](../operations/lookup.md).
- [Text operations](../operations/text.md).
- [Specification composition](composition.md).
- [Source ingestion](../storage/ingestion.md).
- [Resource resolution](../storage/resources.md).
- [Text values](../values/text.md).

## Requirements

### Dataset declarations

<a id="req-0076"></a>

**REQ-0076.** `input` maps dataset identifiers to source data
declarations. Identifiers are used by `base`, `rows.dataset`, qualified
source variables, and `lookup`.

<a id="req-0077"></a>

**REQ-0077.** A declaration is a path, with or without types for its fields.
[Source ingestion](../storage/ingestion.md) owns both forms and shorthand.

<a id="req-0078"></a>

**REQ-0078.** A declared path is a `project_path`. [Resource resolution](../storage/resources.md) fixes its written form,
approved root, and readable bytes. [Specification composition](composition.md) preserves the path origin when a
declaration is inherited. [Specification composition](composition.md) rebases a relative path in a materialized
resolved specification.

<a id="req-0079"></a>

**REQ-0079.** Every referenced dataset identifier must exist in `input`.

<a id="req-0080"></a>

**REQ-0080.** A dataset identifier must not equal the output `domain`.

<a id="req-0081"></a>

**REQ-0081.** A finished dataset from an earlier run is an ordinary source.
Declare that source under its own name.

<a id="req-0082"></a>

**REQ-0082.** No keyed construct reaches a sibling record of the output
dataset. [Execution lifecycle](../execution/lifecycle.md) owns what happens when a column reaches its own value through
that column's window partition. Addressing a sibling record by key is open
work.

### Source expressions

<a id="req-0083"></a>

**REQ-0083.** The concise source form names one variable:

```yaml
source: DM.SEX
```

`DATASET.VARIABLE` refers to `VARIABLE` in the declared
source dataset. An unqualified reference such as `AVAL` refers to a
variable in the output dataset.

<a id="req-0084"></a>

**REQ-0084.** A qualifier is a dataset or record lookup identifier. Both
share one namespace. Lookup and joins defines record lookup resolution.

<a id="req-0085"></a>

**REQ-0085.** A qualified reference to the current row template's input
dataset reads the current input record. A qualified reference to
another dataset follows [Lookup and joins](../operations/lookup.md).

<a id="req-0086"></a>

**REQ-0086.** During grouped row construction there is no single current
input record. A qualified reference to the input dataset of the row
template is scalar only when the exact variable appears in the enclosing
`row.group_by`. That reference then returns the key value of that group.
A column-level derivation reads each constructed row, so the same holds
there: a qualified reference to a row template's input dataset is scalar
only when the exact variable appears in the `group_by` of every grouped
row template driven by that dataset.

<a id="req-0087"></a>

**REQ-0087.** Reading any other source variable with a scalar `source` is
an error. A grouped row reduces non-key variables with an aggregate
expression, as [Expression evaluation](../operations/expressions.md) and [Aggregation](../operations/aggregation.md) define.

<a id="req-0088"></a>

**REQ-0088.** Operation operand fields typed as `variable` accept a
concise source or current output variable. Compose operations through
a named derived column:

```yaml
- name: PREFIX
  type: str
  derivation:
    str_extract:
      source: RAW.TEXT
      pattern: '^[A-Z]+'
- name: CATEGORY
  type: str
  derivation:
    mapping:
      source: PREFIX
      dict: {A: Alpha}
```

<a id="req-0089"></a>

**REQ-0089.** An operation cannot place an arbitrary nested expression in
a variable field.

<a id="req-0090"></a>

**REQ-0090.** A `string_template` placeholder is a variable reference. [Text operations](../operations/text.md)
owns braces and escaping. The placeholder's complete name binds as a
`variable` field name. Text outside placeholders is literal.

<a id="req-0091"></a>

**REQ-0091.** Plain strings outside fields typed as `variable` are
literal under [Text values](../values/text.md).

<a id="req-0092"></a>

**REQ-0092.** Implementations must not infer same-named source variables
when an output column has no derivation.

### Structured source binding

<a id="req-0093"></a>

**REQ-0093.** The `source` expression also accepts an object containing
`variable` and local binding or join behavior:

```yaml
source:
  variable: ADSL.TRTSDT
  missing: null
```

<a id="req-0094"></a>

**REQ-0094.** `missing` and `multiple_matches` are handlers; [Local handlers](../execution/handlers.md) defines
them and [Lookup and joins](../operations/lookup.md) defines the join uniqueness `multiple_matches` relaxes.

<a id="req-0095"></a>

**REQ-0095.** `filter` is not a handler. It states which records the
source may read, and [Lookup and joins](../operations/lookup.md) defines that selection. Reading no record is an
absent match rather than a handled condition.

### ODM contextual references

<a id="req-0096"></a>

**REQ-0096.** This form is retired. A source states which records it reads through
`filter`, which [REQ-0131](../operations/lookup.md#req-0131) owns, so an ODM item is addressed by a predicate
over `ItemOID` rather than by hiding that identifier in the variable name.
No specification in the repository still uses this form, and the
repository validator rejects it without exemption. The requirements below
describe only what the engine still executes, and leave the language when
that code does.

ODM item identifiers may contain periods.
`ODM.IT.LB.LBDTC` means the `Value` whose `ItemOID` is `IT.LB.LBDTC`,
resolved within the current ODM context.

<a id="req-0097"></a>

**REQ-0097.** An ODM context is the current row's values for the
following columns, in this order, when those columns exist in the
declared ODM projection:

1. `StudyOID`;
2. `MetaDataVersionOID`;
3. `SubjectKey`;
4. `StudyEventOID`;
5. `StudyEventRepeatKey`;
6. `FormOID`;
7. `FormRepeatKey`;
8. `ItemGroupOID`;
9. `ItemGroupRepeatKey`.

<a id="req-0098"></a>

**REQ-0098.** ODM resolution first matches every available context column,
then matches the complete `ItemOID`. A projection may omit a context
column only when the projection's source does not carry that level.

<a id="req-0099"></a>

**REQ-0099.** A projection that carries `FormOID` must use that column.
Identical item identifiers in two forms are different contextual values.
Those values must not be collapsed.

<a id="req-0100"></a>

**REQ-0100.** No contextual match is an absent item. The reference fails
unless a structured source declares `missing`, under [Local handlers](../execution/handlers.md).

<a id="req-0101"></a>

**REQ-0101.** More than one match after applying every available context
column is a multiple right-side match. The reference fails unless a
structured source declares `multiple_matches`, also under [Local handlers](../execution/handlers.md).

<a id="req-0102"></a>

**REQ-0102.** A matched row with missing `Value` returns missing. It does
not invoke the absent-item handler.

### Interface behavior

<a id="req-1057"></a>

**REQ-1057.** The `variable` interface has these meanings. Its schema
declaration defines shape, defaults, and structural constraints.

| Field | Meaning |
| --- | --- |
| `variable` | Qualified source variable or unqualified current-output column. |

<a id="req-1058"></a>

**REQ-1058.** The `regex` interface has these meanings. Its schema
declaration defines shape, defaults, and structural constraints.

| Field | Meaning |
| --- | --- |
| `regex` | Regular expression applied to a string value under [Text operations](../operations/text.md). |

## Error conditions

<a id="req-0103"></a>

**REQ-0103.** An unknown dataset identifier or variable: fail.

<a id="req-0104"></a>

**REQ-0104.** A dataset identifier equal to the output `domain`: fail.

<a id="req-0105"></a>

**REQ-0105.** A source path that [Resource resolution](../storage/resources.md) does not accept: fail, under [Resource resolution](../storage/resources.md)'s
condition.

<a id="req-0106"></a>

**REQ-0106.** An unresolved unqualified reference: fail with `unknown_field`,
  or with `unresolvable_name` and the suggested qualified spelling when the
  bare name is a field of an in-scope dataset.

<a id="req-0107"></a>

**REQ-0107.** A scalar source naming a source variable absent from the
`group_by` of the grouped row template that builds the row it reads:
fail. At column level the source is read by every constructed row, so the
variable must appear in the `group_by` of every grouped row template
driven by its dataset.

<a id="req-0108"></a>

**REQ-0108.** An ODM contextual reference with no available context
column: fail.

<a id="req-0109"></a>

**REQ-0109.** More than one ODM contextual match: fail unless locally
handled.

<a id="req-0110"></a>

**REQ-0110.** No ODM contextual match: fail unless locally handled.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [adam-adsl-text](../../benchmarks/schema-text-functions/README.md).
- [negative-source-self-reference](../../benchmarks/negative-source-self-reference/README.md).
- [negative-source-undeclared-field](../../benchmarks/negative-source-undeclared-field/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Keeping this topic in one contract lets other owners refer to it without
defining a second policy.
