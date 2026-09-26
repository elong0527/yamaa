---
id: storage/parquet
title: Parquet profile
status: normative
---

# Parquet profile

## Purpose

Read and write the closed Parquet field and value mapping.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Source ingestion](ingestion.md).
- [Artifact publication](publication.md).
- [Temporal values](../values/temporal.md).
- [Text values](../values/text.md).
- [Types and conversion](../values/types.md).

## Requirements

### Writing: The parquet profile

<a id="req-0734"></a>

**REQ-0734.** Each declared type maps to exactly one Parquet physical/logical
type.

| Column type | Physical | Logical |
|---|---|---|
| `str` | `BYTE_ARRAY` | `String` |
| `int` | `INT64` | none |
| `float` | `DOUBLE` | none |
| `date` | `INT32` | `Date` |
| `datetime` | `INT64` | `Timestamp`, microseconds, not adjusted to UTC |

<a id="req-0735"></a>

**REQ-0735.** The schema's fields are the names in `output.columns`, in that
order. Every field is optional. Every column type admits a missing
value.

#### Missing and the empty string

<a id="req-0736"></a>

**REQ-0736.** Retired. The missing-vs-empty distinction is stated once in
[REQ-1034](#req-1034): a Parquet null is the missing value and a present
zero-length string is the collected empty string, in both directions. This
identifier is never reused.

#### Temporal values

<a id="req-0737"></a>

**REQ-0737.** A `date` is days from 1970-01-01, and a `datetime` is the
count of microseconds from 1970-01-01T00:00:00 on the same wall clock the value
names.

<a id="req-0738"></a>

**REQ-0738.** [Temporal values](../values/temporal.md)'s `datetime` is a reading on a wall clock and carries no zone
and no offset. Its Timestamp is not adjusted to UTC. An implementation must
not attach a zone when writing or reading. A runtime whose
native timestamp always carries a zone -- [Temporal values](../values/temporal.md) names R's `POSIXct` as such a
type -- must still write and read this column. The same wall clock
survives. Shifting a value into or out of a machine timezone changes the
value. Two runtimes that each shift by their own offset do not agree.

<a id="req-0739"></a>

**REQ-0739.** A `datetime` has whole-second resolution. It is written with the
microsecond unit, and its microsecond part is always zero.

#### Determinism

<a id="req-0740"></a>

**REQ-0740.** Two runtimes writing the same completed dataset must produce
Parquet artifacts that read back identically: the same field names in the same
order, the same logical types, the same rows in the same order, the same nulls,
and the same values, with every `DOUBLE` bit-identical.

<a id="req-0741"></a>

**REQ-0741.** An implementation writes uncompressed pages and adds no key-value
metadata of its own beyond what the format requires.

<a id="req-0742"></a>

**REQ-0742.** The bytes are not fixed. A Parquet writer stamps its own
identity and version into the file, and the row-group and page sizing, the
encodings it selects, and the statistics it records are properties of the
library rather than of this design. Requiring identical bytes would require
every conforming implementation to abandon its ecosystem's writer, which buys
less than it costs. An artifact needing direct byte comparison is written
under `csv`, whose byte guarantee is exactly that.

#### Floats are stored, not rendered

<a id="req-0743"></a>

**REQ-0743.** A `float` enters this profile as the binary64 value its derivation
produced. `output.decimals` does not apply. No rounding happens on output, so a
consumer that reads the artifact receives the value the calculation used.
Storing native double values is not display. This design rounds once, at
display.

### Reading: Container and order

<a id="req-1028"></a>

**REQ-1028.** A path selected as `parquet` is read as one Apache Parquet file.
Do not sniff the path or fall back to another profile.

<a id="req-1029"></a>

**REQ-1029.** Fields are delivered in file-schema order. Records are delivered
in their stored row-group order and their order within each row group.
Parallel reads, batches, and projection must not reorder either sequence.

<a id="req-1030"></a>

**REQ-1030.** An artifact with fields and no records is a valid empty source.
A file with no readable Parquet schema is not.

### Reading: Field schema

<a id="req-1031"></a>

**REQ-1031.** Every schema field name is non-empty and unique under [Text values](../values/text.md)
equality. A field is scalar; nested, repeated, dictionary, and extension
types are outside this profile.

<a id="req-1032"></a>

**REQ-1032.** The embedded Parquet type supplies the [Source ingestion](ingestion.md) field type through
this closed mapping, which is the inverse of [REQ-0734](parquet.md#req-0734):

| Parquet type | Column type |
|---|---|
| `BYTE_ARRAY` annotated `String` | `str` |

The two Arrow string widths are the same physical/logical pair: a
`BYTE_ARRAY` field annotated `String` reads as `str` whether the file's
embedded Arrow schema names it `string` or `large_string` (the offset width
differs; nothing else does).
| `INT64` with no logical type | `int` |
| `DOUBLE` with no logical type | `float` |
| `INT32` annotated `Date` | `date` |
| `INT64` annotated `Timestamp`, microseconds, no UTC adjustment | `datetime` |

<a id="req-1033"></a>

**REQ-1033.** The mapping is exact. A different physical type, logical type,
timestamp unit, or timezone is not converted or inferred. Both nullable and
non-nullable fields are admitted; nullability states whether this particular
file can store a missing value and does not change the field's column type.

### Reading: Values

<a id="req-1034"></a>

**REQ-1034.** A Parquet null is the missing value. A present zero-length string
is the collected empty string. The two remain distinct.

<a id="req-1035"></a>

**REQ-1035.** A finite `DOUBLE` is delivered bit-identically. A non-finite
`DOUBLE` is normalized immediately to missing under [Types and conversion](../values/types.md).

<a id="req-1036"></a>

**REQ-1036.** A `Date` count must name a date from `0001-01-01` through
`9999-12-31`. A `Timestamp` count must name a zone-free local datetime in the
same calendar range and must be an exact whole second. No timezone shift,
unit conversion, truncation, or rounding is permitted.

<a id="req-1037"></a>

**REQ-1037.** Key-value metadata, statistics, page encodings, compression, and
row-group sizing do not alter delivered fields, records, types, or values and
are ignored. They cannot supply or override a field type.

## Error conditions

### Reading: Errors

<a id="req-1038"></a>

**REQ-1038.** Bytes that are not one readable Parquet file: fail with
  `source_parquet_invalid`.

<a id="req-1039"></a>

**REQ-1039.** An empty or duplicate field name: fail with
  `source_field_name_empty` or `source_field_name_duplicate`.

<a id="req-1040"></a>

**REQ-1040.** A field outside the closed type mapping: fail with
  `source_field_type_unsupported`, reporting the field and stored type.

<a id="req-1041"></a>

**REQ-1041.** A temporal value outside the calendar or whole-second contract:
  fail with `source_field_value_invalid`, reporting the field, record, and
  stored integer value.

## Conformance examples

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Read and write the closed Parquet field and value mapping. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
