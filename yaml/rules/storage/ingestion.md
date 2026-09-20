---
id: storage/ingestion
title: Source ingestion
status: normative
---

# Source ingestion

## Purpose

Select input profiles and assign source field types without inferring values.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Local handlers](../execution/handlers.md).
- [Execution lifecycle](../execution/lifecycle.md).
- [Aggregation](../operations/aggregation.md).
- [Expression evaluation](../operations/expressions.md).
- [Schema language](../reference/schema-language.md).
- [Execution lifecycle](../execution/lifecycle.md).
- [CSV profile](csv.md).
- [Parquet profile](parquet.md).
- [Artifact publication](publication.md).
- [Resource resolution](resources.md).
- [Temporal values](../values/temporal.md).
- [Types and conversion](../values/types.md).

## Requirements

### Source record order

<a id="req-0514"></a>

**REQ-0514.** Stored record order is part of the input contract. A reader must
return records in stored order after field decoding and typing. Parallel reads,
batches, partitions, or an engine's scan plan must not reorder records.
Filtering preserves the relative order of the records that remain. [Execution lifecycle](../execution/lifecycle.md) uses
this sequence as base-record and grouped-input order. [Ordering](../execution/ordering.md) uses the sequence to
break window ties. [Aggregation](../operations/aggregation.md) consumes the sequence for ordered floating-point
reduction.

<a id="req-0515"></a>

**REQ-0515.** Record order is not a substitute for a business key or a declared
sort. Record order is the stable sequence of one artifact. Replacing an
artifact with the same records in another order changes the input and can
alter an order-sensitive result.

### A field's type belongs to the dataset

<a id="req-0516"></a>

**REQ-0516.** Every field of a source dataset has exactly one type, drawn from
`column_type`, and every value bound from that field carries that type. A
field's type is a property of the dataset, not of the values one extract holds.
Two extracts of the same dataset bind the same field to the same type.

<a id="req-0517"></a>

**REQ-0517.** Where the type comes from depends on the container:

- A **self-describing source** supplies it. An ODM `ItemDef` data type, a
  Parquet schema, and an artifact's producing specification are each the
  field's type authority. The consuming specification never restates the type.
- A **typeless container**, such as a delimited text file, supplies none.
  Every one of its fields is `str` unless the specification declares
  otherwise.

<a id="req-0518"></a>

**REQ-0518.** `dataset_class.types` declares each named field's type in a
typeless container. Any unnamed field is `str`. This declaration covers the
dataset as this specification reads it. Two specifications may read
the same delimited file with different declarations, because the file carries
no types to contradict either declaration. A dataset whose types matter to more
than one specification belongs in a container that carries the types.

```yaml
input:
  EX: {path: input/ex.csv, types: {EXDOSE: float, EXSEQ: int}}
  DM: input/dm.csv
```

<a id="req-0519"></a>

**REQ-0519.** Both forms are the same declaration: a bare path is [Schema language](../reference/schema-language.md) shorthand
for a `dataset_class` with no `types`.

### Producing-specification link

<a id="req-0520"></a>

**REQ-0520.** `dataset_class.schema` makes a stored artifact carry the output
contract and workflow provenance of the yamaa specification that produces it.
`schema` is a `project_path` resolved like `dataset_class.path`, so [Resource resolution](resources.md)
confines both. The referenced document is a complete specification validated
against the same `root_class` in `schema.yaml`. There is no second
source-schema class or field-description language.

```yaml
input:
  DM:
    path: input/dm.csv
    schema: input/dm.schema.yaml
```

<a id="req-0521"></a>

**REQ-0521.** The referenced specification is an executable workflow predecessor.
Its sources must exist and validate. Its derivations
must be complete. Its `schema` links are validated recursively. The dependency
graph must be acyclic. The producer completes before the consumer reads the
artifact named by `path`. A link cannot name the consuming specification or one
already above it in the workflow.

<a id="req-0522"></a>

