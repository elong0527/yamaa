# SAS source profiles

The read-only `sas7bdat` and `xpt` profiles read SAS datasets and SAS
Transport files as input datasets. Each input declares the encoding of its
text, a `types` entry narrows a numeric field explicitly, and nothing is
guessed from the data, the display formats or the host.

## Containers and text

<a id="sas-1"></a>

### SAS-1. `.sas7bdat` and `.xpt` select the read-only SAS dataset and Transport profiles

`.sas7bdat` selects the read-only `sas7bdat` profile and `.xpt` selects the
read-only `xpt` profile; extensions are case insensitive. SAS datasets admit
uncompressed, RLE (`SASYZCRL`) and RDC (`SASYZCR2`) storage. Transport files
admit versions 5 and 8 and contain exactly one dataset member. These profiles
add no output format; publication remains CSV or Parquet.

Evidence: `negative-output-transport`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::rle_and_rdc_compression_equal_independent_upstream_csv_truth`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::reject_multiple_members_in_both_transport_versions`.
Replaces: REQ-1310.

A study document's submission section writes version 5 transport files from
published artifacts under [SAS Transport](../profiles/xpt.md); that is not an
artifact profile, and this reader reads those files like any other.

<a id="sas-2"></a>

### SAS-2. Every SAS or Transport input declares the encoding of its text

Each SAS or Transport input declares `dataset_class.encoding`: `utf-8`,
`windows-1252`, `iso-8859-1` or `ascii`. The declaration supplies the text
authority for character values and field names, including when a SAS dataset
embeds different encoding metadata; neither profile guesses from text or the
host locale. Encoding is absent on CSV and Parquet inputs. A missing SAS
encoding, or an encoding declaration on another profile, fails validation.

Fails: `source_encoding_required`.
Evidence: `rust/crates/yamaa-adapters/src/sas_source/native.rs::declared_decoders_reject_incomplete_or_undefined_text`, `python/tests/installed/installed_sas_sources.py::test_static_encoding_contract_is_shared_by_check_and_domain`.
Replaces: REQ-1311.

The failure reports `source_encoding_required` at `input.<dataset>.encoding`.

```yaml
input:
  DM: {path: input/dm.xpt, encoding: windows-1252}
  EX:
    path: input/ex.sas7bdat
    encoding: utf-8
    types: {EXSEQ: int, EXSTDT: date}
```

## Field types

<a id="sas-3"></a>

### SAS-3. Character storage supplies `str` and numeric storage `float`, and `types` may only narrow a numeric field

Character storage supplies `str`; numeric storage supplies `float`. SAS
display formats and labels do not select types. A `types` entry may retain
`str` on a character field, retain `float` on a numeric field, or narrow a
numeric field to `int`, `date` or `datetime`. Any other physical/type pair
fails ingestion.

Fails: `source_sas_invalid`, `unknown_field`.
Evidence: `rust/crates/yamaa-adapters/tests/table_sas_source.rs::sas_dataset_numeric_date_formats_never_infer_and_epoch_narrowing_is_explicit`, `rust/tests/host-fixtures/sas-cases.json::character-cannot-be-int`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::malformed_truncated_encoding_and_unknown_fields_fail_before_execution`.
Replaces: REQ-1312.

A disallowed physical/type pair fails with `source_sas_invalid`
([SAS-7](#sas-7)). A `types` entry naming an unknown field still
fails with `unknown_field`. A producing-specification link keeps its sole type
authority under [INPUT-6](../inputs.md#input-6).

<a id="sas-4"></a>

### SAS-4. Numeric narrowing accepts only an exact integral value, and SAS dates count from 1960

Ordinary numeric `float` fields use the reader's IBM-to-binary64 conversion,
including normal fractional values such as 0.1. Numeric narrowing accepts only
a finite stored exact integral value in signed 64-bit range; it never
truncates, rounds, saturates or recovers digits already rounded in a binary64
SAS value. A numeric `date` counts whole days from 1960-01-01, and a numeric
`datetime` whole seconds from 1960-01-01T00:00:00, without a timezone.

Fails: `field_parse_failed`.
Evidence: `rust/crates/yamaa-adapters/tests/table_sas_source.rs::ordinary_float_keeps_readstat_conversion_while_exact_narrowing_is_guarded`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::narrowing_never_truncates_or_accepts_an_ibm_precision_loss`, `rust/crates/yamaa-adapters/src/sas_source/native.rs::numeric_narrowing_has_exact_bounds_and_epoch`.
Replaces: REQ-1313, REQ-1314.

