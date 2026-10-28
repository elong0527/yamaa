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

Evidence: `negative-output-transport`, `schema-source-xpt`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::rle_and_rdc_compression_equal_independent_upstream_csv_truth`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::reject_multiple_members_in_both_transport_versions`.

A study document's submission section writes version 5 transport files from
published artifacts under [SAS Transport](../profiles/xpt.md); that is not an
artifact profile, and this reader reads those files like any other.

<a id="sas-2"></a>

### SAS-2. A SAS input uses its stored encoding, and a Transport input declares its encoding

A SAS dataset uses its stored text encoding when `dataset_class.encoding` is
omitted. A declaration, when present, must agree with that stored encoding.
Transport stores no encoding; its input declares one. Both profiles admit `utf-8`, `windows-1252`, `iso-8859-1` or `ascii` for character
values and field names; neither guesses from text or the host locale.
Encoding is absent on CSV and Parquet inputs.

Fails: `source_encoding_required`, `source_sas_invalid`.
Evidence: `schema-source-xpt`, `schema-source-sas7bdat-encoding`, `negative-sas7bdat-encoding-mismatch`, `rust/crates/yamaa-adapters/src/sas_source/native.rs::declared_decoders_reject_incomplete_or_undefined_text`, `python/tests/installed/installed_sas_sources.py::test_static_encoding_contract_is_shared_by_check_and_run`.