**REQ-0522.** The producer's `output.columns` names every stored field exactly
once and in artifact order. Each selected entry in the producer's `columns`
supplies that field's [Types and conversion](../values/types.md) type and label. Declared internal columns omitted
from `output.columns` are not stored fields. A delimited artifact's header and
a Parquet artifact's schema fields must equal `output.columns`. Missing, extra,
reordered, duplicate, or undeclared fields fail. A Parquet field's embedded
type must additionally equal the producer's declared type.

<a id="req-0523"></a>

**REQ-0523.** The producing specification remains the only type authority.
`types` may be present only for a typeless source, so `types` must be absent
whenever `schema` is present. The absence rejects even an inline entry that
agrees with the producer instead of creating two authorities for one type.

<a id="req-0524"></a>

**REQ-0524.** A stored cell in a delimited artifact is still text. After
recognizing missing values, ingestion applies the `str` row of [Types and conversion](../values/types.md)'s
conversion table to every non-missing cell. A producer column declared `date`
or `datetime` uses [Temporal values](../values/temporal.md)'s lexical grammar and representations, exactly as an
inline `types` declaration or a column conversion does. A Parquet artifact
instead supplies typed values under [Parquet profile](parquet.md). A workflow link does not convert
those values through text.

### Values are never inferred

<a id="req-0525"></a>

**REQ-0525.** An implementation must not infer a field's type from its values. A
declared type states what the study collects. A non-matching value is a defect
in the data, not a reason to retype the field.

### Parsing a declared type

<a id="req-0526"></a>

**REQ-0526.** For a typeless container, [Types and conversion](../values/types.md)'s `str` row parses stored text into
its declared type. `int` and `float` use [Types and conversion](../values/types.md)'s numeric text parsing,
including its non-finite normalization. `date` and `datetime` accept exactly
the lexical forms [Temporal values](../values/temporal.md) fixes. A value that does not parse fails the run. [Types and conversion](../values/types.md)
separately recognizes YAML 1.2 non-finite forms during declared numeric
parsing. They remain text when the field's type is `str` and normalize only
after parsing as numbers.

<a id="req-0527"></a>

**REQ-0527.** An ingestion failure is not a conversion failure.
`missing` is declared on a result wrapper and answers for a value the
derivation produced, as [Execution lifecycle](../execution/lifecycle.md) and [Local handlers](../execution/handlers.md) define. A stored value that does not
match its field's declared type is rejected before any derivation runs. No
handler answers for the rejected value. A specification that wants to see such
a value declares the field `str` and converts it at the consuming column,
where a handler exists.

### Missing values

<a id="req-0528"></a>

**REQ-0528.** A missing value is the absence of a value. It is recognized before
typing. Every type admits it. [Local handlers](../execution/handlers.md)'s handlers answer for it.

<a id="req-0529"></a>

**REQ-0529.** In a delimited source, a field with no characters is missing,
whether the field was bare or quoted. No type admits an empty string from a
delimited source. No collected-empty value is distinct from a missing one.

<a id="req-0530"></a>

**REQ-0530.** No text is a missing-value sentinel. `NA`, `NULL`, `.`, `unknown`,
and every other spelling are ordinary string values.

<a id="req-0531"></a>

**REQ-0531.** An empty field of any declared type is missing rather than a parse
failure, because it holds no text to parse.

### Empty-string convention

<a id="req-1158"></a>

**REQ-1158.** Each input dataset declares its empty-string convention as
`dataset_class.empty_string`: `missing` or `present`. An omitted declaration
means `missing`.

<a id="req-1159"></a>

**REQ-1159.** Under `missing`, a stored zero-length string in a `str` field is
the missing value: it is recognized before typing, per REQ-0528, and no
derivation ever sees it as a value. Under `present`, the same stored value is
the collected empty string, a present `str` value that handlers answer for
like any other string. The container profiles keep their own distinction -- a
Parquet null and a zero-length string remain distinct under REQ-1034 -- and
the convention applies at ingestion, after the profile decodes.

<a id="req-1160"></a>

**REQ-1160.** The convention applies to `str` fields only. An empty field of
any other declared type is missing under REQ-0529 and REQ-0531; a typed
container admits no zero-length string outside `str`.

<a id="req-1161"></a>

