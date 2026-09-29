---
id: specification/binding
title: Name binding
status: normative
---

# Name binding

## Requirements

### Dataset declarations

<a id="req-0076"></a>

**REQ-0076.** `input` maps dataset identifiers to source data
declarations. Identifiers are used by `base`, `rows.dataset`, qualified
source variables, and named intermediates. A row template may also name a named
intermediate under [REQ-1262](../execution/rows.md#req-1262).

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

**REQ-0080.** A dataset identifier must not equal the output `domain`;
validation fails when they match.

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

**REQ-0084.** A qualifier is a dataset or record lookup identifier. Datasets
and record lookups share one naming list. Lookup and joins defines record
lookup resolution.

<a id="req-0085"></a>

**REQ-0085.** A qualified reference to the current row template's driver
reads the current driver record. The driver is an input dataset or an eligible
named intermediate under [REQ-1262](../execution/rows.md#req-1262). A
qualified reference to another dataset follows [Lookup and joins](../operations/lookup.md).

<a id="req-0086"></a>

**REQ-0086.** During grouped row construction there is no single current
driver record. A qualified reference to the driver of the row
template is scalar only when the exact variable appears in the enclosing
`row.group_by`. That reference then returns the key value of that group.
A column-level derivation reads each constructed row, so the same holds
there: a qualified reference to a row template's driver is scalar
only when the exact variable appears in the `group_by` of every grouped
row template driven by that dataset.

<a id="req-0087"></a>

**REQ-0087.** Reading any other source variable with a scalar `source` is
an error. A grouped row reduces non-key variables with an aggregate
expression, as [Expression evaluation](../operations/expressions.md) and [Aggregation](../operations/aggregation.md) define.

<a id="req-0088"></a>

**REQ-0088.** Operation operand fields typed as `variable` accept a
concise source or constructed output variable. Compose operations through
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
  absent: null
```

<a id="req-0094"></a>

**REQ-0094.** `absent` and a source's `order_by`/`keep` selection are
handlers; [Local handlers](../execution/handlers.md) defines them and [Lookup and joins](../operations/lookup.md) defines the join
uniqueness the selection relaxes.

<a id="req-0095"></a>

**REQ-0095.** `filter` is not a handler. It states which records the
source may read, and [Lookup and joins](../operations/lookup.md) defines that selection. Reading no record is an
absent match rather than a handled condition.

### ODM item reads

<a id="req-1265"></a>

**REQ-1265.** The `odm` expression reads one collected item from an ODM
input. `item` names the input and the item as `DATASET.ItemOID`. A dataset
identifier holds no period, so the qualifier ends at the first period and
the rest is the complete `ItemOID`, periods included. A bare string is the
[Schema language](../reference/schema-language.md) shorthand for `{item: ...}`:

```yaml
derivation: {odm: ODM.IT.DM.AGE}
```

<a id="req-1266"></a>

**REQ-1266.** An ODM input is an input dataset an `odm` expression names.
Its schema is fixed: the eleven fields `StudyOID`, `MetaDataVersionOID`,
`SubjectKey`, `StudyEventOID`, `StudyEventRepeatKey`, `FormOID`,
`FormRepeatKey`, `ItemGroupOID`, `ItemGroupRepeatKey`, `ItemOID`, and
`Value`, each of type `str`. Eight of them are the hierarchy fields:
`StudyOID`, `SubjectKey`, `StudyEventOID`, `StudyEventRepeatKey`,
`FormOID`, `FormRepeatKey`, `ItemGroupOID`, and `ItemGroupRepeatKey`.
Every field but `Value` is an identifying field.

<a id="req-1267"></a>

**REQ-1267.** A stored field binds to the schema field whose name it
equals under ASCII case folding, which maps `A` through `Z` to `a` through
`z` and leaves every other character unchanged. The binding does not
rename the stored bytes, which the storage profile reads as it always
does; the specification spells the schema's names. A stored field that
binds to no schema field is a vendor field. An ODM input does not expose a
vendor field, and an `odm` expression never reads one.

<a id="req-1268"></a>

**REQ-1268.** An ODM input is verified before any expression reads it.
Its fields are verified at validation, from the CSV header or the Parquet
schema, without reading a record: exactly one stored field binds to each
schema field, and each bound field is stored as text. Its records are
verified at ingest: every record carries `StudyOID`, `MetaDataVersionOID`,
`SubjectKey`, `StudyEventOID`, `FormOID`, `ItemGroupOID`, and `ItemOID`.
The repeat keys and `Value` may be missing. An ODM input declares no
`types` and no `schema`.

<a id="req-1269"></a>

**REQ-1269.** A row's ODM scope is the set of ODM input records the row
was built from:

| The row was built by | Its ODM scope |
| --- | --- |
| no `rows`, with the ODM input as the base | the records its key combination was derived from ([REQ-0042](../execution/rows.md#req-0042)) |
| a row template grouped over the ODM input | the records equal to the row on every hierarchy field in the template's `group_by` |
| a record-driven row template over the ODM input | the records equal to the driver record on all eight hierarchy fields |

Equality on a hierarchy field treats missing as equal to missing, as
[REQ-0037](../execution/rows.md#req-0037) does. `ItemOID`, `Value`, and
every field outside the hierarchy never narrow a scope, so a row built for
one item reads the other items of its own item group occurrence.

<a id="req-1270"></a>

**REQ-1270.** An `odm` expression evaluates for a row that has an ODM
scope over the input it names, in a column derivation or in a row
derivation. The scope belongs to the row: a column derivation reads, for
each row, the scope of the row template that built it. An `odm`
expression in a named intermediate, or one a row built from another
dataset would read, has no scope.

<a id="req-1271"></a>

**REQ-1271.** The read identifies the records of the row's scope whose
`ItemOID` is the named item, whose `StudyEventOID`, `FormOID`, and
`ItemGroupOID` are among the OIDs that `event`, `form`, and `item_group`
list, when they list any, and for which `filter` is `TRUE` under
[Predicates](../operations/predicates.md). `filter` names only schema
fields, qualified with the ODM input.

<a id="req-1272"></a>

**REQ-1272.** No identified record gives missing. One identified record
gives its `Value`, which may itself be missing. Two or more identified
records fail. Values take no part in identification: two records that
carry one value are still two. The read is a lookup, not a reduction;
nothing is counted, ordered, or chosen.

<a id="req-1273"></a>

**REQ-1273.** The result of an `odm` expression is `str`, and converts to
the column's declared type at completion under
[Types and conversion](../values/types.md), as any derivation result does.

### Interface behavior

<a id="req-1057"></a>

**REQ-1057.** The `variable` interface has these meanings.

| Field | Meaning |
| --- | --- |
| `variable` | Qualified source variable or unqualified constructed output column. |

<a id="req-1058"></a>

**REQ-1058.** The `regex` interface has these meanings.

| Field | Meaning |
| --- | --- |
| `regex` | Regular expression applied to a string value under [Text operations](../operations/text.md). |

<a id="req-1274"></a>

**REQ-1274.** The `expressions.odm` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `expressions.odm` | One collected item read from the row's ODM scope ([REQ-1269](binding.md#req-1269)). A bare string is the `item`. |
| `expressions.odm.item` | The ODM input and the complete `ItemOID`, as `DATASET.ItemOID` ([REQ-1265](binding.md#req-1265)). |
| `expressions.odm.event` | `StudyEventOID` values an identified record carries; one OID or a list. |
| `expressions.odm.form` | `FormOID` values an identified record carries; one OID or a list. |
| `expressions.odm.item_group` | `ItemGroupOID` values an identified record carries; one OID or a list. |
| `expressions.odm.filter` | Predicate over the ODM input's schema fields that an identified record satisfies. |
| `Result` | The `Value` of the one identified record, or missing when none is identified ([REQ-1272](binding.md#req-1272)). |

## Error conditions

<a id="req-0103"></a>

**REQ-0103.** An unknown dataset identifier or variable: fail.

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

<a id="req-1275"></a>

**REQ-1275.** An ODM input that lacks a schema field
(`odm_schema_field_missing`), binds two stored fields to one schema field
(`odm_schema_field_ambiguous`), stores a schema field as anything but text,
or declares `types` or `schema` (`odm_schema_field_type`): fail at
validation, naming the input and the fields.

<a id="req-1276"></a>

**REQ-1276.** An ODM input record that lacks a required identifier: fail
at ingest with `odm_schema_value_missing`, naming the field and the first
record that lacks it.

<a id="req-1277"></a>

**REQ-1277.** An `odm` expression in a named intermediate, or one a row
with no ODM scope over its input would read: fail at validation with
`invalid_odm_context`.

<a id="req-1278"></a>

**REQ-1278.** An `odm` read that identifies two or more records: fail with
`odm_not_unique`, reporting the row, the item, the number of records, and
each identifying field whose values differ among them, or reporting that
no identifying field tells them apart.
