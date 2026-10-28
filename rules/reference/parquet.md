# Parquet profile

The `parquet` profile stores an artifact's columns as typed Parquet fields and
reads a Parquet source through the same closed mapping in reverse. A parquet
artifact is fixed by what it reads back as, not by its bytes.

## Writing

<a id="parquet-1"></a>

### PARQUET-1. Each column type maps to exactly one Parquet physical and logical type

A parquet artifact's schema fields are the names in `output.columns`, in that
order. Each declared column type maps to exactly one Parquet physical and
logical type, as the table below fixes. Every field is optional, and every
column type admits a missing value.

Evidence: `schema-parquet`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::closed_schema_and_exact_values_include_full_integers_temporals_nulls_and_zero_sign`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::ordered_row_groups_and_projection_survive_an_empty_source`.

| Column type | Physical | Logical |
|---|---|---|
| `str` | `BYTE_ARRAY` | `String` |
| `int` | `INT64` | none |
| `float` | `DOUBLE` | none |
| `date` | `INT32` | `Date` |
| `datetime` | `INT64` | `Timestamp`, microseconds, not adjusted to UTC |

<a id="parquet-2"></a>

### PARQUET-2. A date counts days and a datetime whole-second microseconds from 1970-01-01, with no zone

A `date` is the count of days from 1970-01-01, and a `datetime` is the count
of microseconds from 1970-01-01T00:00:00 on the same wall clock the value
names; a source's may count milliseconds or nanoseconds instead
([PARQUET-9](#parquet-9)). A [Values](../values.md) `datetime` carries no zone
and no offset, so its Timestamp is not adjusted to UTC, and an implementation
must not attach a zone when writing or reading. A `datetime` has whole-second
resolution: it is written with the microsecond unit, and its microsecond part
is always zero.

Evidence: `rust/crates/yamaa-application/tests/table_arrow_table.rs::closed_schema_and_exact_values_include_full_integers_temporals_nulls_and_zero_sign`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::artifacts_keep_complete_temporal_fields_and_drop_collected_precision`, `python/tests/installed/installed_original_specifications.py::test_closed_parquet_types_and_exact_values_with_independent_reader`.

The same wall clock survives a write and a read. Shifting a value into or out
of a machine timezone changes the value.

<a id="parquet-3"></a>

### PARQUET-3. A float is stored as the binary64 value its derivation produced, never rounded

A `float` enters this profile as the binary64 value its derivation produced.
`output.decimals` does not apply, and no rounding happens on output, so a
consumer that reads the artifact receives the value the calculation used.

Evidence: `rust/crates/yamaa-application/tests/table_arrow_table.rs::retired_python_double_operands_round_trip_without_display_rounding`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::output_decimals_do_not_apply_to_a_parquet_output`, `python/tests/installed/installed_original_specifications.py::test_original_build_saves_parquet_with_independent_logical_readback`.

Storing native double values is not display. Display rounding happens once, in
the `csv` profile under [CSV-8](csv.md#csv-8), and an `output.decimals`
declared on a `parquet` path fails under [CSV-6](csv.md#csv-6).

<a id="parquet-4"></a>

### PARQUET-4. Parquet artifacts of one completed dataset read back identically, without identical bytes

Parquet artifacts written from the same completed dataset must read back
identically: the same field names in the same order, the same logical types,
the same rows in the same order, the same nulls, and the same values, with
every `DOUBLE` bit-identical. An implementation writes uncompressed pages and
adds no key-value metadata of its own beyond what the format requires.
Conforming parquet artifacts are not required to have identical bytes.

Evidence: `schema-parquet`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::closed_schema_and_exact_values_include_full_integers_temporals_nulls_and_zero_sign`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.

An artifact that must be byte-identical uses the `csv` profile.

## Reading

<a id="parquet-5"></a>

### PARQUET-5. A parquet source is one Parquet file, read in its stored field and record order

A path selected as `parquet` is read as one Apache Parquet file; the path is
not sniffed and there is no fallback to another profile. Fields are delivered
in file-schema order, and records in their stored row-group order and their
order within each row group. Parallel reads, batches and projection must not
reorder either sequence. A file with fields and no records is a valid empty
source. Bytes that are not one readable Parquet file fail.

Fails: `source_parquet_invalid`.
Evidence: `schema-parquet`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::empty_tables_and_nonfinite_inputs_keep_the_closed_schema_and_normalized_nulls`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::independently_authored_parquet_schemas_keep_exact_spelling_and_raw_value`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::every_record_reads_in_order_past_the_row_groups_one_footer_numbers`, `rust/crates/yamaa-adapters/src/parquet_source/metadata.rs::a_null_fixed_size_list_row_has_no_size_and_another_size_is_invalid`.

A file with no readable Parquet schema is not a valid empty source. Bytes that
are not one readable Parquet file fail with `source_parquet_invalid`.

<a id="parquet-6"></a>

### PARQUET-6. Every Parquet field has a non-empty unique name and a scalar type

Every schema field name is non-empty and unique under
[Values](../values.md) equality. A field is scalar: nested, repeated and
extension types are outside this profile. A dictionary in the embedded Arrow
schema is a layout of its value type, not a type of its own
([PARQUET-7](#parquet-7)). An empty or duplicate field name fails.

Fails: `source_field_name_empty`, `source_field_name_duplicate`.
Evidence: `rust/crates/yamaa-adapters/tests/table_sas_source.rs::independently_authored_parquet_schemas_keep_exact_spelling_and_raw_value`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::nested_roots_name_their_own_field_after_flat_columns_and_preserve_name_precedence`, `rust/crates/yamaa-adapters/src/parquet_source/profile.rs::field_names_and_physical_alignment_have_stable_precedence`.

An empty name fails with `source_field_name_empty` and a duplicate name with
`source_field_name_duplicate`. A type outside this profile falls outside the
closed mapping of [PARQUET-7](#parquet-7).

<a id="parquet-7"></a>

### PARQUET-7. The embedded Parquet type supplies the field type through a closed mapping that inverts the write mapping and admits signed integers, single-precision floats and other timestamp units

The embedded Parquet type supplies the [Inputs](../inputs.md) field type
through the closed mapping below: the inverse of [PARQUET-1](#parquet-1); an
`INT32` or `INT64` field, unannotated or annotated as a signed integer whose
width fits its physical type, which reads as `int`; a `FLOAT` field, as
`float`; and a timestamp in milliseconds or nanoseconds, as `datetime`. The
mapping is exact: a different physical type, logical type, integer sign or
timezone is not converted or inferred, and fails. A timestamp adjusted to UTC
and a `BOOLEAN` field fall outside it.

Fails: `source_field_type_unsupported`.
Evidence: `adam-adsl-randomization`, `rust/crates/yamaa-adapters/src/parquet_source/profile.rs::only_the_closed_pairs_pass_including_legacy_string_date_and_large_offsets`, `rust/crates/yamaa-adapters/src/parquet_source/profile.rs::view_and_dictionary_layouts_read_as_their_value_type`, `rust/crates/yamaa-adapters/src/parquet_source/profile.rs::signed_integer_annotations_read_as_int_and_declare_their_width`, `rust/crates/yamaa-adapters/src/parquet_source/closed_profile.rs::signed_integer_fields_that_fit_their_physical_type_read_as_int`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::unsupported_stored_types_retain_independent_arrow_type_spelling`, `rust/crates/yamaa-adapters/src/parquet_source/metadata.rs::a_null_fixed_size_list_row_has_no_size_and_another_size_is_invalid`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::the_pyarrow_int32_fixture_reads_as_int`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::dictionaries_of_the_other_admitted_types_read_as_their_value_type`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::duckdb_signed_integer_fields_read_as_int`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::unannotated_int32_fields_read_as_int`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::logical_signed_integer_annotations_read_as_int`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::unsigned_annotations_stay_refused`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::a_stored_value_outside_its_declared_width_is_invalid`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::an_integer_annotation_its_physical_type_cannot_carry_is_invalid`, `python/tests/installed/installed_original_specifications.py::test_parquet_signed_integer_fields_read_as_int`, `rust/crates/yamaa-adapters/src/parquet_source/closed_profile.rs::a_column_row_admits_its_annotations_and_layouts_alone`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::millisecond_and_nanosecond_timestamps_read_as_the_whole_seconds_they_count`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::single_precision_floats_widen_exactly_and_nonfinite_ones_are_missing`, `python/tests/installed/installed_parquet_writer_defaults.py::test_the_types_pyarrow_writes_by_default_read_and_save`, `schema-source-parquet-defaults`, `negative-source-boolean-field`.

| Parquet type | Column type |
|---|---|
| `BYTE_ARRAY` annotated `String` | `str` |
| `INT64` with no annotation | `int` |
| `INT64` annotated `Int(64, signed)` | `int` |
| `INT32` with no annotation | `int` |
| `INT32` annotated `Int(8, signed)`, `Int(16, signed)` or `Int(32, signed)` | `int` |
| `DOUBLE` with no annotation | `float` |
| `FLOAT` with no annotation | `float` |
| `INT32` annotated `Date` | `date` |
| `INT64` annotated `Timestamp`, microseconds, no UTC adjustment | `datetime` |
| `INT64` annotated `Timestamp`, milliseconds, no UTC adjustment | `datetime` |
| `INT64` annotated `Timestamp`, nanoseconds, no UTC adjustment | `datetime` |

Nullable and non-nullable fields are both admitted; nullability states
whether this particular file can store a missing value and does not change
the field's column type.

A field with no annotation has neither a logical type nor a converted type.
A signed integer annotation is the logical type `Int(bitWidth, signed)` or, on
a field without a logical type, the converted type of the same width: `INT_8`,
`INT_16`, `INT_32` or `INT_64`. Either annotation, or both, reads as `int`, as
does an `INT32` or `INT64` field with no annotation, and an `INT32` value reads
as the same 64-bit integer. An unsigned annotation,
`Int(bitWidth, unsigned)` or `UINT_8` through `UINT_64`, falls outside the
mapping. An integer annotation that its physical type cannot carry, such as
`Int(64, signed)` on `INT32`, and a stored value outside the width its
annotation declares, such as 128 in an `INT32` field annotated
`Int(8, signed)`, are not valid Parquet and fail under
[PARQUET-5](#parquet-5).

A `FLOAT` value widens exactly to the binary64 value it names, so a
single-precision 0.1 reads as 0.100000001490116119384765625, never as the
binary64 0.1, and a non-finite one is missing ([PARQUET-8](#parquet-8)). A
timestamp counts milliseconds, microseconds or nanoseconds, as its
annotation says, and reads as the datetime of that count when the count is a
whole second ([PARQUET-9](#parquet-9)). A timestamp adjusted to UTC, in any
unit and whatever zone the embedded Arrow schema names, falls outside the
mapping: a [Values](../values.md) `datetime` names a wall clock with no zone
([PARQUET-2](#parquet-2)), and reading an instant as one would choose a zone
that the file does not state. A `BOOLEAN` field falls outside it too: no
column type holds a true-or-false value, and reading one as text or as a
number would choose a spelling that the file does not state. A legacy
`TIMESTAMP_MILLIS` or `TIMESTAMP_MICROS` converted type on a field without a
logical type is adjusted to UTC, and falls outside the mapping with them.

The file's embedded Arrow schema can name one physical/logical pair in several
Arrow layouts, and the layout does not change the column type. A `BYTE_ARRAY`
field annotated `String` reads as `str` whether that schema names it `string`,
`large_string` or `string_view`. A field that schema names as a `dictionary`
reads as it would if the schema named the dictionary's value type, whatever the
index type: a `dictionary` of `string` values over a `BYTE_ARRAY` field
annotated `String` reads as `str`, and a `dictionary` of `int64` values over an
`INT64` field with no annotation reads as `int`. Only the layout differs: the
offset width, the string views or the dictionary indices. A field outside the
mapping fails with `source_field_type_unsupported`, reporting the field and
stored type.

<a id="parquet-8"></a>

### PARQUET-8. A Parquet null is the missing value, and present values arrive unchanged

A Parquet null is the missing value. A present zero-length string is the
collected empty string, and the two remain distinct. A finite `DOUBLE` is
delivered bit-identically, and a finite `FLOAT` as the binary64 value it
names. A non-finite `DOUBLE` or `FLOAT` is normalized immediately to missing
under [Values](../values.md).

Evidence: `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::independent_codecs_page_versions_and_encodings_preserve_exact_values`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::empty_tables_and_nonfinite_inputs_keep_the_closed_schema_and_normalized_nulls`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::single_precision_floats_widen_exactly_and_nonfinite_ones_are_missing`, `rust/crates/yamaa-application/tests/specification_sources.rs::parquet_input_policy_and_owned_source_observations_use_the_shared_build_path`.

<a id="parquet-9"></a>

### PARQUET-9. A stored date or timestamp names an in-range civil value on a whole second

A `Date` count must name a date from `0001-01-01` through `9999-12-31`. A
`Timestamp` count, in its annotation's unit, must name a zone-free local
datetime in the same calendar range and must be an exact whole second. The
unit may be milliseconds, microseconds or nanoseconds; no timezone shift,
truncation or rounding is permitted. A temporal value outside this contract
fails.

Fails: `source_field_value_invalid`.
Evidence: `adam-adsl-randomization`, `rust/crates/yamaa-adapters/tests/table_sas_source.rs::independently_authored_parquet_schemas_keep_exact_spelling_and_raw_value`, `rust/crates/yamaa-adapters/src/parquet_source/profile.rs::temporal_edges_are_integral_and_never_rounded_into_range`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::millisecond_and_nanosecond_timestamps_read_as_the_whole_seconds_they_count`, `schema-source-parquet-defaults`, `negative-source-fractional-second`.

The failure reports the field as `field`, the record as `record`, counted
from one, and the stored integer value as `value`, in the field's own unit: a
millisecond count of 1,500 fails with the value `1500`. A whole second in any
of the three units is the same datetime, which the
[Values](../values.md) `datetime` holds without its unit. A nanosecond count
reaches only the datetimes from `1677-09-21T00:12:44` through
`2262-04-11T23:47:16`.

<a id="parquet-10"></a>

### PARQUET-10. File metadata and layout never change the fields, records, types or values delivered

Key-value metadata, statistics, page encodings, compression and row-group
sizing do not alter delivered fields, records, types or values, and are
ignored. They cannot supply or override a field type.

Evidence: `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::independent_codecs_page_versions_and_encodings_preserve_exact_values`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::empty_tables_and_nonfinite_inputs_keep_the_closed_schema_and_normalized_nulls`, `python/tests/installed/installed_original_specifications.py::test_original_ordered_sum_reads_parquet_and_preserves_complete_truth`, `rust/crates/yamaa-adapters/tests/table_parquet_source.rs::every_record_reads_in_order_past_the_row_groups_one_footer_numbers`, `rust/crates/yamaa-adapters/src/parquet_source/footer.rs::more_row_groups_than_an_i16_ordinal_numbers_read_whole`.