**REQ-1161.** A delimited source must not declare `present`: the delimited
profile admits no empty string (REQ-0529), so the declaration names a behavior
the container cannot honor. The declaration fails validation.

### A stored artifact carries its profile

<a id="req-0751"></a>

**REQ-0751.** A specification that reads an artifact another specification
produced learns how those bytes are encoded from the producer, through the
producing specification link [Source ingestion](ingestion.md) defines: the producer's `output.path` states
the profile by its extension, just as `output.columns` states the fields.
The consumer reads the profile from the producing specification, not from
the name the consumer happens to know the file by. A copy stored under
another name is still read under the profile its producer wrote it with.

### The path's extension selects the profile

<a id="req-0830"></a>

**REQ-0830.** `dataset_class.path` names the file a specification reads, and its
extension selects the profile that reads it. The mapping is closed, so an
extension outside it names no profile and fails validation rather than
falling back to one. The extension is matched without regard to case, because
a study that stores `DM.CSV` names the same container as one that stores
`dm.csv`.

| Extension | Profile | Container | Source-profile owner |
|---|---|---|---|
| `.csv` | `csv` | delimited text | this contract |
| `.parquet` | `parquet` | Apache Parquet | [Parquet profile](parquet.md) |

<a id="req-0831"></a>

**REQ-0831.** A source profile follows the artifact profile: a file [Artifact publication](publication.md) writes
and this contract reads has one profile name in both directions. A second field
could disagree with the source path, misname the contents, and leave the
reader unable to check the claim.

<a id="req-0832"></a>

**REQ-0832.** Sniffing is not permitted. A reader that inspected a file's
contents to choose a delimiter or a quote character could misread a
conforming source without failing. A reader that accepted an unknown
extension under a default could read a container this profile does not
describe at all.

### Interface behavior

<a id="req-1059"></a>

**REQ-1059.** The `dataset_source` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `dataset_source` | Path to the dataset, or a path with its producing specification or field types. |

<a id="req-1060"></a>

**REQ-1060.** The `dataset_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `dataset_class.path` | Source data path, relative to the specification or rooted at an approved data root and confined by [Resource resolution](resources.md); [CSV profile](csv.md) selects the source profile from its extension. |
| `dataset_class.types` | Type each named field carries; [Source ingestion](ingestion.md) types the rest. |
| `dataset_class.schema` | Producing specification whose output contract supplies the source fields and types; [Source ingestion](ingestion.md) defines the workflow edge. |

## Error conditions

<a id="req-0532"></a>

**REQ-0532.** A `types` entry naming a field the dataset does not have:
  fail.

<a id="req-0533"></a>

**REQ-0533.** A `types` entry for a field whose container or producing
  specification already supplies a type: fail, rather than override the
  source contract. This includes every field of a Parquet source.

<a id="req-0534"></a>

**REQ-0534.** A document named by `schema` that does not validate as a
  complete yamaa specification, or whose producer dependency creates a cycle:
  fail.

<a id="req-0535"></a>

**REQ-0535.** A stored artifact whose field names, order, or self-describing
  types do not exactly match the producing specification's output contract:
  fail.

<a id="req-0536"></a>

**REQ-0536.** A stored text value that does not parse under its field's
  declared type: fail, reporting the dataset, field, and value.

<a id="req-0537"></a>

**REQ-0537.** Inferring a field type from its values, or treating text as
  absence: neither is an implementation option.

<a id="req-0538"></a>

**REQ-0538.** Reordering source records while reading, decoding, typing,
  filtering, or assembling parallel batches: fail.

<a id="req-0852"></a>

**REQ-0852.** `source_profile_unknown` is decided from the written path before
any byte is read and reports under the `validation` phase. Every other
condition is decided while the snapshot is read and reports under the
`ingest` phase.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-redefined-date](../../../benchmark/negative-redefined-date/README.md).
- [negative-ingest-unit](../../../benchmark/negative-ingest-unit/README.md).
- [negative-source-na-age](../../../benchmark/negative-source-na-age/README.md).
- [negative-source-unknown-format](../../../benchmark/negative-source-unknown-format/README.md).

The [execution manifest](../../../benchmark/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Select input profiles and assign source field types without inferring values. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
