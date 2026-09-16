---
id: R027
title: Parquet Source Profile
status: normative
applies_to: [dataset_source, dataset_class.path]
---

# Parquet source profile

## Intent

State what a Parquet source delivers so an artifact written by one conforming
runtime can be read by another without losing its types, field order, record
order, missing values, or collected empty strings.

## Boundaries

R023 selects this profile from the source path. R021 owns which file the path
may reach and the immutable byte snapshot read here. R014 owns the authority
of the embedded field types and a producing specification, and what the
delivered typed values mean. R011 owns non-finite normalization, R016 owns the
temporal values, and R019 owns valid text. R020 owns the Parquet artifact a
specification writes; this rule is its reading counterpart.

## Container and order

**R027-1.** A path selected as `parquet` is read as one Apache Parquet file.
Sniffing and fallback to another profile are not permitted.

**R027-2.** Fields are delivered in file-schema order. Records are delivered
in their stored row-group order and their order within each row group.
Parallel reads, batches, and projection must not reorder either sequence.

**R027-3.** An artifact with fields and no records is a valid empty source.
A file with no readable Parquet schema is not.

## Field schema

**R027-4.** Every schema field name is non-empty and unique under R019
equality. A field is scalar; nested, repeated, dictionary, and extension
types are outside this profile.

**R027-5.** The embedded Parquet type supplies the R014 field type through
this closed mapping, which is the inverse of R020-20:

| Parquet type | Column type |
|---|---|
| `BYTE_ARRAY` annotated `String` | `str` |
| `INT64` with no logical type | `int` |
| `DOUBLE` with no logical type | `float` |
| `INT32` annotated `Date` | `date` |
| `INT64` annotated `Timestamp`, microseconds, UTC-unadjusted | `datetime` |

**R027-6.** The mapping is exact. A different physical type, logical type,
timestamp unit, or timezone is not converted or inferred. Both nullable and
non-nullable fields are admitted; nullability states whether this particular
file can store a missing value and does not change the field's column type.

## Values

**R027-7.** A Parquet null is the missing value. A present zero-length string
is the collected empty string. The two remain distinct.

**R027-8.** A finite `DOUBLE` is delivered bit-identically. A non-finite
`DOUBLE` is normalized immediately to missing under R011.

**R027-9.** A `Date` count must name a date from `0001-01-01` through
`9999-12-31`. A `Timestamp` count must name a zone-free local datetime in the
same calendar range and must be an exact whole second. No timezone shift,
unit conversion, truncation, or rounding is permitted.

**R027-10.** Key-value metadata, statistics, page encodings, compression, and
row-group sizing do not alter delivered fields, records, types, or values and
are ignored. They cannot supply or override a field type.

## Rationale

Parquet is the production container because it preserves types and the
difference between null and empty text without a side declaration. Restricting
the source mapping to the types R020 writes makes the round trip closed and
portable: accepting a host-specific cast would let two runtimes give the same
file different field types or values. Storage choices that do not change the
delivered dataset remain free because R020 deliberately does not fix Parquet
bytes.

## Errors

- **R027-11.** Bytes that are not one readable Parquet file: fail with
  `source_parquet_invalid`.
- **R027-12.** An empty or duplicate field name: fail with
  `source_field_name_empty` or `source_field_name_duplicate`.
- **R027-13.** A field outside the closed type mapping: fail with
  `source_field_type_unsupported`, reporting the field and stored type.
- **R027-14.** A temporal value outside the calendar or whole-second contract:
  fail with `source_field_value_invalid`, reporting the field, record, and
  stored integer value.
