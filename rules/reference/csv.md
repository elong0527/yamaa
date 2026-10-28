# CSV profile

The `csv` profile writes an artifact as one canonical spelling of delimited
text and reads a delimited source exactly as it is stored. These rules fix the
bytes a writer produces, the display precision of `float` values, and the
spellings a reader admits.

## Writing

<a id="csv-1"></a>

### CSV-1. A csv artifact is UTF-8 without a byte-order mark, and every record ends in a line feed

The artifact is UTF-8 and has no byte-order mark. `U+002C` separates fields,
and no other delimiter is defined. `U+000A` terminates every record, including
the last, so every artifact ends with it. `U+000D` is never written as part of
a terminator; it appears only inside a quoted field that contains one.

Evidence: `schema-text-functions`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::exact_quoting_preserves_empty_missing_whitespace_unicode_and_line_characters`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::retired_python_scalar_text_and_calendar_edges_keep_exact_csv_bytes`.

[Values](../values.md) owns the text being encoded. Writing a
byte-order mark, a `U+000D` record terminator, or a quoting that differs from
[CSV-3](#csv-3) is not an implementation option.

<a id="csv-2"></a>

### CSV-2. The header names `output.columns` in order, and each row follows as one record

The first record is the header: the `output.columns` names, in that order,
written under the quoting rule of [CSV-3](#csv-3). Each following record is one
row, in the order [OUTPUT-11](../output.md#output-11) fixes, holding one field
per header name in the same order. An artifact with no rows is the header
record and its terminator alone.

Evidence: `schema-ordering`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::header_only_and_projection_order_are_exact`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.

An artifact with no rows is not empty: the columns exist whether or not a row
does.

<a id="csv-3"></a>

### CSV-3. A field is quoted exactly when it is empty or contains a quote, comma, `U+000D` or `U+000A`

A field is quoted exactly when its text contains `U+0022`, `U+002C`, `U+000D`
or `U+000A`, or when the field is the empty string; every other field is
written bare. A quoted field is wrapped in `U+0022`, and each `U+0022` within
its text is written twice. Nothing else is escaped: a quoted field carries its
newlines, delimiters and every other scalar exactly.

Evidence: `adam-adsl-investigator-comment`, `adam-adcm-atc-classes`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::exact_quoting_preserves_empty_missing_whitespace_unicode_and_line_characters`.

A writer that quotes a field this condition leaves bare, or leaves bare a field
this condition quotes, does not conform even if ordinary readers accept its
output.

<a id="csv-4"></a>

### CSV-4. Missing is written as an empty bare field and the empty string as two quote characters

A missing value is written as no characters at all, unquoted. A collected
empty string is written as two quote characters. The two forms stay apart in
the artifact, and no text stands in for absence. Reading does not restore the
pair: [INPUT-8](../inputs.md#input-8) reads an empty field as
missing whether it was bare or quoted.

Evidence: `schema-str-replace`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::exact_quoting_preserves_empty_missing_whitespace_unicode_and_line_characters`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.

A collected empty string written here therefore returns as missing if the
artifact is later read as a delimited source.

```text
STUDYID,COMMENT,NOTE
S1,plain text,
S1,"has, comma",""
S1,"say ""hi""",x
```

The third row's `NOTE` is the ordinary string `x`. The second row's is a
collected empty string. The first row's is missing.

<a id="csv-5"></a>

### CSV-5. Each value is written as the canonical text of its column type

A value is written as the text its column type fixes in the table below. An
`int` is written without a leading `U+002B`, without digit grouping and without
a leading zero; zero is `0`. A `float` that takes no display precision is
written by [Values](../values.md)'s conversion to `str`:
the shortest round-tripping digits in positional notation, with a trailing
`.0` omitted. That conversion has no exponent.

Evidence: `adam-adlb-bds`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::scalar_output_uses_exact_integer_and_shortest_positional_float_text`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::retired_python_scalar_text_and_calendar_edges_keep_exact_csv_bytes`.

| Column type | Text written |
|---|---|
| `str` | its scalar values, under [Values](../values.md) |
| `int` | its decimal digits, with a leading `U+002D` when negative |
| `float` | [Values](../values.md)'s float text, or the fixed-point form of [CSV-7](#csv-7) |
| `date` | [Values](../values.md)'s canonical `date` text |
| `datetime` | [Values](../values.md)'s canonical `datetime` text |

## Display precision

<a id="csv-6"></a>

### CSV-6. `output.decimals` is an integer from 0 to 18 for every float column of a csv artifact

`output.decimals` is optional. When declared, it is an integer from 0 through
18, inclusive, and it applies to the `csv` profile alone and to every `float`
column of the artifact. The declaration is validated even when the artifact
has no rows or no `float` columns. An invalid value fails validation at
`output.decimals`, and so does a declaration on an output path the extension
mapping resolves to `parquet`.

Fails: `invalid_field_type`, `decimals_not_applicable`.
Evidence: `adam-adlb-reported-precision`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/csv_precision.json::negative_decimals_fail_the_output_and_publish_nothing`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/csv_precision.json::decimals_above_eighteen_fail_the_output_and_publish_nothing`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/csv_precision.json::decimals_beyond_any_integer_fail_the_output_and_publish_nothing`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/csv_precision.json::decimals_on_a_parquet_output_are_not_applicable_and_publish_nothing`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_output_validation.json::a_csv_precision_of_19_is_refused_for_a_float_output_without_records`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_output_validation.json::a_csv_precision_of_19_is_refused_for_an_int_output`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_output_validation.json::a_csv_precision_of_19_is_refused_for_a_float_output_without_study_data`.

An invalid value fails with `invalid_field_type`. A declaration on a `parquet`
path fails with `decimals_not_applicable`: a display precision that cannot
take effect is a defect in the specification, not a setting to ignore.

<a id="csv-7"></a>

### CSV-7. With `output.decimals` of n, every float is written in fixed point with exactly n decimal digits

When `output.decimals` is absent, a `float` is written as
[Values](../values.md)'s float text. When it is present
with the value `n`, a `float` is written in fixed-point form with exactly `n`
digits after the decimal point, and with a decimal point only when `n` is
greater than zero. A value keeps its declared width whether or not its digits
require it: at `n` of 4, an integral 25 is written `25.0000`.

Evidence: `adam-adlb-reported-precision`, `negative-transport-display-decimals`, `rust/crates/yamaa-core/tests/source_csv_precision.rs::ties_and_near_ties_round_away_from_zero_with_independent_text`, `rust/crates/yamaa-core/tests/source_csv_precision.rs::large_precision_uses_the_exact_fraction_and_retains_all_declared_zeros`.

The fixed-point text is the artifact's presentation, and nothing the
specification computes sees it ([CSV-8](#csv-8)); a reader of the artifact
receives the rounded value. A submission section that requests a transport
reads the published artifact back, so a specification it composes declares no
`output.decimals` ([JSON-5](../profiles/dataset-json.md#json-5)), and the
submitted display is the column's `submission.display_format`.

<a id="csv-8"></a>

### CSV-8. Display rounding happens once, when a field is written, and no other stage sees it

Writing a field under `output.decimals` is the only place a value is rounded
for presentation. It happens once, when the field is written, after everything
the [Column derivation](../columns.md) sequences: every
derivation, every conversion, every verification, key validation and row
ordering. No dependent column, predicate, aggregate, verification, key or
order term sees a rounded value, and changing `output.decimals` cannot change
whether a run passes or which rows it produces.

Evidence: `adam-adlb-reported-precision`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/csv_precision.json::two_decimals_round_the_saved_bytes_and_leave_the_output_unrounded`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::a_result_reads_its_input_once_and_records_its_rounded_bytes_only_when_saved`, `python/tests/installed/installed_public_environment.py::test_csv_precision_changes_saved_bytes_only_and_preserves_derived_values`.

<a id="csv-9"></a>

### CSV-9. Display rounding applies the one rounding to the exact binary64 value

Each binary64 value is an exact decimal fraction. Writing it with `n` digits
rounds that exact value with the one rounding of
[FORMULA-16](../formulas.md#formula-16): a tie, or a value within
`2^-26 * 10^-n` below a tie, goes away from zero. The rounded decimal is
written directly, and a value that rounds to zero is written without a sign.
No host rounding or formatting routine may be assumed to do this.

Evidence: `adam-adlb-reported-precision`, `rust/crates/yamaa-core/tests/source_csv_precision.rs::ties_and_near_ties_round_away_from_zero_with_independent_text`, `rust/crates/yamaa-application/tests/table_arrow_table.rs::precision_serializes_float_projection_only_and_preserves_missing_and_empty_text`.

The difference is observable:

| Value as written in source | Its exact binary64 value | `decimals: 2` |
|---|---|---|
| `0.125` | 0.125 | `0.13` |
| `-0.125` | -0.125 | `-0.13` |
| `2.675` | 2.674999999999999822364316059974953532218933105468750 | `2.68` |
| `1.004999999` | 1.0049999990000000327228235619259066879749298095703125 | `1.00` |

`0.125` is a genuine tie and rounds away from zero. `2.675` is `1.8e-14` below
its tie, inside the window, so it rounds up as a decimal reader expects;
`1.004999999` is `1e-7` below its tie, outside the window, so it rounds down.
A routine that sends an exact tie to the even digit reports `0.12` for
`0.125` and does not conform. Rounding a value another stage can observe
([CSV-8](#csv-8)) is not an implementation option.

## Reading

<a id="csv-10"></a>

### CSV-10. A csv source is UTF-8, with a byte-order mark only where its input declares one

A delimited source is UTF-8. Ill-formed encoded bytes fail under
[Values](../values.md) rather than being replaced, skipped or decoded
under a machine default. A byte-order mark is rejected rather than skipped,
unless the input declares `byte_order_mark: true`: then the source must begin
with one mark, which is skipped.

Fails: `invalid_text`, `source_byte_order_mark`, `value_not_permitted`.
Evidence: `negative-source-bad-encoding`, `schema-source-byte-order-mark`, `negative-source-undeclared-byte-order-mark`, `negative-source-missing-byte-order-mark`, `negative-source-byte-order-mark-parquet`, `rust/crates/yamaa-adapters/tests/table_csv_source.rs::defects_have_exact_priority_and_coordinates`, `rust/crates/yamaa-adapters/tests/table_csv_source.rs::a_declared_mark_is_skipped_once_and_any_other_mark_fails`, `rust/crates/yamaa-adapters/tests/fixtures/csv_profile_diagnostics.json::bom`, `rust/crates/yamaa/tests/csv_byte_order_mark.rs`.

Ill-formed encoded bytes fail with [Values](../values.md)'s
`invalid_text`, reporting the record and field the reader had reached.
Decoding precedes every other finding, a byte-order mark included.

A byte-order mark is the three bytes `EF BB BF`, which encode `U+FEFF`.
`byte_order_mark` is an optional Boolean of an input that the csv profile
reads. When it is true, the source must begin with one mark, which the reader
skips, and the first record starts after it. A source that does not begin with
a mark fails with `source_byte_order_mark`, and so does a second mark after the
declared one, since only one is skipped: the declaration decides, never the
file. When it is false or omitted, a mark that begins the source fails with
`source_byte_order_mark`. Either way the failure names record 1 and field 1,
and its context records `declared`, whether the input declares a mark. A
`U+FEFF` after the first character of the source is a character of its field.

On an input that another profile reads, `byte_order_mark` fails with
`value_not_permitted` at `input.<dataset>.byte_order_mark`, whatever its value,
before any input is read; on an input whose path selects no profile,
[INPUT-1](../inputs.md#input-1) fails instead. The artifacts of
[CSV-1](#csv-1) never carry a mark.

<a id="csv-11"></a>

### CSV-11. A record ends in `U+000A`, optionally preceded by `U+000D`, the last may end the file, and blank lines that end the file are no record

A reader admits a spelling other than the one [CSV-1](#csv-1) writes only when
both spellings deliver the same records. A record terminator is `U+000A`,
optionally preceded by `U+000D`, and the final record may omit its terminator.
Blank lines after the last record, up to the end of the file, are no record,
whatever the header's width. `U+000D` occurs only as the first character of a
record terminator; anywhere else, including inside a quoted field, it fails
rather than joining the value.

Fails: `source_carriage_return`.
Evidence: `adam-advs-windows`, `schema-source-trailing-blank-line`, `rust/crates/yamaa-adapters/tests/table_csv_source.rs::retired_python_admitted_terminators_and_header_only_schema_are_exact`, `rust/crates/yamaa-adapters/tests/table_csv_source.rs::trailing_blank_lines_are_no_record_and_an_inner_blank_line_is_one`, `rust/crates/yamaa-adapters/tests/table_partitioned_sources.rs::trailing_blank_lines_are_no_record_of_any_partition`, `rust/crates/yamaa-adapters/tests/fixtures/csv_profile_diagnostics.json::quoted-cr`, `negative-source-carriage-return`.

A writer controls its bytes and emits one spelling, while a reader receives a
study file as stored. The admitted spellings are a record terminated by
`U+000D U+000A` rather than `U+000A`, a final record with no terminator at
all, and a final record followed by blank lines. A blank line holds no
character before its terminator; every blank line from the last record's
terminator to the end of the file ends the file and is no record, so a source
of one field ends cleanly and a wider source has no short record to refuse
([CSV-13](#csv-13)). The header is always the first record: a file of blank
lines alone has an empty header name, and a header followed by blank lines
holds no records. A blank line before the last record is a record of one empty
field, and a line holding a space or a quoted empty field is no blank line
([CSV-12](#csv-12)). A line feed inside a quoted field is a character of its
value, so no line a quoted field holds is a blank line: a quoted field the
file never closes is read to the end of the file, and fails at the first
`U+000D` it holds or else as unterminated. Every other difference is refused
rather than silently repaired: a byte-order mark the input does not declare
([CSV-10](#csv-10)), a `U+000D` anywhere else, and every reader option
[CSV-15](#csv-15) excludes.

<a id="csv-12"></a>

### CSV-12. Fields are separated by commas and are either bare or quoted, with nothing trimmed

`,` is the only delimiter. A field is bare or quoted, a property of the stored
field rather than of its value. A bare field runs from the delimiter or
terminator before it to the one after it and carries no `U+0022` and no
`U+000D`. A quoted field opens and closes with `U+0022`; inside it,
`U+0022 U+0022` is one literal `U+0022`, and `,` and `U+000A` are ordinary
characters of the value. A closing quote is followed only by a delimiter or a
terminator.

Fails: `source_quote_unterminated`, `source_quote_in_bare_field`, `source_text_after_quote`.
Evidence: `negative-source-unterminated-quote`, `adam-adsl-investigator-comment`, `rust/crates/yamaa-adapters/tests/table_csv_source.rs::retired_python_nonconforming_csv_has_exact_coordinates`, `negative-source-quote-in-bare-field`, `negative-source-text-after-quote`.

Nothing is trimmed. A space beside a delimiter is a character of the field,
and a reader that removes it changes a collected value. A quoted field with no
closing quote fails with `source_quote_unterminated`, a `U+0022` inside a bare
field with `source_quote_in_bare_field`, and text after a closing quote with
`source_text_after_quote`.

<a id="csv-13"></a>

### CSV-13. The header names each field once, and every later record has exactly that many fields

The first record is the header, and it names the source's fields in order.
Each name is non-empty, and no two names are the same under
[Values](../values.md)'s equality; a quoted name carries the text
inside its quotes, so a quoted empty name is an empty name and fails. Every
later record carries exactly as many fields as the header names. A file with
no bytes has no header and fails; a header-only file holds no records and is
valid.

Fails: `source_header_absent`, `source_field_name_empty`, `source_field_name_duplicate`, `source_record_width`.
Evidence: `negative-source-extra-field`, `negative-source-field-duplicate`, `negative-source-unnamed-field`, `negative-source-empty-file`.

A record with more or fewer fields than the header fails. Padding a short
record and discarding a long record's surplus are not implementation options.
An empty dataset still has its fields.

<a id="csv-14"></a>

### CSV-14. A field reaches ingestion as its text, or as missing when it has no characters

Every field reaches [Inputs](../inputs.md) as its text or
as missing, and an implementation must preserve the text and the missing
state. A field with no characters is missing, bare or quoted. Quoting is a
transport detail the reader does not report:
[Inputs](../inputs.md) gives an empty field one meaning
and never sees an empty string.

Evidence: `adam-adsl-investigator-comment`, `rust/crates/yamaa-adapters/tests/table_csv_source.rs::retired_python_quoted_and_bare_empty_records_keep_order`, `rust/crates/yamaa-adapters/tests/table_csv_source.rs::collected_text_and_missing_remain_lossless`.

<a id="csv-15"></a>

### CSV-15. No reader option changes what the csv profile fixes, and a reader repairs nothing

An implementation must not expose, and must not silently apply, a reader
option that changes what this profile fixes: no comment prefix or skipped
preamble, no alternate delimiter or quote character, no whitespace trimming,
no header synthesis or renaming, and no missing-value sentinel. A reader does
not repair a rejected file by skipping a mark, trimming a field, padding a
record, renaming a duplicate name or normalizing a terminator inside a value.
Blank lines that end the file terminate it ([CSV-11](#csv-11)); that is no
repair.

Evidence: `negative-source-na-age`, `schema-source-trailing-blank-line`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/typed_csv_sources.json::missing_looking_text_is_never_inferred_as_missing`, `rust/crates/yamaa-adapters/tests/table_csv_source.rs::discarded_ragged_rows_preserve_complete_syntax_and_header_priority`.

[Inputs](../inputs.md) fixes what a stored field means,
including that no text spells absence. No reader option skips a blank line
before the last record: that line is a record ([CSV-11](#csv-11)), and a source
whose header names more than one field refuses it for its width
([CSV-13](#csv-13)).

<a id="csv-16"></a>

### CSV-16. A csv reading failure names the dataset, the written path, and the record and field

A reading failure names the input dataset, the path exactly as the
specification wrote it, and the record and field where it was decided.
Records and fields are counted from one, and the header is record one. A
message carries no host path. Each condition rejects what the table below
lists.

Fails: `source_profile_unknown`, `source_byte_order_mark`, `source_header_absent`, `source_field_name_empty`, `source_field_name_duplicate`, `source_record_width`, `source_quote_unterminated`, `source_quote_in_bare_field`, `source_text_after_quote`, `source_carriage_return`.
Evidence: `negative-source-unterminated-quote`, `negative-source-unnamed-field`, `rust/crates/yamaa-adapters/tests/table_csv_source.rs::every_csv_cause_retains_the_complete_independent_context`, `negative-source-carriage-return`, `negative-source-quote-in-bare-field`, `negative-source-text-after-quote`, `negative-source-empty-file`.

| Condition | Rejects |
|---|---|
| `source_profile_unknown` | an extension the [Source ingestion](../inputs.md#input-1) mapping does not name |
| `source_byte_order_mark` | a byte-order mark the input does not declare, a declared one that is absent, or a second leading mark ([CSV-10](#csv-10)) |
| `source_header_absent` | a file with no header record |
| `source_field_name_empty` | an empty header name |
| `source_field_name_duplicate` | one name twice in the header |
| `source_record_width` | a record whose field count is not the header's |
| `source_quote_unterminated` | a quoted field with no closing quote |
| `source_quote_in_bare_field` | `U+0022` inside a bare field |
| `source_text_after_quote` | text after a closing quote |
| `source_carriage_return` | `U+000D` that does not begin a record terminator |

`source_text_after_quote` excludes a delimiter or a terminator after the quote.
