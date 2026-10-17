# Dataset-JSON

This file is a profile: it is optional, and it applies only to a build whose
selected submission section of a study environment sets `dataset_json`. Such a
build writes one deterministic Dataset-JSON 1.1 file per composed
specification, carrying the published dataset and the submission metadata the
[Define-XML](define-xml.md) document is generated from. A domain build never
writes Dataset-JSON.

## Requesting Dataset-JSON

<a id="json-1"></a>

### JSON-1. A section's `dataset_json` requests one Dataset-JSON file beside each listed specification's artifact

The selected environment submission section's `dataset_json` boolean requests
one Dataset-JSON file for each specification in its ordered `specs`. Each
target is beside that specification's already published `output.path`, with
the same basename and a `.json` extension. Omission or false produces only the
Define-XML document. The submission section requests the files, and the
extension mapping of [Artifact publication](../storage/publication.md) stays
closed at `.csv` and `.parquet`.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::entry_relative_inherited_output_supplies_json_capture_and_publication_targets`, `python/tests/installed/installed_public_define.py::test_missing_artifact_only_when_requested_and_semantic_failure_zero_writes`.
Replaces: REQ-1193, REQ-1194.

Package references use `/` separators and satisfy [JSON-8](#json-8). A
Dataset-JSON file carries `studyOID`, `metaDataVersionOID`, `itemGroupOID`, a
per-column `itemOID` and a creation timestamp. The composition derives those
facts from the study, the selected section and the specification domain, and
[DEFINE-3](define-xml.md#define-3) selects the explicit creation time or the
current UTC time once during preparation.

<a id="json-2"></a>

### JSON-2. Every Dataset-JSON target has exactly one writer

Every Dataset-JSON target must differ from the selected section's `define`
target, every other Dataset-JSON target and every composed specification's
published `output.path`. Each file has one writer. A derived Dataset-JSON
target equal to one of those targets fails validation, reporting the
conflicting targets.

Fails: `dataset_json_path_collision`, `artifact_path_collision`.
Evidence: `rust/crates/yamaa-adapters/tests/file_resources.rs::case_alias_targets_are_refused_before_overwriting_a_sentinel`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::DefineDatasetJsonCollision`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::DefineArtifactCollision`.
Replaces: REQ-1195, REQ-1226.

A derived Dataset-JSON target equal to the selected section's `define` target,
another Dataset-JSON target or a composed specification's `output.path` fails
with `dataset_json_path_collision`.

<a id="json-3"></a>

### JSON-3. A dataset declared to have no data is not requested as Dataset-JSON

A selected specification whose dataset submission metadata declares
`has_no_data` must not be requested as Dataset-JSON. Such a request fails
validation, reporting the dataset.

Fails: `dataset_json_without_data`.
Evidence: `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionDatasetJsonNoData`.
Replaces: REQ-1196, REQ-1227.

<a id="json-4"></a>

### JSON-4. The file is Dataset-JSON 1.1 in its single-object form and valid against that standard's schema

The generated file conforms to Dataset-JSON 1.1, and `datasetJSONVersion` is
exactly `1.1.0`. The newline-delimited form the standard also defines is not
this profile. A generated file that is not valid against the Dataset-JSON 1.1
schema fails, reporting the violation, and the generation publishes nothing.

Fails: `value_not_permitted`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-core/tests/support/diagnostic/submission.rs::SubmissionJsonInvalid`, `python/tests/installed/installed_public_define.py::test_dm_goldens_public_issues_and_held_save`.
Replaces: REQ-1197, REQ-1198, REQ-1231.

