# Dataset-JSON

This file is a profile: it is optional, and it applies only to a build whose
selected submission section of a project file sets `transport: dataset_json`
or `transport: dataset_ndjson`. Such a build writes one deterministic
Dataset-JSON 1.1 file per composed specification, in the single-object or the
newline-delimited representation the value selects ([JSON-4](#json-4)),
carrying the published dataset and the submission metadata the
[Define-XML](define-xml.md) document is generated from. A domain build never
writes Dataset-JSON. [JSON-1](#json-1), [JSON-2](#json-2),
[JSON-3](#json-3), [JSON-5](#json-5) and [JSON-14](#json-14) also govern the
files [SAS Transport](xpt.md) writes. [JSON-15](#json-15) states how a file
this profile writes reads back as an input.

## Requesting a transport file

<a id="json-1"></a>

### JSON-1. A section's `transport` requests one transport file per composed dataset, named for the dataset beside the document

The selected project submission section's optional `transport` requests one
transport file for each specification in its ordered `specs`: `dataset_json`
a Dataset-JSON file in its single-object form and `dataset_ndjson` one in its
newline-delimited form, both under this profile, and `xpt` a SAS Transport
version 5 file under [SAS Transport](xpt.md). Each file is in the directory
of the section's `define` target, named the lower-case dataset name with the
extension `.json`, `.ndjson` or `.xpt`. Omission produces only the Define-XML
document. The extension mapping of [OUTPUT-5](../output.md#output-5) stays
closed at `.csv` and `.parquet`.

Evidence: `sdtm-dm-metadata`, `schema-transport-dataset-ndjson`, `sdtm-dm-xpt`, `adam-adsl-xpt`, `rust/crates/yamaa-core/src/front_end/v1/project.rs::a_project_lists_function_files_and_holds_each_host_lock`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::a_transport_file_is_published_beside_the_document_wherever_its_dataset_is`, `python/tests/installed/installed_public_define.py::test_missing_artifact_only_when_requested_and_semantic_failure_zero_writes`.

One section requests one transport. The file's place depends on the document
and the dataset name alone, not on where the artifact is published: a `DM`
dataset beside `out/define.xml` is written to `out/dm.json`, `out/dm.ndjson`
or `out/dm.xpt`.
A section that requests a transport names its document `define.xml`
([DEFINE-2](define-xml.md#define-2)), so `metaDataRef` is `define.xml`
([JSON-8](#json-8)). A Dataset-JSON file carries `studyOID`,
`metaDataVersionOID`, `itemGroupOID`, a per-column `itemOID` and a creation
timestamp. The composition derives those facts from the study, the selected
section and the specification domain, and [DEFINE-3](define-xml.md#define-3)
selects the explicit creation time or the current UTC time once during
preparation.

<a id="json-2"></a>

### JSON-2. Every transport file has exactly one writer

Every transport file must differ from the selected section's `define` target,
every other transport file, every composed specification's published
`output.path` and every path a composed specification declares for an input.
Each file has one writer. A transport file equal to one of those paths fails
validation, as does a published artifact equal to the document, reporting the
conflicting paths.

Fails: `transport_path_collision`, `artifact_path_collision`.
Evidence: `rust/crates/yamaa-adapters/tests/file_resources.rs::transport_targets_equal_after_lowercasing_are_refused_before_overwriting_a_sentinel`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::a_transport_file_never_replaces_a_declared_input`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::two_listings_writing_one_transport_file_collide`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::two_listings_writing_one_artifact_collide`.

A transport file equal to another transport file or to a declared input path
fails with `transport_path_collision`, reporting the specification or the
input's position; two dataset names that differ only in case name one file. A
published artifact equal to a written target fails with
`artifact_path_collision`. The written targets are compared once more on the
filesystem before publication, so two spellings of one file are refused as
well.

<a id="json-3"></a>

### JSON-3. A dataset declared to have no data has no transport file, and a dataset without rows declares it

A composed specification whose dataset submission metadata declares
`has_no_data` is never written as a transport file, and its artifact is not
read. When a transport is requested, a composed dataset whose published
artifact has no rows and that does not declare `has_no_data` fails, reporting
the dataset and the artifact.

Fails: `transport_dataset_empty`.
Evidence: `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::a_dataset_declared_without_data_has_no_transport_file_and_no_leaf`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::a_dataset_without_rows_must_declare_it_has_no_data`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_published_dataset_without_rows_must_declare_it`, `negative-transport-empty-dataset`.

The document still describes a dataset with no data, with `def:HasNoData`; it
names no file for it ([DEFINE-15](define-xml.md#define-15)). The failure
reports `transport_dataset_empty` at `datasets.<dataset>.artifact` with the
dataset and the artifact.

<a id="json-4"></a>

### JSON-4. The file is Dataset-JSON 1.1 in the representation `transport` selects and valid against that standard's schema

The generated file conforms to Dataset-JSON 1.1, and `datasetJSONVersion` is
exactly `1.1.0`. `transport: dataset_json` selects the standard's
single-object representation and `transport: dataset_ndjson` its
newline-delimited one; the two carry the same members, rows and spellings
([JSON-7](#json-7), [JSON-13](#json-13)), and one section writes one of them.
A generated file that is not valid against the Dataset-JSON 1.1 schema fails,
reporting the violation, and the generation publishes nothing.

Fails: `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `schema-transport-dataset-ndjson`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::a_published_boolean_in_a_text_column_is_not_permitted`, `python/tests/installed/installed_public_define.py::test_dm_goldens_public_issues_and_held_save`.

A later release of the standard is a new profile under a new name rather than a
redefinition of this one. The standard defines its newline-delimited form as
the same dataset written one JSON text per line, for a receiver that reads a
large dataset a row at a time; the first line is the metadata object and each
later line one row. Both forms are this profile, so the representation is a
choice of the section and never a conversion outside the generation's
determinism. A schema-invalid file fails with `value_not_permitted`, and the
generation publishes nothing, exactly as
[DEFINE-25](define-xml.md#define-25) requires of an invalid document.

## Content

<a id="json-5"></a>

### JSON-5. The rows are the published artifact's rows, read the way a consumer reads a producer's artifact, and no display precision reaches them

A transport file's rows are the rows of the artifact the entry's specification
published at its `output.path`, read exactly as a consuming specification
reads a producer's artifact; generation never runs the specification.
Composition does not re-derive, re-order, filter or round: column order is
`output.columns`, row order is the artifact's, and a value is the value the
artifact carries, so a composed specification declares no `output.decimals`.
An unpublished artifact, one that does not match its producing specification,
or a composed `output.decimals` fails.

Fails: `producer_contract_mismatch`, `decimals_not_applicable`.
Evidence: `sdtm-dm-metadata`, `sdtm-dm-xpt`, `negative-transport-display-decimals`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/submission.json::display_decimals_are_not_applicable_under_a_transport`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::a_published_dataset_with_reordered_columns_fails_its_producer_contract`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::a_published_dataset_missing_a_declared_column_fails_its_producer_contract`, `rust/crates/yamaa-adapters/tests/file_resources.rs::a_published_value_outside_its_codelist_keeps_the_verification_phase_and_row_key`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.

The artifact is read:

- under the profile its `output.path` extension selects under
  [INPUT-1](../inputs.md#input-1);
- with the field agreement [INPUT-11](../inputs.md#input-11)
  requires; and
- with the typed values [INPUT-11](../inputs.md#input-11)
  delivers.

A `csv` artifact carries the display precision
[CSV-7](../reference/csv.md#csv-7) writes: under `output.decimals` each float
is fixed-point text rounded once, and read back it is that rounded value, which
would become the submitted value. A specification that a section requesting a
transport composes therefore declares no `output.decimals`. One that does
fails validation with `decimals_not_applicable` at
`<section>.specs[<n>].output.decimals`, reporting the dataset, the
specification as the section's `specs` writes it and the declared digits,
before any artifact is read. The submitted display is the column's
`submission.display_format` ([SUB-9](submission.md#sub-9)), which changes no
value, and a value the analysis rounds is rounded by its derivation. A
`parquet` artifact carries no display precision
([PARQUET-3](../reference/parquet.md#parquet-3)).

An artifact that has not been published, or whose fields, order or types do not
match the producing specification's output contract, fails under
[INPUT-11](../inputs.md#input-11) with
`producer_contract_mismatch`, reporting the dataset and the artifact path. The
document and its data files are generated from one composition, so a stale
artifact fails the generation rather than producing a package whose two halves
describe different datasets.

<a id="json-6"></a>

### JSON-6. Every metadata member comes from the declarations the Define-XML document is generated from

Every metadata member of the file is generated from the same declarations the
Define-XML document is generated from, and nothing in this file is declared a
second time. A specification a document composes already declares
`root.submission`, and every one of its output columns already declares a
`label` and a `column.submission`. A specification with no submission metadata
therefore produces no Dataset-JSON file, as it produces no Define-XML entry.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::dm_both_goldens_without_input_execution_and_save_uses_held_bytes`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.

There is no case in which the column block and the Define-XML disagree and a
rule has to say which one wins. This is the posture of
[SUB-2](submission.md#sub-2) applied to a container: what a governed field owns
cannot also be written somewhere else, and a second declaration is refused
rather than reconciled. [DEFINE-7](define-xml.md#define-7) requires both of a
composed specification and fails the document that lacks them.

<a id="json-7"></a>

### JSON-7. The file carries the standard's members in a fixed order, omitting absent ones: one object, or a first line and one row per line

Under `dataset_json` the file is one JSON object carrying the members the
table below lists, in that order, the order the standard presents them. Under
`dataset_ndjson` the first line is that object without `rows`, and each later
line is one entry of `rows`, in order ([JSON-13](#json-13)). A member whose
source is absent is omitted and is never written as `null`: within this file
`null` is the missing value of a row. A file always has rows
([JSON-3](#json-3)), and `dbLastModifiedDateTime` is never written.

Evidence: `sdtm-dm-metadata`, `schema-transport-dataset-ndjson`, `python/tests/installed/installed_public_define.py::test_source_version_defaults_and_schema_facet_failures`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.

| Member | Source |
|---|---|
| `datasetJSONCreationDateTime` | the selected creation time |
| `datasetJSONVersion` | fixed `1.1.0` |
| `fileOID` | the selected FileOID, a period, and the specification domain |
| `originator` | `originator`, when declared |
| `sourceSystem` | `yamaa` and the selected source-system version |
| `studyOID` | the study's generated identifier |
| `metaDataVersionOID` | the metadata version's generated identifier |
| `metaDataRef` | the generated document, per [JSON-8](#json-8) |
| `itemGroupOID` | the dataset's generated identifier |
| `records` | the number of rows |
| `name` | the specification's `domain`, per [DEFINE-5](define-xml.md#define-5) |
| `label` | `submission.label` |
| `columns` | one entry per `output.columns` entry, in that order |
| `rows` | the artifact's rows, in artifact order |

`sourceSystem` is one object carrying `name` and `version`, in that order. Its
name is `yamaa`; its version is the selected section's explicit
`source_system_version`, or the actual installed yamaa version when omitted,
under [DEFINE-12](define-xml.md#define-12). The same selected version is used
in Define-XML and Dataset-JSON.

A column's `targetDataType` is written only as [JSON-10](#json-10) states;
every value is written in the spelling its own type fixes.

<a id="json-8"></a>

### JSON-8. `metaDataRef` names the Define-XML document beside the file

`metaDataRef` is `define.xml`, the file name of the selected section's
`define` target, which [JSON-1](#json-1) places in the directory holding the
Dataset-JSON file and [DEFINE-2](define-xml.md#define-2) names. The reference
therefore never leaves that directory.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::a_transport_file_is_published_beside_the_document_wherever_its_dataset_is`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::inherited_original_output_supplies_both_json_capture_and_publication_target`.

The pair travels together: the file names the document its `itemOID`s point
into, and [JSON-12](#json-12) has that document name the file.

<a id="json-9"></a>

### JSON-9. Each output column produces one column object carrying that column's Define-XML metadata

Each `output.columns` entry produces one `columns` object carrying the members
the table below lists, in that order. Every member is the one the Define-XML
document carries for the same column: `itemOID` is the `ItemDef` identifier
[DEFINE-19](define-xml.md#define-19) generates, `length` and `displayFormat`
are its `Length` and `def:DisplayFormat`, and `keySequence` is the `ItemRef`
`KeySequence` [DEFINE-18](define-xml.md#define-18) writes.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::integer_terminology_items_admit_equal_float_published_values`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.

| Member | Source |
|---|---|
| `itemOID` | the column's generated identifier |
| `name` | the column name |
| `label` | the column's `label` |
| `dataType` | per [JSON-10](#json-10) |
| `targetDataType` | per [JSON-10](#json-10), when it applies |
| `length` | the `ItemDef` `Length`, when one applies ([SUB-24](submission.md#sub-24)) |
| `displayFormat` | the `ItemDef` `def:DisplayFormat`, when one applies |
| `keySequence` | one-based position in `keys`; omitted when not a key |

`keySequence` therefore describes the same non-missing identity in `keys`; it
is not independently declarable. Sponsor ordering and a separate `unique`
verification, including combinations with missing values, follow the
[output identity example](../output.md#output-3) and do not
change this metadata.

<a id="json-10"></a>

### JSON-10. `dataType` maps the resolved submission data type through a closed table, and an ADaM date targets an integer

`dataType` maps the resolved submission data type of
[Submission metadata](submission.md) through the closed table below; the eight
collapsed types are carried as text and keep their submission type in the
`ItemDef`. Under `adam`, a `date` or `datetime` the document describes as
`integer` ([SUB-25](submission.md#sub-25)) keeps its ISO value with `dataType`
`date` or `datetime` and `targetDataType` `integer`. No other column carries
`targetDataType`, and `boolean`, `decimal` and `double` are never written.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::adam_dataset_json_targets_the_integer_its_document_submits`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.

| Submission data type | `dataType` |
|---|---|
| `text` | `string` |
| `integer` | `integer` |
| `float` | `float` |
| `date` | `date` |
| `datetime` | `datetime` |
| `time` | `time` |
| `URI` | `URI` |
| `partialDate`, `partialTime`, `partialDatetime` | `string` |
| `incompleteDate`, `incompleteTime`, `incompleteDatetime` | `string` |
| `durationDatetime`, `intervalDatetime` | `string` |

`targetDataType` asks the receiving system to convert the ISO value into the
SAS date or datetime the document describes, and the column's `length` and
`displayFormat` are that `ItemDef`'s `8` and `DATE9.` or `DATETIME20.`. The
table is not a second type system: Dataset-JSON's set is smaller than
Define-XML's. A reader that needs the finer type reads the document. `float` is
written for a `float` column even though the value of [Values](../values.md) is
binary64; this member must equal the `DataType` that `ItemDef` carries, and the
storage width of a number is not what it names. [Values](../values.md) declares
no Boolean column type, and neither `decimal` nor `double` names a submission
type [Submission metadata](submission.md) resolves.

<a id="json-11"></a>

### JSON-11. Each row is an array of its values written by column type, with missing as `null`

Each row is a JSON array holding one element per `columns` entry, in that
order, written by the column's declared type as the table below fixes. A
temporal value is its canonical text and never a count from an epoch. A missing
value is `null` and a collected empty string is `""`; which of them reaches
this file is decided by the artifact's own container and by nothing else.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/src/submission_json.rs::streamed_cells_preserve_exact_json_and_full_width_scalar_spelling`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.

| Column type | Written as |
|---|---|
| missing, any type | `null` |
| `str` | a JSON string carrying the value's scalar values, under [Values](../values.md) |
| `int` | a JSON number, its decimal digits |
| `float` | a JSON number, [Values](../values.md)'s float text |
| `date` | a JSON string, [Values](../values.md)'s canonical `date` text |
| `datetime` | a JSON string, [Values](../values.md)'s canonical `datetime` text |

The epoch counts of the [Parquet profile](../reference/parquet.md) are a
property of that container and are not a second spelling here. A `parquet`
artifact's zero-length string arrives as a present empty string. A `csv`
artifact's empty field arrives as missing under
[INPUT-8](../inputs.md#input-8). The empty-string convention of
[INPUT-9](../inputs.md#input-9) does not apply here.

## Serialization and publication

<a id="json-12"></a>

### JSON-12. When Dataset-JSON is requested, each dataset's leaf names its Dataset-JSON file

When the selected section sets `transport: dataset_json` or
`transport: dataset_ndjson`, each dataset's `def:leaf` names that dataset's
`.json` or `.ndjson` file beside the document: its `xlink:href` and
`def:title` use that file name rather than the published CSV or Parquet
path. When no transport is requested, the leaf names the specification's
published `output.path` ([DEFINE-16](define-xml.md#define-16)).

Evidence: `sdtm-dm-metadata`, `schema-transport-dataset-ndjson`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::entry_relative_inherited_output_supplies_json_capture_and_publication_targets`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.

<a id="json-13"></a>

### JSON-13. The file's bytes are fixed: UTF-8, one layout per representation, one spelling per string and number

The file is encoded UTF-8 and carries no byte-order mark. `U+000A` terminates
every line, including the last, and `U+000D` is never written outside a string
escape. The layout of each representation, the string escapes and the number
spellings are fixed exactly as the lists below state, leaving exactly one
spelling of every part of a file. A generation's data files are byte-identical
for the same inputs, and a golden file is compared byte for byte.

Evidence: `sdtm-dm-metadata`, `schema-transport-dataset-ndjson`, `rust/crates/yamaa-adapters/tests/file_resources.rs::dm_both_goldens_without_input_execution_and_save_uses_held_bytes`, `rust/crates/yamaa-adapters/src/submission_json.rs::streamed_cells_preserve_exact_json_and_full_width_scalar_spelling`, `rust/crates/yamaa-adapters/tests/submission_transport.rs::the_newline_delimited_form_carries_the_single_object_forms_value`.

The layout of the single-object form is fixed exactly:

- the first line is `{` and the last line is `}`, both unindented;
- each member of the file's object is one line indented two spaces, written as
  its name in quotes, a colon, one space, and its value, with a comma after
  every member but the last;
- `sourceSystem` and each entry of `columns` is one object written on one line.
  The object opens with `{`, writes its members separated by a comma and one
  space, and closes with `}`. Each member is written as its name in quotes, a
  colon, one space, and its value;
- each entry of `rows` is one array written on one line, opening `[`, its
  elements separated by a comma and one space, then `]`;
- `columns` and `rows` open with `[` at the end of their member's line. They
  write one entry per line indented four spaces, with a comma after every entry
  but the last. They close with `]` on its own line indented two spaces. An
  empty array is written `[]` on the member's line. The comma that separates
  such a member from the next follows its closing bracket; and
- no other whitespace is written: no trailing space, no blank line, and nothing
  between two tokens the rules above do not place there.

The layout of the newline-delimited form is fixed exactly:

- the first line is the object of [JSON-7](#json-7) without `rows`, written on
  one line. It opens with `{`, writes its members separated by a comma and one
  space, each as its name in quotes, a colon, one space, and its value, and
  closes with `}`;
- within that line, `sourceSystem` and each entry of `columns` is one object
  written as the single-object form writes it, and `columns` is one array
  written on that line, opening `[`, its entries separated by a comma and one
  space, then `]`;
- each later line is one entry of `rows`, written exactly as the single-object
  form writes it, unindented, with no comma between lines, so the file has
  `records` plus one lines; and
- no other whitespace is written.

The two forms therefore differ only in their framing: the first line of the
newline-delimited form, with the later lines appended as its `rows`, is the
single-object form's value, token for token. The standard admits a `U+000D`
before each line's `U+000A`; this profile never writes one.

Within a string, `U+0022` is written `\"`, `U+005C` is written `\\`, and
`U+0008`, `U+000C`, `U+000A`, `U+000D`, and `U+0009` are written `\b`, `\f`,
`\n`, `\r`, and `\t`. Every other scalar below `U+0020` is written as `\u00`
and two lowercase hexadecimal digits. Nothing else is escaped: `U+002F` is
written bare, and every other Unicode scalar value is written as itself in
UTF-8.

No number is written with an exponent, a leading `U+002B`, digit grouping, or a
leading zero. An `int` is the integer spelling of
[CSV-5](../reference/csv.md#csv-5) and a `float` is the float text of
[VALUE-8](../values.md#value-8), which is positional and shortest.

A generation's data files meet the byte-identity rule of
[DEFINE-23](define-xml.md#define-23).

<a id="json-14"></a>

### JSON-14. Transport files are published before the document, and only after every file renders and validates

A generation renders and validates every transport file and the Define-XML
document before touching any target. It publishes the transport files first,
in entry order, and the document last, applying
[OUTPUT-8](../output.md#output-8) to each file. A failure at any point
publishes nothing further and leaves every untouched target as it was.

Fails: `publication_failed`.
Evidence: `rust/crates/yamaa-adapters/src/file_define/publication_failure_tests.rs::later_publication_failure_preserves_prior_bytes_original_cause_and_stopped_remainder`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/shared_define.json::a_semantic_failure_writes_nothing`, `rust/crates/yamaa-adapters/tests/file_resources.rs::a_target_refused_at_save_stops_the_composition_with_zero_writes`, `rust/crates/yamaa-adapters/tests/file_resources.rs::dm_both_goldens_without_input_execution_and_save_uses_held_bytes`.

A visible document therefore never points at a data file that is not there yet,
which is the order of [OUTPUT-10](../output.md#output-10) applied to
this pair. Atomic replacement is guaranteed per file and not across the
package, exactly as [OUTPUT-10](../output.md#output-10) states for a
primary artifact and its report file. A failed publication fails under
[OUTPUT-8](../output.md#output-8) and this rule.

## Reading

<a id="json-15"></a>

### JSON-15. A file this profile writes reads back through the Dataset-JSON source profiles as its published table

A file this profile writes, in either representation, is read as an input by
the `dataset_json` or `dataset_ndjson` source profile its extension selects
([INPUT-1](../inputs.md#input-1),
[DSJSON-1](../reference/dataset-json.md#dsjson-1)). It reads back as its
published table: the same fields in the same order, with the same types and
values, under `empty_string: present`. A `str` column the document submits as
`date` or `datetime` reads as that type unless the input declares it `str`
([DSJSON-2](../reference/dataset-json.md#dsjson-2)).

Evidence: `rust/crates/yamaa-adapters/tests/table_dataset_json_source.rs::the_writers_output_for_a_published_table_reads_back_as_that_table`, `rust/crates/yamaa-adapters/tests/table_dataset_json_source.rs::a_written_empty_string_and_a_text_date_read_back_under_the_input_s_declarations`, `rust/crates/yamaa-adapters/tests/table_dataset_json_source.rs::every_dataset_json_golden_reads_back_as_its_published_rows`.

Each `dataType` this profile writes ([JSON-10](#json-10)) supplies the type
the column was written from under the closed table of
[DSJSON-2](../reference/dataset-json.md#dsjson-2), and each value is written
in the spelling its type fixes ([JSON-11](#json-11), [JSON-13](#json-13)),
which [DSJSON-3](../reference/dataset-json.md#dsjson-3) reads back. A
collected empty string is written `""`; under the default `empty_string`
([INPUT-9](../inputs.md#input-9)) it reads back as missing, as from any
source. A column of one of the collapsed date and time types is written as
`string` and reads back as `str`.

Reading a file back makes Dataset-JSON no artifact profile: the extension
mapping of [OUTPUT-5](../output.md#output-5) stays closed at `.csv` and
`.parquet`, and only a submission's `transport` writes Dataset-JSON
([JSON-1](#json-1)).