A missing Transport encoding, or an encoding declaration on another profile,
fails validation as `source_encoding_required` at `input.<dataset>.encoding`.
A contradictory SAS declaration fails ingestion as `source_sas_invalid`,
cause `encoding_mismatch`, at that encoding entry, with `declared_encoding`
and `stored_encoding` in its context. An absent or unsupported stored SAS
encoding fails ingestion with cause `encoding` ([SAS-7](#sas-7)).

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
Evidence: `schema-source-xpt`, `negative-xpt-character-as-int`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::sas_dataset_numeric_date_formats_never_infer_and_epoch_narrowing_is_explicit`, `rust/tests/host-fixtures/sas-cases.json::character-cannot-be-int`, `rust/tests/host-fixtures/sas-cases.json::numeric-cannot-be-str`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::malformed_truncated_encoding_and_unknown_fields_fail_before_execution`.

A disallowed physical/type pair fails with `source_sas_invalid`, cause
`field_type`, at `input.<dataset>.types.<field>`, naming the field
([SAS-7](#sas-7)). A `types` entry naming an unknown field still
fails with `unknown_field` at the same entry. A producing-specification link keeps its sole type
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
Evidence: `schema-source-xpt`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::ordinary_float_keeps_readstat_conversion_while_exact_narrowing_is_guarded`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::narrowing_never_truncates_or_accepts_an_ibm_precision_loss`, `rust/crates/yamaa-adapters/src/sas_source/native.rs::numeric_narrowing_has_exact_bounds_and_epoch`.

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
Evidence: `schema-source-xpt`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::all_numeric_missing_tags_map_to_missing`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::both_xport_versions_preserve_order_padding_unicode_and_default_float`, `rust/tests/host-fixtures/sas-cases.json::embedded-nul-is-refused`.

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

Evidence: `rust/crates/yamaa-adapters/src/sas_source/native.rs::fixed_header_geometry_is_checked_before_retaining_any_columns_or_rows`, `rust/crates/yamaa-adapters/src/resource_diagnostics.rs::fixed_sas_boundary_keeps_its_bound_and_input_owner`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::references_bouncing_between_pages_cannot_read_more_than_the_file_holds`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::the_row_limit_admits_live_records_not_stored_ones`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::decompressed_records_are_admitted_as_source_bytes_before_any_is_decompressed`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::a_small_file_cannot_decompress_to_more_than_the_source_byte_limit`, `python/tests/installed/installed_sas_sources.py::test_held_output_and_repeat_saves_survive_source_and_specification_mutation`.

Resource refusal is separate from semantic source invalidity.

The rows of a `sas7bdat` input are its live records ([SAS-8](#sas-8)), so
admission counts them, not the records its header counts as deleted.

The stored bytes that moved-record references read ([SAS-8](#sas-8)) are page
work. Together they may not exceed the bytes of the file's pages, so following
references costs no more than a multiple of the file's size. A file whose
references would read more fails as `malformed` ([SAS-7](#sas-7)).

Each live record of a compressed `sas7bdat` input decompresses to the row
length, and no record after the live records is decompressed. The live records
times the row length are source bytes: they are admitted under the source byte
limit before any record is decompressed, and a file over it is refused as that
limit.

<a id="sas-7"></a>

### SAS-7. A SAS or Transport file that cannot be decoded fails ingestion with one closed cause

`source_sas_invalid` reports in the `ingest` phase at `input.<dataset>.path`,
at the `encoding` entry for cause `encoding_mismatch`, or at the `types`
entry for cause `field_type`, with the dataset, the path and
a closed `cause`: `framing`, `members`, `version`, `encoding`, `compression`,
`header`, `field_name`, `field_type`, `encoding_mismatch`, `record_order`,
`deleted_records`, `truncated` or `malformed`. A cause that one field locates names the `field`,
and a value's also its `record`. Undecodable text fails without replacement
characters. Unsupported versions, unsupported compression signatures,
malformed compressed data, duplicate or empty field names and incomplete
physical framing fail before derivation.

Fails: `source_sas_invalid`.
Evidence: `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_sas_transport_file_that_breaks_its_framing_is_invalid`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_malformed_sas_dataset_is_invalid`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_sas_dataset_with_records_deleted_in_place_is_invalid_and_says_how_to_rewrite_it`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_repeated_sas_field_name_names_the_name`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::an_empty_sas_field_name_names_its_position`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_sas_field_name_that_does_not_decode_names_its_position`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_types_entry_a_sas_field_cannot_take_is_refused_at_the_entry`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_sas_value_that_does_not_decode_names_its_field_and_record`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::a_refusal_a_field_locates_names_the_field_and_a_value_also_its_record`, `rust/tests/host-fixtures/sas-cases.json::cp1252-is-not-utf8`, `negative-xpt-character-as-int`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::unsupported_compression_has_a_specific_retained_cause`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::counted_deletions_without_marks_are_refused_with_a_rewrite_hint`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::version_8_observation_count_keeps_trailing_blank_records`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::transport_without_a_usable_count_treats_trailing_blank_records_as_padding`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::trailing_blank_records_of_any_width_are_padding_only_without_a_count`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::a_row_size_subheader_cannot_change_the_row_length_once_columns_are_submitted`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::compressed_records_after_a_changed_row_length_are_refused_before_they_are_written`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::column_subheaders_shorter_than_their_fixed_part_are_refused`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::a_short_attributes_subheader_cannot_rewrite_a_column_after_the_first_record`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::stored_record_counts_beyond_32_bits_are_refused`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::version_8_observation_count_must_be_decimal_digits_between_blanks`, `python/tests/installed/installed_sas_sources.py::test_bad_sources_are_not_read_by_check_and_failed_saves_have_no_output`.

| Cause | Context beyond the dataset, path and cause |
|---|---|
| `field_name` | `field`: an empty name's position, or the repeated name |
| `encoding_mismatch` | `declared_encoding` and `stored_encoding`, at `input.<dataset>.encoding` ([SAS-2](#sas-2)) |
| `field_type` | `field`, at `input.<dataset>.types.<field>` ([SAS-3](#sas-3)) |
| `encoding` | for a field name that does not decode, `field`, its position; for a value that does not decode or holds a NUL byte ([SAS-5](#sas-5)), `field` and `record` |
| `deleted_records` | `hint` |
| any other | none |

A field's position counts the stored fields from one, and a value's `record`
counts from one the records the input yields ([SAS-8](#sas-8)), as its
generated ordinal does ([INPUT-3](../inputs.md#input-3)). A NUL byte in a
field's name, label or format, or in the dataset's own name or label, fails as
`encoding` and names no field.

Cause `deleted_records` reports records deleted in place that cannot be
reconciled with the dataset's count of them ([SAS-8](#sas-8)). That finding's
context adds a `hint` to rewrite the dataset first, for example with PROC COPY
or a DATA step.

A `sas7bdat` input has one row length. A row-size subheader that follows the
first record or data page may repeat it, and one that changes it fails as
`malformed`. A column-name or column-attributes subheader shorter than its
fixed part, 28 bytes in a 64-bit file and 20 otherwise, fails as `malformed`.
So does a stored record count above 4,294,967,295.

Transport member descriptor chains at 80-byte boundaries are structural
records; multiple complete chains fail, including concatenated transport
datasets. A character value resembling a MEMBER header alone remains text.

A version 8 file whose `OBSV8` header record counts its observations, in the
fifteen characters from byte 48, yields exactly that many records, including
trailing records made entirely of blanks. The count is decimal digits, which
blanks may precede or follow; a sign or any other character fails as
`malformed`. The count is admitted under [SAS-6](#sas-6) before any record is
read. Fewer complete records than counted, or anything but blank padding after
them, fails as `malformed`.

Version 5 framing, and a version 8 count of zero, carry no authoritative row
count. Trailing records made entirely of blanks, whatever their width, may be
padding and are not data, and removing a complete terminal record that leaves
a valid smaller dataset cannot be distinguished from that smaller dataset.
Partial physical records and non-padding partial rows fail.

<a id="sas-8"></a>

### SAS-8. A SAS dataset yields only its live records, in stored order

A `sas7bdat` input yields its live records in stored order. A record that SAS
deleted in place, marked in its page's deleted-record bitmap or by a deletion
code on its compressed row, is skipped. A compressed record that SAS moved to
another page is read where the reference that replaced it stands. The number
of records found marked deleted must equal the deleted count in the dataset's
header; otherwise ingestion fails with cause `deleted_records`.

Fails: `source_sas_invalid`.
Evidence: `rust/crates/yamaa-adapters/tests/table_sas_source.rs::records_deleted_in_place_are_skipped_in_the_independent_haven_fixture`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::deleted_records_after_the_last_live_record_are_found_on_later_pages`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::deleted_record_counts_that_disagree_with_their_marks_are_refused`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::marks_on_pages_after_the_counts_are_met_are_not_read`, `rust/tests/host-fixtures/sas-cases.json::deleted-record-skipped`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::moved_records_are_read_at_their_reference`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::a_moved_record_stored_before_its_reference_is_skipped_and_read_at_the_reference`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::references_that_reach_each_moved_record_once_read_across_pages`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::references_that_do_not_reach_a_moved_record_are_refused`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::a_reference_with_a_zero_page_or_subheader_index_is_refused_not_skipped`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::references_bouncing_between_pages_cannot_read_more_than_the_file_holds`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::moved_records_deleted_later_count_as_deleted`.

SAS keeps a record deleted in place, for example by `PROC SQL DELETE` or a DATA
step `MODIFY` with `REMOVE`, until the dataset is rewritten. The row-size
subheader counts the stored records, deleted ones included, and counts the
deleted ones separately. A data or mix page that holds deleted records carries
the page-type flag `0x0080` and, after its rows and free space, a bitmap with
one bit per stored record, most significant bit first. A compressed row's
subheader pointer marks a deleted record with code `0x05`, and a moved record
deleted later with code `0x09` or `0x0d`.

A reader reads past the last live record until it has found as many marks as
the count says. It stops after the page on which it has found every live
record and that many marks, but reads every page of a dataset with no live
records. A mark beyond the count fails when the reader reaches it, and a mark
on a page it does not reach is not read. The check compares counts: it does
not tell which records the header meant to delete. A deleted count above the
stored count, a mark beyond the count, a counted deletion never found, a
bitmap that does not fit its page, or a page row beyond the records its bitmap
covers fails with cause `deleted_records` ([SAS-7](#sas-7)). The bitmap covers
the page's records, and no more than the dataset stores.

In compressed storage SAS can move an updated record that no longer fits its
slot to another page. The slot then holds a reference, code `0x03`, whose
offset and length fields hold the one-based page and subheader indices of the
moved record. That record carries code `0x02` when stored uncompressed and
`0x06` when compressed, and is read only through its reference. A reference
that does not reach such a record on a page with subheaders, including one
whose page or subheader index is zero, fails as `malformed`, or as
`compression` when it reaches a subheader with another compression code. The
stored bytes that all references read may not exceed the bytes of the file's
pages, which [SAS-6](#sas-6) counts as page work; a file whose references would
read more fails as `malformed`.
