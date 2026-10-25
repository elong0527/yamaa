# SAS Transport

This file is a profile: it is optional, and it applies only to a build whose
selected submission section of a project file sets `transport: xpt`. Such a
build writes one SAS Transport version 5 file per composed dataset in the
public record layout SAS documents as TS-140, carrying the published dataset
and the names, labels and formats the [Define-XML](define-xml.md) document is
generated from. A domain build never writes a transport file; reading one is
the [SAS source profiles](../reference/sas.md)' concern.

## Requesting a transport file

<a id="xpt-1"></a>

### XPT-1. A section's `transport: xpt` writes each composed dataset as one SAS Transport version 5 file

`transport: xpt` requests one transport file per composed specification, named
and placed as [JSON-1](dataset-json.md#json-1) fixes and written by one writer
under [JSON-2](dataset-json.md#json-2). A dataset declared to have no data has
no file under [JSON-3](dataset-json.md#json-3). The rows are the published
artifact's rows read as [JSON-5](dataset-json.md#json-5) reads them, the
dataset's leaf names the file under [DEFINE-16](define-xml.md#define-16), and
the files publish before the document under
[JSON-14](dataset-json.md#json-14).

Evidence: `sdtm-dm-xpt`, `adam-adsl-xpt`, `negative-transport-display-decimals`, `rust/crates/yamaa-core/src/front_end/v1/project.rs::a_section_keeps_to_its_own_surface_spellings`, `rust/crates/yamaa-application/tests/submission_transport.rs::every_define_benchmark_matches_its_independent_truth_in_both_hosts`.

Read as [JSON-5](dataset-json.md#json-5) reads them, the rows carry no display
precision: a composed specification declares no `output.decimals`, whose
rounded text the file would otherwise carry as the submitted numbers, and a
numeric variable's display is its format under [XPT-9](#xpt-9).

Every refusal this profile states is a named validation or output failure. It
reports the dataset, and the column and row where they are known. Nothing is
truncated, shortened, rounded or rewritten to fit: a name, label or value the
file cannot carry fails the generation, and the generation publishes nothing.

## Records

<a id="xpt-2"></a>

### XPT-2. The file is a fixed sequence of 80-byte records in the transport layout

The file is a sequence of 80-byte records: the library header, two real
headers, the member and descriptor headers, two member records, the NAMESTR
header, one 140-byte NAMESTR per variable, the observation header and the
observations. The NAMESTRs and the observations each run across record
boundaries, and each run is padded with blanks to a whole record. Integers are
big-endian. The same inputs and creation time produce the same bytes.

Evidence: `sdtm-dm-xpt`, `tools/tests/test_xpt_oracle.py`, `rust/crates/yamaa-application/tests/submission_transport.rs::written_transport_files_read_back_through_the_xport_reader`.

Each header record is `HEADER RECORD*******`, the record kind in eight
blank-padded bytes, `HEADER RECORD!!!!!!!`, the counts below and two blanks:

| Record | Kind | Counts |
|---|---|---|
| library header | `LIBRARY` | 30 `0` |
| member header | `MEMBER` | `000000000000000001600000000140` |
| descriptor header | `DSCRPTR` | 30 `0` |
| NAMESTR header | `NAMESTR` | `000000`, the variable count in four digits, 20 `0` |
| observation header | `OBS` | 30 `0` |

The observations follow their header directly, each the concatenation of its
variables' cells in `output.columns` order, with no separator. A file has no
observation records when its dataset has no rows, which
[JSON-3](dataset-json.md#json-3) refuses.

<a id="xpt-3"></a>

### XPT-3. The header records carry fixed text, the dataset's name and label, and the creation time

The first real header is `SAS`, `SAS`, `SASLIB`, release `9.4` and system
`yamaa`, each in eight blank-padded bytes, then 24 blanks and the creation
time; the second real header is the modification time. The first member record
is `SAS`, the dataset name, `SASDATA`, `9.4` and `yamaa` the same way, 24
blanks and the creation time; the second is the modification time, 16 blanks,
the dataset's `submission.label` in 40 bytes and a blank dataset type. Both
times are the selected `created`.

Evidence: `sdtm-dm-xpt`, `adam-adsl-xpt`, `tools/tests/test_xpt_oracle.py`.

The creation and modification times are the one `created` that
[DEFINE-3](define-xml.md#define-3) selects, written `ddMMMyy:hh:mm:ss` with the
month upper case: `2026-09-11T00:00:00` is `11SEP26:00:00:00`. Every record is
blank padded to 80 bytes.

<a id="xpt-4"></a>

### XPT-4. Each output column produces one NAMESTR, in `output.columns` order

Each `output.columns` entry produces one 140-byte NAMESTR, in that order,
carrying its variable type, a zero hash, its width, its one-based number, its
name and label, its display format under [XPT-9](#xpt-9), zero justification,
a blank informat with zero width and decimals, its byte position in the
observation and zero filler. No informat is ever written.

Evidence: `tools/tests/test_xpt_oracle.py`, `rust/crates/yamaa-application/tests/submission_transport.rs::written_transport_files_read_back_through_the_xport_reader`.

| Bytes | Field | Value |
|---|---|---|
| 0-1 | type | `1` numeric, `2` character |
| 2-3 | hash | `0` |
| 4-5 | width | bytes in an observation, [XPT-6](#xpt-6) |
| 6-7 | number | one-based position in `output.columns` |
| 8-15 | name | the column name, blank padded |
| 16-55 | label | the column's `label`, blank padded |
| 56-63 | format name | [XPT-9](#xpt-9), blank padded; blanks without a format |
| 64-65 | format width | [XPT-9](#xpt-9); `0` without a format |
| 66-67 | format decimals | [XPT-9](#xpt-9); `0` without a format |
| 68-71 | justification and fill | `0` |
| 72-79 | informat name | blanks |
| 80-83 | informat width and decimals | `0` |
| 84-87 | position | the sum of the widths of the variables before it |
| 88-139 | filler | `0` |

## Names, types and values

<a id="xpt-5"></a>

### XPT-5. Names are names DEFINE-22 admits, and labels are at most 40 printable ASCII characters

The member name is the dataset name and each variable name its column name,
and each must be a name [DEFINE-22](define-xml.md#define-22) admits:
`^[A-Z][A-Z0-9]{0,7}$` for a dataset and `^[A-Z][A-Z0-9_]{0,7}$` for a column.
The dataset's `submission.label` and each column's `label` are at most 40
characters of printable ASCII, `0x20` through `0x7E`. A name or label that does
not fit fails before any data is read, under `transport: xpt` in every context.

Fails: `sas_name_too_long`, `transport_label_invalid`.
Evidence: `negative-transport-name-too-long`, `negative-transport-label-too-long`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_label_beyond_forty_characters_cannot_travel_by_xpt`.

`context: Submission` imposes the same names and labels on every composed
dataset, with or without a transport ([DEFINE-4](define-xml.md#define-4)). A
name failure reports the name and the pattern it fails. A label failure
reports `transport_label_invalid` at the label's position with the dataset, the
column for a column label, the label, and the cause `ascii` or `length`;
`ascii` is reported first when both hold.

<a id="xpt-6"></a>

### XPT-6. A `str` column is a character variable, a number a numeric one, and a date travels as its model submits it

A `str` column is a character variable, and an `int` or `float` column a
numeric variable. Under `ADaM`, a `date` column is a numeric variable holding
days since 1960-01-01 and a `datetime` column one holding seconds since
1960-01-01T00:00:00 ([SUB-25](submission.md#sub-25)); under `SDTM` and `SEND`,
each is a character variable holding its canonical text
([VALUE-17](../values.md#value-17)). A character variable is as wide as
[SUB-24](submission.md#sub-24) measures it, and a numeric one is 8 bytes.

Evidence: `sdtm-dm-xpt`, `adam-adsl-xpt`.

A SAS date and datetime count from midnight on 1960-01-01 on the same wall
clock the value names; a value before the origin is negative. The canonical
text of an `SDTM` or `SEND` date is 10 bytes, and such a dataset carries no
`datetime` column ([SUB-27](submission.md#sub-27)).

<a id="xpt-7"></a>

### XPT-7. A character value is printable ASCII of at most 200 bytes without a trailing blank, and missing is blanks

A character value is written left aligned and padded with blanks to its
variable's width, and a missing value is all blanks. A present value must be
printable ASCII of at most 200 bytes, must not end in a blank and must not be
the empty string: in a transport file a trailing blank is padding and an empty
string is missing. A value that breaks one of these fails, reporting the
dataset, column, row, value and cause.

Fails: `transport_text_invalid`.
Evidence: `negative-transport-non-ascii`, `negative-transport-empty-string`, `negative-transport-trailing-blank`, `rust/crates/yamaa-core/src/submission_transport.rs::text_causes_follow_one_order_and_only_xpt_refuses_padding_lookalikes`.

The failure reports `transport_text_invalid` at
`datasets.<dataset>.columns.<column>` with the row's zero-based position in the
artifact, its key values and one cause, checked in this order:

| Cause | Value |
|---|---|
| `empty_string` | the present empty string |
| `ascii` | a scalar outside `0x20` through `0x7E` |
| `length` | more than 200 bytes |
| `trailing_blank` | a final `0x20` |

A leading blank is kept. Under `context: Submission` with any transport, the
`ascii` and `length` causes also hold for a Dataset-JSON file
([DEFINE-4](define-xml.md#define-4)), which carries an empty string and a
trailing blank unchanged.

<a id="xpt-8"></a>

### XPT-8. A numeric value is an IBM System/360 double converted exactly from binary64, and missing is a period

A numeric value is eight bytes of IBM System/360 floating point: a sign bit, a
seven-bit excess-64 hexadecimal exponent and a 56-bit fraction in [1/16, 1),
converted exactly from the binary64 value, with zero of either sign written as
eight zero bytes. A missing value is `0x2E` followed by seven `0x00` bytes, and
no special missing value is written. A magnitude of at least 16^63, a nonzero
one below 16^-65, an `int` whose magnitude exceeds 2^53, and a date or
datetime before 1582-01-01 fail.

Fails: `xpt_value_out_of_range`.
Evidence: `adam-adsl-xpt`, `rust/crates/yamaa-adapters/src/submission_xpt.rs::ibm_doubles_are_exact_and_normalized`, `rust/crates/yamaa-core/src/submission_transport.rs::number_causes_bound_exact_integers_and_ibm_magnitudes`, `negative-transport-date-before-1582`.

Every binary64 value in range converts exactly: its 53 significant bits fit the
56-bit fraction whatever leading zero bits the first hexadecimal digit has. An
`int` is converted through its exact binary64 value. The failure reports
`xpt_value_out_of_range` at `datasets.<dataset>.columns.<column>` with the row,
its key values, the value and the cause `magnitude`, `integer` or `date`. SAS
dates begin on 1582-01-01, and a date or datetime the `ADaM` model submits as
a number must not precede it.

<a id="xpt-9"></a>

### XPT-9. A numeric variable carries a display format only when its declared one parses as a SAS format

An `int` or `float` column's declared `display_format` is written in its
NAMESTR when it parses as a SAS format: a name of at most eight upper-case
letters, digits and underscores that begins and ends with a letter or an
underscore, an optional width, `.`, and optional decimals, the name empty only
beside a width. A declared format that does not parse fails under
`transport: xpt`. An
`ADaM` date carries `DATE9.` and a datetime `DATETIME20.`; a character variable
carries no format.

Fails: `xpt_display_format_invalid`.
Evidence: `adam-adsl-xpt`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_numeric_display_format_under_xpt_must_parse_as_a_sas_format`.

Width and decimals are decimal integers without a leading zero, at most 32767,
and decimals may be `0`. `8.2` is the empty name with width 8 and 2 decimals,
`BEST12.` the name `BEST` with width 12, and `E8601DA10.` the name `E8601DA`
with width 10. `TIME8.` is the name `TIME` with width 8, the display of an
`int` column holding seconds since midnight, such as an ADaM `*TM` time
([TIME-20](../temporal.md#time-20)); the NAMESTR carries it as it carries
any other numeric format. A value-level `display_format` is described in the document but
not written in the file, which has one format per variable. The failure
reports the dataset, the column and the declared format at
`<section>.specs[<n>].columns.<column>.submission.display_format`, before any
data is read.

<a id="xpt-10"></a>

### XPT-10. A file above 5,000,000,000 bytes, or one ending in an all-blank observation, is refused before writing

The file's size is computed before any byte is written: 640 header bytes, the
NAMESTRs padded to whole records, the 80-byte observation header and the
observations padded to whole records. A size above 5,000,000,000 bytes fails.
An artifact whose last row encodes to blanks only, every variable character
and every value missing, fails: a reader drops trailing all-blank observations
as the padding of the final record.

Fails: `xpt_file_too_large`, `xpt_last_row_blank`.
Evidence: `rust/crates/yamaa-adapters/tests/submission_encoders.rs::a_blank_last_row_or_a_file_past_five_billion_bytes_is_refused_before_writing`, `rust/crates/yamaa-adapters/src/submission_xpt.rs::file_size_pads_namestrs_and_observations_to_whole_records`, `rust/tests/host-fixtures/cases.json::a_composed_dataset_path_has_no_location`.

Both failures report the dataset and the artifact at
`datasets.<dataset>.artifact`, the size with `bytes` and the row with `row`. A
numeric variable never encodes to blanks, so only a dataset whose variables are
all character can end in a blank observation.
