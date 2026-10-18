---
id: storage/sas
title: SAS source profiles
status: normative
---

# SAS source profiles

## Requirements

### Read-only containers

<a id="req-1310"></a>

**REQ-1310.** `.sas7bdat` selects the read-only `sas7bdat` profile and `.xpt`
selects the read-only `xpt` profile. Extensions are case insensitive. SAS
datasets admit uncompressed, RLE (`SASYZCRL`), and RDC (`SASYZCR2`) storage.
Transport files admit versions 5 and 8 and contain exactly one dataset member.
These profiles add no output format; publication remains CSV or Parquet.

### Declared text encoding

<a id="req-1311"></a>

**REQ-1311.** Each SAS or Transport input declares `dataset_class.encoding`:
`utf-8`, `windows-1252`, `iso-8859-1`, or `ascii`. The declaration supplies the
text authority for character values and field names, including when a SAS
dataset embeds different encoding metadata. Neither profile guesses from text
or the host locale. Encoding is absent on CSV and Parquet inputs. A missing SAS
encoding or an encoding declaration on another profile fails validation with
`source_encoding_required` at `input.<dataset>.encoding`.

```yaml
input:
  DM: {path: input/dm.xpt, encoding: windows-1252}
  EX:
    path: input/ex.sas7bdat
    encoding: utf-8
    types: {EXSEQ: int, EXSTDT: date}
```

### Physical types and explicit narrowing

<a id="req-1312"></a>

**REQ-1312.** Character storage supplies `str`; numeric storage supplies
`float`. SAS display formats and labels do not select types. A `types` entry
may retain `str` on a character field, retain `float` on a numeric field, or
narrow a numeric field to `int`, `date`, or `datetime`. Any other physical/type
pair fails ingestion. Unknown named fields retain `unknown_field`.
A producing-specification link retains its sole authority under REQ-0523.

<a id="req-1313"></a>

**REQ-1313.** Ordinary numeric `float` fields use the reader's IBM-to-binary64
conversion, including normal fractional values such as 0.1. Numeric narrowing accepts only a finite stored exact integral
value in signed-i64 range. It never truncates, rounds, saturates, or recovers
digits already rounded in a binary64 SAS value. An IBM transport number whose
stored precision cannot be represented exactly in binary64 cannot narrow,
even if decoding would produce an integral binary64 value. Non-integral,
out-of-range, or precision-losing narrowing fails with
`field_parse_failed`, reporting dataset, field, declared type, and the
decoded numeric value.

<a id="req-1314"></a>

**REQ-1314.** A numeric `date` counts whole days from 1960-01-01. A numeric
`datetime` counts whole seconds from 1960-01-01T00:00:00, without a timezone.
Negative values precede the epoch. Fractions are rejected before conversion;
dates and datetimes must remain within the complete civil ranges fixed by
[Temporal values](../values/temporal.md).

### Missing and character storage

<a id="req-1315"></a>

**REQ-1315.** Ordinary numeric missing and tagged missing values `.A` through
`.Z` and `._` become the shared missing value. Non-finite numeric values
normalize to missing under REQ-0006. Character storage loses right-hand ASCII
space and NUL padding; leading spaces and internal spaces remain. The resulting
zero-length character value follows `empty_string`: omitted or `missing`
produces missing; `present` produces the present empty string. Strings such as
`.` and `NA` remain ordinary text. Embedded NUL bytes are rejected.

### Snapshot and order

<a id="req-1316"></a>

**REQ-1316.** Readers decode only the captured immutable bytes obtained from
the declared source authority. Native readers do not reopen a path. Stored
field order and record order are preserved, and the generated `ordinal`
applies before filtering under REQ-1292. Source bytes, rows, fields, cells,
retained values, page/row work, and header work are admitted before their
corresponding allocations where the physical framing supplies those counts.
Resource refusal is separate from semantic source invalidity.

## Error conditions

<a id="req-1317"></a>

**REQ-1317.** `source_sas_invalid` reports in the `ingest` phase at
`input.<dataset>.path`, with dataset, path, and a closed `cause`:
`framing`, `members`, `version`, `encoding`, `compression`, `header`,
`field_name`, `field_type`, `record_order`, `deleted_records`, `truncated`, or
`malformed`. Undecodable text fails without replacement characters.
Unsupported versions, unsupported compression signatures, malformed compressed
data, duplicate or empty field names, and incomplete physical framing fail
before derivation. A SAS dataset that retains records deleted in place fails
with cause `deleted_records` when a data or mix page is flagged as holding
deleted records or the row-size subheader records a non-zero deleted-record
count; readers never return a deleted record. That finding's context adds a `hint` to
rewrite the dataset first, for example with PROC COPY or a DATA step. In
compressed storage a deleted record carries an unsupported compression code
and fails as `compression` first.
Transport member descriptor chains at 80-byte boundaries are structural
records; multiple complete chains fail, including concatenated transport
datasets. A character value resembling a MEMBER header alone remains text.
Transport framing contains no authoritative row count; removing a complete
terminal record that leaves a valid smaller dataset cannot be distinguished
from that smaller dataset. Partial physical records and non-padding partial
rows fail.