An IBM transport number whose stored precision cannot be represented exactly
in binary64 cannot narrow, even if decoding would produce an integral binary64
value. Non-integral, out-of-range or precision-losing narrowing fails with
`field_parse_failed`, reporting the dataset, field, declared type and decoded
numeric value. Negative counts precede the epoch. Fractions are rejected
before conversion, and dates and datetimes must remain within the complete
civil ranges [Values](../values.md) fixes.

## Values and order

<a id="sas-5"></a>

### SAS-5. SAS missing values become missing, and character values lose only trailing padding

Ordinary numeric missing and the tagged missing values `.A` through `.Z` and
`._` become the shared missing value, and non-finite numeric values normalize
to missing under [VALUE-4](../values.md#value-4). Character storage
loses right-hand ASCII space and NUL padding; leading and internal spaces
remain. The resulting zero-length character value follows `empty_string`.
Strings such as `.` and `NA` remain ordinary text, and embedded NUL bytes are
rejected.

Fails: `source_sas_invalid`.
Evidence: `rust/crates/yamaa-adapters/tests/table_sas_source.rs::all_numeric_missing_tags_map_to_missing`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::both_xport_versions_preserve_order_padding_unicode_and_default_float`, `rust/tests/host-fixtures/sas-cases.json::embedded-nul-is-refused`.
Replaces: REQ-1315.

Under an omitted or `missing` `empty_string`, a zero-length character value is
missing; under `present` it is the present empty string. An embedded NUL byte
fails with `source_sas_invalid` ([SAS-7](#sas-7)).

<a id="sas-6"></a>

### SAS-6. A SAS reader decodes only the captured snapshot, in stored field and record order

Readers decode only the captured immutable bytes obtained from the declared
source authority, and never reopen a path. Stored field order and record order
are preserved, and the generated `ordinal` applies before filtering under
[INPUT-3](../inputs.md#input-3). Source bytes, rows, fields,
cells, retained values, page and row work, and header work are admitted before
their corresponding allocations where the physical framing supplies those
counts.

Evidence: `rust/crates/yamaa-adapters/src/sas_source/native.rs::fixed_header_geometry_is_checked_before_retaining_any_columns_or_rows`, `rust/crates/yamaa-adapters/src/resource_diagnostics.rs::fixed_sas_boundary_keeps_its_bound_and_input_owner`, `python/tests/installed/installed_sas_sources.py::test_held_output_and_repeat_saves_survive_source_and_specification_mutation`.
Replaces: REQ-1316.

Resource refusal is separate from semantic source invalidity.

<a id="sas-7"></a>

### SAS-7. A SAS or Transport file that cannot be decoded fails ingestion with one closed cause

`source_sas_invalid` reports in the `ingest` phase at `input.<dataset>.path`,
with the dataset, the path and a closed `cause`: `framing`, `members`,
`version`, `encoding`, `compression`, `header`, `field_name`, `field_type`,
`record_order`, `deleted_records`, `truncated` or `malformed`. Undecodable text
fails without replacement characters. Unsupported versions, unsupported
compression signatures, malformed compressed data, duplicate or empty field
names and incomplete physical framing fail before derivation.

Fails: `source_sas_invalid`.
Evidence: `rust/crates/yamaa-core/tests/support/diagnostic/sas.rs`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::unsupported_compression_has_a_specific_retained_cause`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::records_deleted_in_place_are_refused_with_a_rewrite_hint`, `python/tests/installed/installed_sas_sources.py::test_bad_sources_are_not_read_by_check_and_failed_saves_have_no_output`.
Replaces: REQ-1317.

A SAS dataset that retains records deleted in place fails with cause
`deleted_records` when a data or mix page is flagged as holding deleted records
or the row-size subheader records a non-zero deleted-record count; a reader
never returns a deleted record. That finding's context adds a `hint` to rewrite
the dataset first, for example with PROC COPY or a DATA step. In compressed
storage a deleted record carries an unsupported compression code and fails as
`compression` first.

Transport member descriptor chains at 80-byte boundaries are structural
records; multiple complete chains fail, including concatenated transport
datasets. A character value resembling a MEMBER header alone remains text.
Transport framing contains no authoritative row count: removing a complete
terminal record that leaves a valid smaller dataset cannot be distinguished
from that smaller dataset. Partial physical records and non-padding partial
rows fail.