A later release of the standard is a new profile under a new name rather than a
redefinition of this one. The newline-delimited form carries the same
information in a second spelling, and this profile fixes one. A schema-invalid
file fails with `value_not_permitted`, and the generation publishes nothing,
exactly as [DEFINE-25](define-xml.md#define-25) requires of an invalid
document.

## Content

<a id="json-5"></a>

### JSON-5. The rows are the published artifact's rows, read the way a consumer reads a producer's artifact

The file's rows are the rows of the artifact the entry's specification
published at its `output.path`, read exactly as a consuming specification reads
a producer's artifact, and the producer completes before the document is
generated. Composition does not re-derive, re-order, filter or round: column
order is `output.columns`, row order is the artifact's, and a value is the
value the artifact carries. An artifact that is unpublished or does not match
its producing specification fails.

Fails: `producer_contract_mismatch`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::mutual_references_and_published_contract_fail_before_any_publication`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.
Replaces: REQ-1199, REQ-1200, REQ-1230.

The artifact is read:

- under the profile [REQ-0751](../storage/ingestion.md#req-0751) says the
  producer wrote it with;
- with the field agreement [REQ-0522](../storage/ingestion.md#req-0522)
  requires; and
- with the typed values [REQ-0524](../storage/ingestion.md#req-0524)
  delivers.

`output.decimals` is a `csv` display precision under
[CSV-6](../reference/csv.md#csv-6) and does not reach this file, exactly as it
does not reach `parquet` under [PARQUET-3](../reference/parquet.md#parquet-3).

An artifact that has not been published, or whose fields, order or types do not
match the producing specification's output contract, fails under
[REQ-0535](../storage/ingestion.md#req-0535) with
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
Replaces: REQ-1201, REQ-1202.

There is no case in which the column block and the Define-XML disagree and a
rule has to say which one wins. This is the posture of
[SUB-2](submission.md#sub-2) applied to a container: what a governed field owns
cannot also be written somewhere else, and a second declaration is refused
rather than reconciled. [DEFINE-7](define-xml.md#define-7) requires both of a
composed specification and fails the document that lacks them.

<a id="json-7"></a>

### JSON-7. The file is one JSON object carrying the standard's members in a fixed order, omitting absent ones

The file is one JSON object carrying the members the table below lists, in that
order, which is the order the standard presents them. A member whose source is
absent is omitted and is never written as `null`: within this file `null` is
the missing value of a row. `rows` is written even when the dataset has no
rows, as `[]`. `dbLastModifiedDateTime` and `targetDataType` are never
written.

Evidence: `sdtm-dm-metadata`, `python/tests/installed/installed_public_define.py::test_source_version_defaults_and_schema_facet_failures`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.
Replaces: REQ-1203, REQ-1204, REQ-1205, REQ-1207, REQ-1208.

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
under [FUNC-2](functions.md#func-2). The same selected version is used in
Define-XML and Dataset-JSON.

`targetDataType` asks a receiving system to convert a transmitted value into
another logical type; every value here is written in the spelling its own type
fixes.

<a id="json-8"></a>

### JSON-8. `metaDataRef` names the Define-XML document relative to the file, without leaving its directory

`metaDataRef` is the selected section's `define` target expressed relative to
the directory holding the Dataset-JSON file, written with `/` separators, and
must not begin with a parent traversal. A `dataset_json` path, or a
`metaDataRef` computed from it, that cannot be expressed relative to its
directory without a parent traversal fails, reporting the dataset and both
paths.

Fails: `artifact_outside_document`.
Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::mutual_references_and_published_contract_fail_before_any_publication`, `rust/crates/yamaa-adapters/tests/file_resources.rs::inherited_original_output_supplies_both_json_capture_and_publication_target`.
Replaces: REQ-1206, REQ-1229.

The pair travels together: the file names the document its `itemOID`s point
into, and [JSON-12](#json-12) has that document name the file. The failure
reports `artifact_outside_document` as [DEFINE-16](define-xml.md#define-16)
does for the artifact a leaf names.

<a id="json-9"></a>

### JSON-9. Each output column produces one column object carrying that column's Define-XML metadata

Each `output.columns` entry produces one `columns` object carrying the members
the table below lists, in that order. Every member is the one the Define-XML
document carries for the same column: `itemOID` is the `ItemDef` identifier
[DEFINE-19](define-xml.md#define-19) generates, `length` and `displayFormat`
are its `Length` and `def:DisplayFormat`, and `keySequence` is the `ItemRef`
`KeySequence` [DEFINE-18](define-xml.md#define-18) writes.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::integer_terminology_items_admit_equal_float_published_values`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.
Replaces: REQ-1209.

| Member | Source |
|---|---|
| `itemOID` | the column's generated identifier |
| `name` | the column name |
| `label` | the column's `label` |
| `dataType` | per [JSON-10](#json-10) |
| `length` | `submission.length`, when [Submission metadata](submission.md) admits one |
| `displayFormat` | `submission.display_format`, when declared |
| `keySequence` | one-based position in `keys`; omitted when not a key |

`keySequence` therefore describes the same non-missing identity in `keys`; it
is not independently declarable. Sponsor ordering and a separate `unique`
verification, including combinations with missing values, follow the
[output identity example](../storage/publication.md#req-0221) and do not
change this metadata.

<a id="json-10"></a>

### JSON-10. `dataType` maps the resolved submission data type through a closed table

`dataType` maps the resolved submission data type of
[Submission metadata](submission.md) through the closed table below. The table
is not a second type system: Dataset-JSON's set is smaller than Define-XML's,
so the eight collapsed types are carried as text here and keep their
submission type in the `ItemDef` the column's `itemOID` points at. `boolean`,
`decimal` and `double` are never written.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::integer_terminology_items_admit_equal_float_published_values`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.
Replaces: REQ-1210, REQ-1211.

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

A reader that needs the finer type reads the document. `float` is written for a
`float` column even though the value of
[Types and conversion](../values/types.md) is binary64; this member must equal
the `DataType` that `ItemDef` carries, and the storage width of a number is not
what it names. [Types and conversion](../values/types.md) declares no Boolean
column type, and neither `decimal` nor `double` names a submission type
[Submission metadata](submission.md) resolves.

<a id="json-11"></a>

### JSON-11. Each row is an array of its values written by column type, with missing as `null`

Each row is a JSON array holding one element per `columns` entry, in that
order, written by the column's declared type as the table below fixes. A
temporal value is its canonical text and never a count from an epoch. A missing
value is `null` and a collected empty string is `""`; which of them reaches
this file is decided by the artifact's own container and by nothing else.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/src/submission_json.rs::streamed_cells_preserve_exact_json_and_full_width_scalar_spelling`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.
Replaces: REQ-1212, REQ-1213, REQ-1214.

| Column type | Written as |
|---|---|
| missing, any type | `null` |
| `str` | a JSON string carrying the value's scalar values, under [Text values](../values/text.md) |
| `int` | a JSON number, its decimal digits |
| `float` | a JSON number, [Numeric values](../values/numbers.md)'s float text |
| `date` | a JSON string, [Temporal values](../values/temporal.md)'s canonical `date` text |
| `datetime` | a JSON string, [Temporal values](../values/temporal.md)'s canonical `datetime` text |

The epoch counts of the [Parquet profile](../reference/parquet.md) are a
property of that container and are not a second spelling here. A `parquet`
artifact's zero-length string arrives as a present empty string. A `csv`
artifact's empty field arrives as missing under
[REQ-0529](../storage/ingestion.md#req-0529). The empty-string convention of
[REQ-1158](../storage/ingestion.md#req-1158) does not apply here.

## Serialization and publication

<a id="json-12"></a>

### JSON-12. When Dataset-JSON is requested, each dataset's leaf names its JSON file

When the selected section requests `dataset_json`, each dataset's `def:leaf`
names that dataset's JSON target: its `xlink:href` and `def:title` use the JSON
file rather than the published CSV or Parquet path. When Dataset-JSON is not
requested, the leaf names the specification's published `output.path`.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::entry_relative_inherited_output_supplies_json_capture_and_publication_targets`, `python/tests/installed/installed_saved_readers.py::test_public_held_saves_cover_every_type_and_empty_table`.
Replaces: REQ-1215.

<a id="json-13"></a>

### JSON-13. The file's bytes are fixed: UTF-8, one member per line, one spelling per string and number

The file is encoded UTF-8 and carries no byte-order mark. `U+000A` terminates
every line, including the last, and `U+000D` is never written outside a string
escape. The layout, the string escapes and the number spellings are fixed
exactly as the lists below state, leaving exactly one spelling of every part of
a file. A generation's data files are byte-identical for the same inputs, and a
golden file is compared byte for byte.

Evidence: `sdtm-dm-metadata`, `rust/crates/yamaa-adapters/tests/file_resources.rs::dm_both_goldens_without_input_execution_and_save_uses_held_bytes`, `rust/crates/yamaa-adapters/src/submission_json.rs::streamed_cells_preserve_exact_json_and_full_width_scalar_spelling`.
Replaces: REQ-1216, REQ-1217, REQ-1218, REQ-1219, REQ-1220.

The layout is fixed exactly:

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

Within a string, `U+0022` is written `\"`, `U+005C` is written `\\`, and
`U+0008`, `U+000C`, `U+000A`, `U+000D`, and `U+0009` are written `\b`, `\f`,
`\n`, `\r`, and `\t`. Every other scalar below `U+0020` is written as `\u00`
and two lowercase hexadecimal digits. Nothing else is escaped: `U+002F` is
written bare, and every other Unicode scalar value is written as itself in
UTF-8.

No number is written with an exponent, a leading `U+002B`, digit grouping, or a
leading zero. An `int` is the integer spelling of
[CSV-5](../reference/csv.md#csv-5) and a `float` is the float text of
[REQ-0018](../values/numbers.md#req-0018), which is positional and shortest.

A generation's data files meet the byte-identity rule of
[DEFINE-23](define-xml.md#define-23).

<a id="json-14"></a>

### JSON-14. Data files are published before the document, and only after every file renders and validates

A generation renders and validates every Dataset-JSON file and the Define-XML
document before touching any target. It publishes the data files first, in
entry order, and the document last, applying
[REQ-0752](../storage/publication.md#req-0752) through
[REQ-0754](../storage/publication.md#req-0754) to each file. A failure at any
point publishes nothing further and leaves every untouched target as it was.

Fails: `publication_failed`.
Evidence: `rust/crates/yamaa-adapters/src/file_define/publication_failure_tests.rs::later_publication_failure_preserves_prior_bytes_original_cause_and_stopped_remainder`, `rust/crates/yamaa-adapters/tests/file_resources.rs::semantic_failure_and_target_admission_have_zero_writes`, `rust/crates/yamaa-adapters/tests/file_resources.rs::dm_both_goldens_without_input_execution_and_save_uses_held_bytes`.
Replaces: REQ-1221, REQ-1222, REQ-1232.

A visible document therefore never points at a data file that is not there yet,
which is the order of [REQ-0757](../storage/publication.md#req-0757) applied to
this pair. Atomic replacement is guaranteed per file and not across the
package, exactly as [REQ-0758](../storage/publication.md#req-0758) states for a
primary artifact and its log. A failed publication fails under
[REQ-0764](../storage/publication.md#req-0764) and this rule.

## Reading

<a id="json-15"></a>

### JSON-15. A Dataset-JSON file is never read as a source

This container is not a source. The extension mapping of
[Source ingestion](../storage/ingestion.md) stays closed at `.csv` and
`.parquet`, and a `dataset_class.path` ending in `.json` names no profile.

Replaces: REQ-1223.
