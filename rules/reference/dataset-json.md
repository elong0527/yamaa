# Dataset-JSON source profiles

The read-only `dataset_json` and `dataset_ndjson` profiles read a CDISC
Dataset-JSON 1.1 file as an input dataset. The file alone is the source: each
column's `dataType` supplies its field's type through one closed table, a
`types` entry may only narrow it, and every value is read as that type or the
read fails. Nothing is guessed from the values, the labels or a Define-XML
document.

## Reading

<a id="dsjson-1"></a>

### DSJSON-1. `.json` and `.ndjson` select the read-only Dataset-JSON 1.1 profiles, and the file alone is the source

`.json` selects the read-only `dataset_json` profile, the standard's
single-object representation, and `.ndjson` the `dataset_ndjson` profile, its
newline-delimited one: a first line holding the file's object without `rows`,
then one row per line. Extensions are case insensitive. Only Dataset-JSON 1.1
is read: `datasetJSONVersion` is `1.1`, or `1.1.` and a release number. No
Define-XML document is read. These profiles add no output format; only a
submission's `transport` writes Dataset-JSON.

Fails: `source_dataset_json_invalid`.
Evidence: `schema-source-dataset-json`, `schema-source-dataset-ndjson`, `negative-dataset-json-version`, `negative-dataset-json-malformed`, `negative-dataset-json-missing-member`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_single_object_file_reads_each_data_type_as_the_type_its_table_gives`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_newline_delimited_file_reads_its_first_line_and_one_row_per_line`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::newline_delimited_lines_may_end_in_a_carriage_return_and_the_last_newline_may_be_omitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::members_the_profile_does_not_read_are_skipped_and_members_may_come_in_any_order`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_file_that_is_not_one_json_object_is_malformed`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_single_object_written_over_several_lines_is_no_newline_delimited_file`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_blank_line_between_rows_is_malformed`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_string_whose_escape_names_no_unicode_scalar_value_is_malformed`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_dataset_json_1_0_file_is_refused_for_its_version`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_version_that_only_begins_like_1_1_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_file_without_records_names_the_member`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_member_written_twice_names_the_member`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::columns_that_are_not_an_array_name_the_member`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_column_without_its_data_type_names_the_member_and_the_column_position`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_newline_delimited_first_line_holding_rows_names_the_member`.

**The file's object.** The profile reads `datasetJSONVersion`, the standard's
other required members (`datasetJSONCreationDateTime`, `itemGroupOID`,
`records`, `name`, `label` and `columns`) and `rows`. Each is a JSON string,
except `records`, a non-negative integer written without a fraction or an
exponent; `columns`, an array; and `rows`, an array of rows. In the
single-object form `rows` may be absent, which holds no row; a newline-delimited
first line holds no `rows`. Every other member, such as `sourceSystem`,
`originator`, `studyOID`, `metaDataRef` or `dbLastModifiedDateTime`, is
skipped unread, and members may come in any order. A member the profile reads
that is absent, written twice or of another JSON kind fails as `member`
([DSJSON-5](#dsjson-5)).

**The columns.** Each `columns` entry is an object holding the standard's
required `itemOID`, `name`, `label` and `dataType`, each a JSON string. Its
`name` names the field, and its `dataType` types it under
[DSJSON-2](#dsjson-2); `itemOID` and `label` are not read further, and
`targetDataType`, `length`, `displayFormat` and `keySequence` are skipped.

**The bytes.** The file is UTF-8 JSON text without a byte-order mark. In the
single-object form the whole file is one JSON object, with only whitespace
around it. In the newline-delimited form each line ends in `U+000A`, which the
last line may omit, and a `U+000D` before it belongs to the line ending, as
the standard admits ([JSON-13](../profiles/dataset-json.md#json-13)); every
line, the first included, is one JSON text, and an empty line among them fails
as `malformed`. A pretty-printed object in an `.ndjson` file, or a
newline-delimited file named `.json`, therefore fails as `malformed`.

**Versions.** The version is `1.1`, or `1.1.` followed by `0` or by digits
that do not begin with `0`, as the standard's schema writes it. A Dataset-JSON
1.0 file, whose structure differs, names its own version and fails as
`version`, as does any other version.

The input is captured once, as every input is
([FILE-11](files.md#file-11)), and the run report records its profile as
`dataset_json` or `dataset_ndjson` and the rows it decoded as its records
([CHECK-24](../verification.md#check-24)). A file a submission writes reads
back under [JSON-15](../profiles/dataset-json.md#json-15).

## Field types

<a id="dsjson-2"></a>

### DSJSON-2. Each column's `dataType` supplies its field's type through a closed table, and `types` may only narrow it

Each column's `dataType` supplies its field's type through the closed table
below, which inverts [JSON-10](../profiles/dataset-json.md#json-10):
`string`, `URI` and `time` give `str`; `integer` gives `int`; `float`,
`double` and `decimal` give `float`; `date` gives `date`; and `datetime` gives
`datetime`. `boolean`, or any other spelling, names no type and fails.
`targetDataType`, `label`, `length` and `displayFormat` select nothing. A
`types` entry may restate the supplied type or narrow it as the table states;
any other entry fails.

Fails: `source_dataset_json_invalid`, `unknown_field`.
Evidence: `schema-source-dataset-json`, `schema-source-dataset-ndjson`, `negative-dataset-json-boolean`, `rust/crates/yamaa-adapters/tests/table_dataset_json_source.rs::a_types_entry_narrows_exactly_as_the_narrowing_table_states`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_single_object_file_reads_each_data_type_as_the_type_its_table_gives`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::types_narrow_text_and_numbers_and_read_a_decimal_or_temporal_value_as_its_text`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_boolean_column_names_no_type`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_data_type_outside_the_standard_names_no_type`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_types_entry_widening_an_integer_is_refused_at_the_entry`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_types_entry_naming_no_stored_field_is_an_unknown_field`.

| `dataType` | Supplies | A `types` entry may also declare |
|---|---|---|
| `string`, `URI`, `time` | `str` | `int`, `float`, `date`, `datetime` |
| `integer` | `int` | none |
| `float`, `double` | `float` | `int` |
| `decimal` | `float` | `int`, `str` |
| `date` | `date` | `str` |
| `datetime` | `datetime` | `str` |
| `boolean`, any other | none: fails as `data_type` | none |

A field narrowed from text is read as a delimited field is
([INPUT-7](../inputs.md#input-7)). A number narrowed to `int` accepts only an
exactly integral value within range ([VALUE-7](../values.md#value-7)); it
never rounds or truncates. A `decimal`, `date` or `datetime` field declared
`str` holds the text the file stores, unparsed: a decimal keeps its trailing
zeros, and a partial date a sender wrote under `date` stays text. `integer`
admits no other type, and nothing narrows a number to a date: Dataset-JSON
writes dates as ISO 8601 text, never as counts.

`targetDataType` asks a receiving system to convert a value, such as an ISO
date into the SAS date a document describes; the reader ignores it, so a
`date` column with `targetDataType` `integer` reads as `date`. A file a
submission writes for a `str` column the document submits as `date` or
`datetime` therefore reads as that type unless the input declares the field
`str`.

The checks are made at ingest, once the file's columns are read, as a SAS
input's are ([SAS-3](sas.md#sas-3)). A `types` entry naming a field the file
does not store fails with `unknown_field` at `input.<dataset>.types.<field>`
([INPUT-6](../inputs.md#input-6)); a refused pair fails as `field_type` at the
same entry. A producing-specification link keeps its sole type authority under
[INPUT-6](../inputs.md#input-6): its types are checked against the table as
`types` entries are.

## Values and order

<a id="dsjson-3"></a>

### DSJSON-3. Each value is spelled as its column's `dataType` writes it and parses into its field's type, or fails ingestion

A value is `null`, which is missing, or the JSON kind
[JSON-11](../profiles/dataset-json.md#json-11) writes for its column's
`dataType`: a number for `integer`, `float` and `double`, and a string for
every other, a `decimal` included. Its text then becomes its field's type by
the `str` row of [VALUE-3](../values.md#value-3), as
[INPUT-7](../inputs.md#input-7) reads delimited text. A value of another JSON
kind fails as `cell`, and one that does not parse fails with
`field_parse_failed`, before any derivation.

Fails: `field_parse_failed`, `source_dataset_json_invalid`.
Evidence: `schema-source-dataset-json`, `negative-dataset-json-value-type`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_single_object_file_reads_each_data_type_as_the_type_its_table_gives`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::an_empty_string_the_input_declares_present_is_the_collected_empty_string`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::an_integer_written_as_a_json_string_names_its_record_and_field`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_boolean_value_names_its_record_and_field`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::an_integer_with_a_fraction_does_not_parse_as_int_at_the_input_path`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::an_integral_number_written_with_a_fraction_reads_as_an_int`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_date_outside_the_calendar_does_not_parse_as_date_at_the_input_path`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_narrowed_value_that_does_not_parse_fails_at_its_types_entry`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_dataset_json_value_its_data_type_does_not_parse_fails_at_the_input_path`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_dataset_json_value_its_types_entry_does_not_parse_fails_at_the_entry`.

A value's text is a string's characters, or a number exactly as the file
writes it. An `int` field reads it by numeric parsing and then integral
conversion ([VALUE-6](../values.md#value-6),
[VALUE-7](../values.md#value-7)), so `2` and `2.0` read as 2 and `2.5` does
not parse; a `float` field by numeric parsing, a number beyond binary64
becoming missing ([VALUE-4](../values.md#value-4)); and a `date` or
`datetime` field by exactly the forms of [VALUE-15](../values.md#value-15), so
a zone, an offset or a fractional second does not parse. A `str` field holds
the text as it is.

A zero-length string of a field read as `str`, whether its `dataType`
supplies `str` or a `types` entry declares it, follows the input's
`empty_string` ([INPUT-9](../inputs.md#input-9)): it is missing under the
default `missing`, and the collected empty string under `present`. The
convention is applied before narrowing, so a zero-length `string` value
narrowed to `int` is missing under `missing` and does not parse under
`present`. A zero-length string of a `decimal`, `date` or `datetime` field
read as its own type does not parse. No other text is missing
([INPUT-8](../inputs.md#input-8)).

`field_parse_failed` reports the dataset, the field, the type it is read as
in `type`, the value's text in `value`, and the value's row in `record`,
counted from one. It reports at `input.<dataset>.types.<field>` when a `types`
entry declared the type, and at `input.<dataset>.path` when the `dataType`
supplied it. A value of another JSON kind, `true`, `false`, an array or an
object included, fails as `cell` ([DSJSON-5](#dsjson-5)).

<a id="dsjson-4"></a>

### DSJSON-4. Fields are the `columns` in order and records the rows in stored order, and `records` counts them

Stored field order is the `columns` order, and record order is the rows'
stored order ([INPUT-2](../inputs.md#input-2)); each row is an array of one
value per column, in that order. `records` must equal the number of rows. A
row of another width, or one that is not an array, fails as `row_width`, and
rows that `records` does not count fail as `records`. The input is read whole
from its captured snapshot and bounded as a whole table, and a generated
ordinal counts its rows from one.

Fails: `source_dataset_json_invalid`.
Evidence: `schema-source-dataset-ndjson`, `negative-dataset-json-row-width`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_generated_ordinal_counts_the_rows_from_one`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::fewer_rows_than_records_counts_are_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_row_beyond_the_ones_records_counts_is_refused`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_row_with_a_value_too_few_names_its_record`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_row_that_is_not_an_array_names_its_record`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::a_column_name_written_twice_names_the_name`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/dataset_json_sources.json::an_empty_column_name_names_its_position`.

A column name is the field's name exactly; an empty name, or one two columns
share, fails as `field_name`. The first row is record 1 in both
representations, although in the newline-delimited form it is the file's
second line.

The columns are admitted under the column limit before any is kept, and
`records` is admitted under the row and cell limits before any row is read; a
file over a limit is refused as that limit, which is separate from the causes
of [DSJSON-5](#dsjson-5). A row beyond the ones `records` counts is refused
before it is read, and fewer rows than it counts are refused once the last row
is read.

The input is read whole, as a SAS or ODM input is: a run that reads its driver
input by partition reads a Dataset-JSON input whole, and the row and cell
limits bound such an input, and the output of a run it drives, as whole
tables.

<a id="dsjson-5"></a>

### DSJSON-5. A Dataset-JSON file that cannot be read fails ingestion with one closed cause

`source_dataset_json_invalid` reports in the `ingest` phase at
`input.<dataset>.path`, or at the `types` entry for cause `field_type`, with
the dataset, the path as written and a closed `cause`: `malformed`, `version`,
`member`, `field_name`, `data_type`, `field_type`, `records`, `row_width` or
`cell`. The file's object and columns are checked before any row is read, and
the read stops at the first failure.

Fails: `source_dataset_json_invalid`.
Evidence: `negative-dataset-json-version`, `negative-dataset-json-malformed`, `negative-dataset-json-missing-member`, `negative-dataset-json-boolean`, `negative-dataset-json-row-width`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_dataset_json_file_that_is_not_its_json_text_is_malformed`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_dataset_json_file_of_another_version_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_dataset_json_file_without_a_member_it_reads_names_the_member`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_dataset_json_column_without_a_member_it_reads_names_the_column_position`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::an_empty_dataset_json_column_name_names_its_position`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_repeated_dataset_json_column_name_names_the_name`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_dataset_json_data_type_that_names_no_type_names_the_field_and_data_type`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_types_entry_a_dataset_json_data_type_does_not_admit_is_refused_at_the_entry`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::dataset_json_rows_that_records_does_not_count_are_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_dataset_json_row_of_another_width_names_its_record`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/sources.json::a_dataset_json_value_of_another_json_kind_names_its_record_and_field`.

