---
id: submission/dataset-json
title: Dataset-JSON
status: normative
---

# Dataset-JSON

## Requirements

### Where a Dataset-JSON file is declared

<a id="req-1193"></a>

**REQ-1193.** A `datasets` entry's `dataset_json` names the Dataset-JSON file
the study document produces for that dataset. The field is optional: a
document that declares none produces its Define-XML document alone, and a
document that declares one for some entries produces one for exactly those.
The path is relative to the directory holding the generated document, is
written with `/` separators, must not begin with a parent traversal, and ends
in `.json`, matched without regard to case.

<a id="req-1194"></a>

**REQ-1194.** The study document declares it, and [Artifact publication](../storage/publication.md)'s extension
mapping stays closed at `.csv` and `.parquet`. A Dataset-JSON file carries
`studyOID`, `metaDataVersionOID`, `itemGroupOID`, a per-column `itemOID`, and
a creation timestamp. Every one of those is a study-document fact: [REQ-0970](define-xml.md#req-0970)
builds the identifiers from the document's `study`, `metadata_version`, and
dataset `id`, and [REQ-0962](define-xml.md#req-0962) declares the timestamp rather than reading a clock.
A specification holds none of them. An `output.path` ending in `.json` would
therefore either mint identifiers a document can contradict or require every
specification to restate the study it belongs to, and [REQ-0960](define-xml.md#req-0960) already fixes
where composition is declared. The exclusion is not [REQ-1234](../storage/publication.md#req-1234)'s:
this container loses nothing, and [REQ-1233](../storage/publication.md#req-1233)
records that it is the identity rather than the values that keeps it out.

<a id="req-1195"></a>

**REQ-1195.** A `dataset_json` path must differ from the document's
`output.path`, from every other entry's `dataset_json`, and from the
`output.path` of every specification the document composes. Each file has one
writer, and a path with two is a document that overwrites its own package.

<a id="req-1196"></a>

**REQ-1196.** An entry declaring `has_no_data` must not declare
`dataset_json`. The entry states that the dataset holds no records, and a
data file beside it would assert the opposite. [REQ-0989](define-xml.md#req-0989) already requires that
entry to say in a comment why the dataset is empty.

### Version

<a id="req-1197"></a>

**REQ-1197.** The generated file conforms to Dataset-JSON 1.1, and
`datasetJSONVersion` is exactly `1.1.0`. A later release of the standard is a
new profile under a new name rather than a redefinition of this one, for the
reason [REQ-0721](../storage/publication.md#req-0721) gives: a file keeps the meaning its producer's version gave
it, and a reader that resolved `1.1.0` against a later mapping would read a
conforming file wrongly without failing.

<a id="req-1198"></a>

**REQ-1198.** The newline-delimited form the standard also defines is not this
profile. It carries the same information in a second spelling, and this
contract fixes one.

### The data the file carries

<a id="req-1199"></a>

**REQ-1199.** The file's rows are the rows of the artifact the entry's
specification published at its `output.path`, read exactly as a consuming
specification reads a producer's artifact: under the profile [REQ-0751](../storage/ingestion.md#req-0751) says the
producer wrote it with, with the field agreement [REQ-0522](../storage/ingestion.md#req-0522) requires and the
typed values [REQ-0524](../storage/ingestion.md#req-0524) delivers. The producer completes before the document is
generated.

<a id="req-1200"></a>

**REQ-1200.** Composition does not re-derive, re-order, filter, or round.
Column order is `output.columns`, row order is the artifact's, and a value is
the value the artifact carries. `output.decimals` is a `csv` display
precision under [REQ-0744](../storage/csv.md#req-0744) and does not reach this file, exactly as it does not
reach `parquet` under [REQ-0743](../storage/parquet.md#req-0743).

### Metadata provenance

<a id="req-1201"></a>

**REQ-1201.** Every metadata member of the file is generated from the same
declarations the Define-XML document is generated from. Nothing in this file
is declared a second time, so there is no case in which the column block and
the Define-XML disagree and a rule has to say which one wins. This is
[REQ-0857](metadata.md#req-0857)'s posture applied to a container: what a governed field owns cannot
also be written somewhere else, and a second declaration is refused rather
than reconciled.

<a id="req-1202"></a>

**REQ-1202.** A specification a document composes already declares
`root.submission`, and every one of its output columns already declares a
`label` and a `column.submission`. [REQ-0967](define-xml.md#req-0967) requires both of a composed
specification and [REQ-1015](define-xml.md#req-1015) fails the document that lacks them. A specification
with no submission metadata therefore produces no Dataset-JSON file, for the
same reason it produces no Define-XML entry.

### The file's members

<a id="req-1203"></a>

**REQ-1203.** The file is one JSON object carrying these members, in this
order, which is the order the standard presents them:

| Member | Source |
|---|---|
| `datasetJSONCreationDateTime` | `creation_datetime` |
| `datasetJSONVersion` | fixed `1.1.0` |
| `fileOID` | `file_oid`, a period, and the entry's `id` |
| `originator` | `originator`, when declared |
| `sourceSystem` | `source_system` and `source_system_version`, when declared |
| `studyOID` | the study's generated identifier |
| `metaDataVersionOID` | the metadata version's generated identifier |
| `metaDataRef` | the generated document, per [REQ-1206](dataset-json.md#req-1206) |
| `itemGroupOID` | the dataset's generated identifier |
| `records` | the number of rows |
| `name` | the entry's `id` |
| `label` | `submission.label` |
| `columns` | one entry per `output.columns` entry, in that order |
| `rows` | the artifact's rows, in artifact order |

<a id="req-1204"></a>

**REQ-1204.** A member whose source is absent is omitted. It is never written
as `null`: within this file `null` is the missing value of a row, and a member
present with no value would say that the document declared something empty
rather than declared nothing.

<a id="req-1205"></a>

**REQ-1205.** `rows` is written even when the dataset has no rows, as `[]`.
The standard makes the member optional; two admissible spellings of an empty
dataset would leave two conforming runtimes with different files.

<a id="req-1206"></a>

**REQ-1206.** `metaDataRef` is the document's `output.path` expressed relative
to the directory holding the Dataset-JSON file, written with `/` separators,
and must not begin with a parent traversal. The pair travels together: the
file names the document its `itemOID`s point into, and [REQ-1215](dataset-json.md#req-1215) has that
document name the file.

<a id="req-1207"></a>

**REQ-1207.** `sourceSystem` is one object carrying `name` and `version`, in
that order. The standard requires both. A document that declares
`source_system` without `source_system_version` therefore cannot write the
member; the declaration fails rather than silently producing a file with no
source system, and [REQ-0979](define-xml.md#req-0979) already refuses the reverse pair.

<a id="req-1208"></a>

**REQ-1208.** `dbLastModifiedDateTime` and `targetDataType` are never written.
No declaration states when a source database was last modified, and writing
the creation timestamp again in its place would assert something nobody
declared. `targetDataType` asks a receiving system to convert a transmitted
value into another logical type; every value here is written in the spelling
its own type fixes, and asking a reader to convert a value the file already
carries exactly would make the file say two things about one value.

### The column block

<a id="req-1209"></a>

**REQ-1209.** Each `output.columns` entry produces one `columns` object
carrying these members, in this order:

| Member | Source |
|---|---|
| `itemOID` | the column's generated identifier |
| `name` | the column name |
| `label` | the column's `label` |
| `dataType` | per [REQ-1210](dataset-json.md#req-1210) |
| `length` | `submission.length`, when [Submission metadata](metadata.md) admits one |
| `displayFormat` | `submission.display_format`, when declared |
| `keySequence` | one-based position in `keys`; omitted when not a key |

Every member is the one the Define-XML document carries for the same column:
`itemOID` is the `ItemDef` identifier [REQ-0992](define-xml.md#req-0992) generates, `length` and
`displayFormat` are its `Length` and `def:DisplayFormat`, and `keySequence` is
the `ItemRef` `KeySequence` [REQ-0990](define-xml.md#req-0990) writes.

<a id="req-1210"></a>

**REQ-1210.** `dataType` maps [Submission metadata](metadata.md)'s resolved submission data type
through this closed table:

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

<a id="req-1211"></a>

**REQ-1211.** The table is not a second type system. Dataset-JSON's set is
smaller than Define-XML's, so the eight collapsed types are carried as text
here and keep their submission type in the `ItemDef` the column's `itemOID`
points at; a reader that needs the finer type reads the document. `float` is
written for a `float` column even though [Types and conversion](../values/types.md)'s value is binary64,
This member must equal the `DataType` that `ItemDef` carries. The
storage width of a number is not what it names. `boolean`, `decimal`, and
`double` are never written: [Types and conversion](../values/types.md) declares no Boolean column type, and
neither of the other two names a submission type [Submission metadata](metadata.md) resolves.

### Values

<a id="req-1212"></a>

**REQ-1212.** Each row is a JSON array holding one element per `columns`
entry, in that order, written by the column's declared type:

| Column type | Written as |
|---|---|
| missing, any type | `null` |
| `str` | a JSON string carrying the value's scalar values, under [Text values](../values/text.md) |
| `int` | a JSON number, its decimal digits |
| `float` | a JSON number, [Numeric values](../values/numbers.md)'s float text |
| `date` | a JSON string, [Temporal values](../values/temporal.md)'s canonical `date` text |
| `datetime` | a JSON string, [Temporal values](../values/temporal.md)'s canonical `datetime` text |

<a id="req-1213"></a>

**REQ-1213.** A temporal value is its canonical text and never a count from an
epoch. Dataset-JSON 1.0 admitted a number whose origin it did not state, and
1.1 fixed the text form for exactly that reason. [Parquet profile](../storage/parquet.md)'s epoch counts are a
property of that container and are not a second spelling here.

<a id="req-1214"></a>

**REQ-1214.** A missing value is `null` and a collected empty string is `""`.
The two stay apart, as they do in [Parquet profile](../storage/parquet.md) under [REQ-1034](../storage/parquet.md#req-1034). Which of them
reaches this file is decided by the artifact's own container and by nothing
else: a `parquet` artifact's zero-length string arrives as a present empty
string, and a `csv` artifact's empty field arrives as missing under
[REQ-0529](../storage/ingestion.md#req-0529), so a study whose package must carry the distinction publishes its
artifact as `parquet`. [REQ-1158](../storage/ingestion.md#req-1158)'s empty-string convention does not apply here.
It is a property of an input a specification declares, and a composition
declares none; reading an artifact under a convention the study document
chose would let a package change a value the producer published.

### Serialization

<a id="req-1215"></a>

**REQ-1215.** When an entry declares `dataset_json`, the dataset's `def:leaf`
names that file: [REQ-0987](define-xml.md#req-0987)'s `xlink:href` and `def:title` are taken from the
Dataset-JSON path rather than from the specification's `output.path`. A
submission's document points at the file the submission carries. An entry
declaring no `dataset_json` is unchanged.

<a id="req-1216"></a>

**REQ-1216.** The file is encoded UTF-8 and carries no byte-order mark.
`U+000A` terminates every line, including the last. `U+000D` is never written
outside a string escape.

<a id="req-1217"></a>

**REQ-1217.** The layout is fixed exactly:

- the first line is `{` and the last line is `}`, both unindented;
- each member of the file's object is one line indented two spaces, written
  as its name in quotes, a colon, one space, and its value, with a comma
  after every member but the last;
- `sourceSystem` and each entry of `columns` is one object written on one
  line, opening `{`, its members separated by a comma and one space, each
  written as its name in quotes, a colon, one space, and its value, then `}`;
- each entry of `rows` is one array written on one line, opening `[`, its
  elements separated by a comma and one space, then `]`;
- `columns` and `rows` open with `[` at the end of their member's line, write
  one entry per line indented four spaces with a comma after every entry but
  the last, and close with `]` on its own line indented two spaces; an empty
  array is written `[]` on the member's line, and the comma separating such a
  member from the next follows its closing bracket; and
- no other whitespace is written: no trailing space, no blank line, and
  nothing between two tokens the rules above do not place there.

<a id="req-1218"></a>

**REQ-1218.** Within a string, `U+0022` is written `\"`, `U+005C` is written
`\\`, and `U+0008`, `U+000C`, `U+000A`, `U+000D`, and `U+0009` are written
`\b`, `\f`, `\n`, `\r`, and `\t`. Every other scalar below `U+0020` is written
as `\u00` and two lowercase hexadecimal digits. Nothing else is escaped:
`U+002F` is written bare, and every other Unicode scalar value is written as
itself in UTF-8.

<a id="req-1219"></a>

**REQ-1219.** No number is written with an exponent, a leading `U+002B`, digit
grouping, or a leading zero. An `int` is [REQ-0733](../storage/csv.md#req-0733)'s integer spelling and a
`float` is [REQ-0018](../values/numbers.md#req-0018)'s float text, which is positional and shortest and therefore
the one spelling two runtimes can both produce.

<a id="req-1220"></a>

**REQ-1220.** The bytes are fixed rather than the information. Two conforming
implementations produce byte-identical files, and a golden file is compared
byte for byte. The
layout above keeps a file readable in a diff, which is what a reviewer
compares, while leaving exactly one spelling of every part of it.

### Publication

<a id="req-1221"></a>

**REQ-1221.** A generation renders and validates every Dataset-JSON file and
the Define-XML document before touching any target. It publishes the data
files first, in entry order, and the document last, applying [REQ-0752](../storage/publication.md#req-0752) through
[REQ-0754](../storage/publication.md#req-0754) to each file. A visible document therefore never points at a data
file that is not there yet, which is [REQ-0757](../storage/publication.md#req-0757)'s order applied to this pair.

<a id="req-1222"></a>

**REQ-1222.** A failure at any point publishes nothing further and leaves
every untouched target as it was. Atomic replacement is guaranteed per file
and not across the package, exactly as [REQ-0758](../storage/publication.md#req-0758) states for a primary artifact
and its log.

### Reading

<a id="req-1223"></a>

**REQ-1223.** This container is not a source. [Source ingestion](../storage/ingestion.md)'s extension mapping
stays closed at `.csv` and `.parquet`, and a `dataset_class.path` ending in
`.json` names no profile.

### Interface behavior

<a id="req-1225"></a>

**REQ-1225.** The `dataset_json_path` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `dataset_json_path` | Relative file this dataset's data is written to, named with a json extension. |

## Error conditions

<a id="req-1226"></a>

**REQ-1226.** A `dataset_json` path equal to the document's `output.path`, to
another entry's `dataset_json`, or to a composed specification's
`output.path`: fail validation with `dataset_json_path_collision`, reporting
both declarations.

<a id="req-1227"></a>

**REQ-1227.** A `dataset_json` declared on an entry that declares
`has_no_data`: fail validation with `dataset_json_without_data`, reporting the
dataset.

<a id="req-1228"></a>

**REQ-1228.** A declared `source_system` with no `source_system_version`, in a
document that declares any `dataset_json`: fail validation with
`source_system_incomplete`.

<a id="req-1229"></a>

**REQ-1229.** A `dataset_json` path, or a `metaDataRef` computed from it, that
cannot be expressed relative to its directory without a parent traversal:
fail with `artifact_outside_document`, reporting the dataset and both paths,
as [REQ-1021](define-xml.md#req-1021) does for the artifact a leaf names.

<a id="req-1230"></a>

**REQ-1230.** An artifact that has not been published, or whose fields, order,
or types do not match the producing specification's output contract: fail
under [REQ-0535](../storage/ingestion.md#req-0535), reporting the dataset and the artifact path. The document
and its data files are generated from one composition, so a stale artifact
fails the generation rather than producing a package whose two halves
describe different datasets.

<a id="req-1231"></a>

**REQ-1231.** A generated file that is not valid against the Dataset-JSON 1.1
schema: fail and report the violation. The generation publishes nothing,
exactly as [REQ-1026](define-xml.md#req-1026) requires of an invalid document.

<a id="req-1232"></a>

**REQ-1232.** A failed publication: fail and report the target, exactly as
[Artifact publication](../storage/publication.md) does. Every file already published stays as it is, and every
other target is unchanged.