| Cause | Rejects | Context beyond the dataset, path and cause |
|---|---|---|
| `malformed` | bytes that are not the representation's JSON text: no UTF-8 JSON, not one object, a byte-order mark, an empty line, or a line of an `.ndjson` file that is not one JSON text | none |
| `version` | a `datasetJSONVersion` that names no 1.1 release ([DSJSON-1](#dsjson-1)) | none |
| `member` | a member the profile reads that is absent, written twice or of another JSON kind, `rows` on a newline-delimited first line, or a `columns` entry that is not an object | `member`; for a column's member, `field`, the entry's position |
| `field_name` | an empty column name, or one two columns share | `field`: an empty name's position, or the shared name |
| `data_type` | a `dataType` that names no type ([DSJSON-2](#dsjson-2)) | `field`, `data_type` |
| `field_type` | a `types` entry the narrowing table does not admit | `field`, `data_type`, and the declared `type` |
| `records` | rows more or fewer than `records` counts ([DSJSON-4](#dsjson-4)) | none |
| `row_width` | a row that is not an array of one value per column | `record` |
| `cell` | a value of another JSON kind than its `dataType` writes ([DSJSON-3](#dsjson-3)) | `record`, `field` |

Positions are counted from one: a column's position among the `columns`
entries, and a row's `record` among the rows.

The file's object is checked first: its JSON text, then the version, then each
member the profile reads, in the standard's order. Then the columns: each
entry's members, then the field names, then the data types, then the input's
`types` entries in written order, an `unknown_field` included. Then the rows,
in order, each value in column order: a row beyond the ones `records` counts,
a row of another width, a value of another JSON kind and a value that does not
parse ([DSJSON-3](#dsjson-3)) each stop the read where they are found, and
fewer rows than `records` counts fail after the last row. Undecodable text
fails without replacement characters.
