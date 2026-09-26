---
id: submission/define-xml
title: Define-XML
status: normative
---

# Define-XML

## Purpose

Compose study metadata into deterministic Define-XML 2.1 documents.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Schema language](../reference/schema-language.md).
- [Specification structure](../specification/structure.md).
- [Artifact publication](../storage/publication.md).
- [Dataset-JSON](dataset-json.md).
- [Submission metadata](metadata.md).
- [Controlled terminology](terminology.md).
- [Temporal values](../values/temporal.md).

## Requirements

### The study document

<a id="req-0959"></a>

**REQ-0959.** A **study document** is a separately validated entry point,
written against `schema_define.yaml`. It declares one study, one metadata
version, the standards, supporting documents, and codelists its datasets
share, the ordered datasets it represents, and the file it produces. A
specification remains a single-dataset contract and declares none of this.

<a id="req-0960"></a>

**REQ-0960.** The study document is the only place the composition is declared.
An implementation must not assemble a document from whatever specifications a
directory holds: which datasets one submission represents is a study decision,
and a directory listing is not one.

<a id="req-0961"></a>

**REQ-0961.** `output.path` names the file the document produces, and its
extension must be `.xml`. [Artifact publication](../storage/publication.md)'s extension mapping is not consulted: that
mapping selects a profile for a dataset artifact, and this artifact has one
form.

<a id="req-0962"></a>

**REQ-0962.** `creation_datetime` is declared rather than read from a clock.
Two runs of the same inputs must produce the same bytes, and a timestamp taken
at run time would make all runs differ. `file_oid` is declared for
byte-identical runs: an identity minted per run is not reproducible.

<a id="req-0963"></a>

**REQ-0963.** `context` states whether the document is used in a submission.
`Submission` adds the requirements [REQ-1004](define-xml.md#req-1004) lists. Nothing below is relaxed by
`Other`; that context only omits requirements a regulator imposes.

### Composition

<a id="req-0964"></a>

**REQ-0964.** Each `datasets` entry names one resolved specification through
`spec` and gives it an `id`. `id` is the name the document gives the dataset,
it is unique within the document, and it is what every generated identifier
for that dataset is built from.

<a id="req-0965"></a>

**REQ-0965.** An entry's `standard` binds that dataset to a declared standard
of type `IG`. It defaults to the document's `default_standard`, so a document
whose datasets all follow one implementation guide names it once. Each entry
has two lines. A document mixing standards -- two releases of an
implementation guide, or tabulation and analysis datasets together -- names
the exception on the exceptional entry. A dataset with neither fails:
[REQ-0859](metadata.md#req-0859) reads this binding to decide the dataset's family, and every
family-dependent requirement rests on it.

<a id="req-0966"></a>

**REQ-0966.** The entry order is the document's dataset order. This contract imposes
no other order. The recommended order of a submission's datasets is a
property of the standard a study follows rather than of this language, and a
study that follows one writes its entries in it.

<a id="req-0967"></a>

**REQ-0967.** Every specification a document names must declare
`root.submission`, and every column in its `output.columns` must declare both
`label` and `column.submission`. A document reports on the whole artifact, so
a column with nothing to report is a gap in the document, not an
omitted row.

<a id="req-0968"></a>

**REQ-0968.** Composition resolves the names [Submission metadata](metadata.md) and [Controlled terminology](terminology.md) leave unresolved: a
column's `codelist` and every document reference. A name matching no
declaration fails. Resolution also determines each dataset's family from the
standard [REQ-0965](define-xml.md#req-0965) binds and [REQ-0859](metadata.md#req-0859)'s table, and every family-dependent
requirement in [Submission metadata](metadata.md) is applied here.

<a id="req-0969"></a>

**REQ-0969.** Two entries must not name the same specification, and two entries
must not resolve to the same artifact path. A document that described one
artifact twice would carry two definitions a reader cannot reconcile.

### Identifiers

<a id="req-0970"></a>

**REQ-0970.** Every generated identifier is a prefix, a period, and the
declared identifiers of the object's position. The construction is total:

| Object | Identifier |
|---|---|
| `Study` | `STDY.<study.id>` |
| `MetaDataVersion` | `MDV.<metadata_version.id>` |
| `def:Standard` | `STD.<standard.id>` |
| `ItemGroupDef` | `IG.<dataset.id>` |
| `ItemDef` | `IT.<dataset.id>.<column>` |
| `CodeList` | `CL.<codelist.id>` |
| `MethodDef` | `MT.<dataset.id>.<column>` |
| `def:CommentDef`, dataset | `COM.<dataset.id>` |
| `def:CommentDef`, column | `COM.<dataset.id>.<column>` |
| `def:leaf`, document | `LF.<document.id>` |
| `def:leaf`, dataset archive | `LF.<dataset.id>` |

<a id="req-0971"></a>

**REQ-0971.** Collisions are impossible rather than resolved. Dataset, codelist,
standard, and document identifiers are each unique within the study document;
a column name is unique within its specification; the prefixes are disjoint.
Two kinds of object share one identifier space, `def:leaf`, so a document
identifier must not equal a dataset identifier; checked, not disambiguated.

<a id="req-0972"></a>

**REQ-0972.** An implementation must not mint an identifier from a counter, a
hash, or a random value. A generated identifier is a function of declarations
a reviewer can read, so a document regenerated after an unrelated edit keeps
every identifier it had, and two implementations agree without coordinating.

<a id="req-0973"></a>

**REQ-0973.** One `ItemDef` is generated per dataset and column, and never
shared between datasets. Define-XML admits a shared definition, and sharing
one would require every dataset that carries the column to agree on its label,
length, origin, and codelist forever. Two specifications can drift, and a
shared definition turns that drift into a silently wrong document. The cost is
stated: a document carries as many `STUDYID` definitions as it has datasets.

### Document structure

<a id="req-0974"></a>

**REQ-0974.** The generated document is exactly this structure, and every list
appears in the order stated. An element whose content is absent is omitted
rather than written empty.

```
<?xml version="1.0" encoding="UTF-8"?>
<?xml-stylesheet ...?>            when stylesheet is declared
<ODM>
  <Study>
    <GlobalVariables>
      <StudyName/> <StudyDescription/> <ProtocolName/>
    </GlobalVariables>
    <MetaDataVersion>
      <def:Standards>              standards, in declaration order
      <def:AnnotatedCRF>           annotated_crf documents, declaration order
      <def:SupplementalDoc>        supplemental documents, declaration order
      <ItemGroupDef>*              datasets, in entry order
      <ItemDef>*                   dataset order, then output.columns order
      <CodeList>*                  codelists, in declaration order
      <MethodDef>*                 dataset order, then output.columns order
      <def:CommentDef>*            see REQ-0976
      <def:leaf>*                  documents, in declaration order
    </MetaDataVersion>
  </Study>
</ODM>
```

<a id="req-0975"></a>

**REQ-0975.** The element order is the order the Define-XML 2.1 schema
requires. Within each repeated element the order above is this contract's choice,
and it is fixed.

<a id="req-0976"></a>

**REQ-0976.** Comment definitions are emitted per dataset in entry order: the
dataset's own comment first, its columns' comments in `output.columns` order.

<a id="req-0977"></a>

**REQ-0977.** A dataset's `def:leaf` is a child of its `ItemGroupDef`.
A supporting document's `def:leaf` is a child of `MetaDataVersion`, as the
schema requires.

<a id="req-0978"></a>

**REQ-0978.** The namespaces declared on `ODM` are the ODM 1.3 namespace as the
default, `def` for `http://www.cdisc.org/ns/def/v2.1`, and `xlink` for
`http://www.w3.org/1999/xlink`. No other namespace is declared and no
extension element is generated.

### Element mapping

<a id="req-0979"></a>

**REQ-0979.** `ODM` carries its namespace declarations, then these
attributes in this order:

| Attribute | Source |
|---|---|
| `ODMVersion` | fixed `1.3.2` |
| `FileType` | fixed `Snapshot` |
| `FileOID` | `file_oid` |
| `CreationDateTime` | `creation_datetime` |
| `Originator` | `originator`, when declared |
| `SourceSystem` | `source_system`, when declared |
| `SourceSystemVersion` | `source_system_version`, when declared |
| `def:Context` | `context` |

`source_system_version` is declared only with `source_system`.

<a id="req-0980"></a>

**REQ-0980.** `Study` carries `OID` alone. Its `GlobalVariables` child
carries `StudyName`, `StudyDescription`, and `ProtocolName`, in that order,
from `study.name`, `study.description`, and `study.protocol_name`.

<a id="req-0981"></a>

**REQ-0981.** `MetaDataVersion` carries `OID`, `Name` from
`metadata_version.name`, `Description` when declared, and `def:DefineVersion`
from `define_version`, in that order.

#### Standards and documents

<a id="req-0982"></a>

**REQ-0982.** Each `def:Standard` carries `OID`, `Name`, `Type`,
`PublishingSet`, `Version`, and `Status`, in that order. `PublishingSet` must
be declared for a `CT` standard and must not be declared for an `IG`
standard.

<a id="req-0983"></a>

**REQ-0983.** Each document produces one `def:leaf` whose `ID` is its generated
identifier, whose `xlink:href` is the document's `href`, and whose
`def:title` is its `title`. A document of kind `annotated_crf` is also listed
in `def:AnnotatedCRF`, one of kind `supplemental` in `def:SupplementalDoc`,
and one of kind `other` in neither; every kind may be referenced from an
origin, a method, or a comment. `href` names a file the submission package
carries, and this contract does not require it to exist when the document is
generated: supporting documents are assembled outside this language, and a
document generated before its annotated case report form is finished is still
the correct document.

<a id="req-0984"></a>

**REQ-0984.** At most one document may declare kind `annotated_crf`, and one
must be declared whenever any column's origin is `Collected` from an
`Investigator` or a `Subject`. [REQ-0893](metadata.md#req-0893) names that document and [REQ-0894](metadata.md#req-0894) derives
the reference to it, and neither is possible unless exactly one exists. A
document reference on such an origin that names any other document fails.

#### Dataset

<a id="req-0985"></a>

**REQ-0985.** Each dataset produces one `ItemGroupDef`:

| Attribute or child | Source |
|---|---|
| `OID` | generated |
| `Name` | the entry's `id` |
| `Domain` | `submission.domain`, default `domain`; omit for `adam` |
| `Purpose` | `Tabulation` for `sdtm` and `send`, `Analysis` for `adam` |
| `SASDatasetName` | the entry's `id`, when [REQ-1002](define-xml.md#req-1002) admits it |
| `Repeating` | `Yes` when `submission.repeating`, else `No` |
| `IsReferenceData` | `Yes` when `submission.reference_data`, else `No` |
| `def:Structure` | `submission.structure` |
| `def:ArchiveLocationID` | the dataset's generated leaf identifier |
| `def:StandardOID` | the generated identifier of the standard [REQ-0965](define-xml.md#req-0965) binds |
| `def:CommentOID` | generated, when `submission.comment` is declared |
| `def:HasNoData` | `Yes` when the entry declares it, else omitted |
| `Description` | `submission.label` |
| `ItemRef` | one per `output.columns` entry, in that order |
| `def:Class` | `submission.class`, with `def:SubClass` when declared |
| `def:leaf` | the artifact, per [REQ-0987](define-xml.md#req-0987) |

The `Domain` default is the specification's `domain`.

<a id="req-0986"></a>

**REQ-0986.** `Purpose` is derived from the family alone, not declared. A
declared value could only agree with or contradict the derivation.

<a id="req-0987"></a>

**REQ-0987.** The dataset's `def:leaf` names the file the package carries for
that dataset: the entry's `dataset_json` when it declares one, per
[REQ-1215](dataset-json.md#req-1215), and otherwise the artifact its
specification produces. `xlink:href` is that path expressed relative
to the directory holding the generated document, written with `/` separators,
and `def:title` is that path's final component. The href must not begin with a
parent traversal: a document that pointed outside its own directory would
describe a file that is not in the package it belongs to.

<a id="req-0988"></a>

**REQ-0988.** The leaf names a file this language produced, and the two it
produces are the artifact [Artifact publication](../storage/publication.md) publishes and the Dataset-JSON file
[Dataset-JSON](dataset-json.md) writes beside this document. A document that named any other
container would assert a file no run wrote. A package that converts its
artifacts to a container neither contract writes regenerates the document
against the converted paths, and that conversion is outside this language.

<a id="req-0989"></a>

**REQ-0989.** A `def:HasNoData` dataset must declare `submission.comment`,
which states why a planned dataset holds no records.

#### Column reference

<a id="req-0990"></a>

**REQ-0990.** Each `output.columns` entry produces one `ItemRef` inside its
`ItemGroupDef`:

| Attribute | Source |
|---|---|
| `ItemOID` | the column's generated identifier |
| `OrderNumber` | the column's one-based position in `output.columns` |
| `Mandatory` | `Yes` when [Submission metadata](metadata.md)'s mandatory resolution is true, else `No` |
| `KeySequence` | one-based position in `keys`; omit when not a key |
| `Role` | `submission.role`, when declared |
| `MethodOID` | generated method identifier; omit without `submission.method` |

<a id="req-0991"></a>

**REQ-0991.** `OrderNumber` is written for every `ItemRef` of a container or for
none of them, and this contract writes it for every one. Define-XML makes the
attribute optional and falls back to document order, and writing it always
means the presented order survives any later reordering of the document.

#### Column definition

<a id="req-0992"></a>

**REQ-0992.** Each column produces one `ItemDef`:

| Attribute or child | Source |
|---|---|
| `OID` | generated |
| `Name` | the column name |
| `DataType` | [Submission metadata](metadata.md)'s resolved submission data type |
| `Length` | `submission.length`, when [Submission metadata](metadata.md) admits it |
| `SignificantDigits` | `submission.significant_digits`, when [Submission metadata](metadata.md) admits it |
| `SASFieldName` | the column name, when [REQ-1002](define-xml.md#req-1002) admits it |
| `def:DisplayFormat` | `submission.display_format`, when declared |
| `def:CommentOID` | generated, when `submission.comment` is declared |
| `Description` | the column's `label` |
| `CodeListRef` | the bound codelist's generated identifier, when declared |
| `def:Origin` | per [REQ-0993](define-xml.md#req-0993) |

<a id="req-0993"></a>

**REQ-0993.** `def:Origin` carries `Type` and, when [Submission metadata](metadata.md) admits one, `Source`,
which [REQ-0890](metadata.md#req-0890) derives for the `adam` family. Its `Description` child carries
`origin.description` when declared, and one `def:DocumentRef` child follows per
document reference in declaration order, including the annotated case report
form [REQ-0894](metadata.md#req-0894) derives when a collected origin declares none. Exactly one
`def:Origin` is generated per column: a second would describe a column whose
rows have different provenance, which needs the value-level metadata [REQ-1007](define-xml.md#req-1007)
defers.

<a id="req-0994"></a>

**REQ-0994.** A `def:DocumentRef` carries `leafID`, the referenced document's
generated leaf identifier, and one `def:PDFPageRef` child when `pages` is
declared, carrying `PageRefs`, `Type`, and `Title` when declared.

#### Terminology

<a id="req-0995"></a>

**REQ-0995.** Each codelist produces one `CodeList` carrying `OID`, `Name`,
`DataType`, then `def:StandardOID` when `standard` is declared, then
`def:IsNonStandard="Yes"` when `standard` is not declared and the codelist
declares `items`, then `SASFormatName` from `format_name` when declared. A
codelist declaring `items` therefore always carries exactly one of the two,
the choice Define-XML requires of it. An external codelist naming
no standard carries neither: `def:IsNonStandard` marks items a sponsor
defined, and an external codelist has none.

<a id="req-0996"></a>

**REQ-0996.** A codelist declaring `items` produces `CodeListItem` children
when its items declare `decode` and `EnumeratedItem` children when they do
not. [Controlled terminology](terminology.md) requires the choice to be uniform within a codelist, so the element
is determined rather than chosen per item.

<a id="req-0997"></a>

**REQ-0997.** Each item carries `CodedValue` from `value`, `Rank` when
declared, and `def:ExtendedValue="Yes"` when `extended`. A `CodeListItem`
carries a `Decode` child holding the decode text, and either element carries
an `Alias` child with `Context="nci:ExtCodeID"` when the item declares
`alias`. Items keep the codelist's declaration order, and `OrderNumber` is not
written: the document order already presents them in it, and a second ordering
could disagree.

<a id="req-0998"></a>

**REQ-0998.** A codelist declaring `alias` carries an `Alias` child with
`Context="nci:ExtCodeID"` after its items. A codelist declaring `external`
carries one `ExternalCodeList` child with `Dictionary`, `Version`, and `href`
when declared, and no items.

#### Methods and comments

<a id="req-0999"></a>

**REQ-0999.** Each declared method produces one `MethodDef` carrying `OID`,
`Name` from `method.name` or, when omitted, `<dataset.id>.<column>`, and `Type`
from `method.type`, in that order. Its `Description`
child carries the method's `description`, one `FormalExpression` child follows
when `expression` is declared, carrying `Context` and the code as its text,
and one `def:DocumentRef` child follows per declared document reference.

<a id="req-1000"></a>

**REQ-1000.** Each declared comment produces one `def:CommentDef` carrying its
generated identifier, a `Description` child holding the comment text, and one
`def:DocumentRef` child per declared document reference.

<a id="req-1001"></a>

**REQ-1001.** Every `Description` and `Decode` holds exactly one
`TranslatedText` whose `xml:lang` is the document's `language`. One document
carries one language. A multilingual document would need a language-keyed text
object in every place this design carries a string, which is a change to [Submission metadata](metadata.md)
and [Controlled terminology](terminology.md) as much as to this contract.

#### Transport names

<a id="req-1002"></a>

**REQ-1002.** `SASDatasetName` and `SASFieldName` are written only for names of
at most eight characters matching `^[A-Za-z_][A-Za-z0-9_]*$`. A longer name is
omitted, and fails under `Submission` instead. Define-XML carries a long
`Alias` element, which [REQ-1007](define-xml.md#req-1007) defers.

### Requirements a submission context adds

<a id="req-1003"></a>

**REQ-1003.** Under `context: Other`, a document is generated from whatever the
declarations supply and is valid against the Define-XML 2.1 schema.

<a id="req-1004"></a>

**REQ-1004.** Under `context: Submission`, each of the following additionally
holds, and each is a conditional requirement the Define-XML specification
places on a regulatory submission:

- every dataset declares `submission.domain` or inherits one, for the `sdtm`
  and `send` families;
- every dataset's `SASDatasetName` and every column's `SASFieldName` is
  written, so [REQ-1002](define-xml.md#req-1002) must admit every name;
- every dataset carries a `def:ArchiveLocationID`, unless it declares
  `has_no_data`;
- every dataset carries a `Description` and a `def:Class`;
- every dataset declares at least one key, which [Artifact publication](../storage/publication.md) already requires, and
  every key column carries a `KeySequence`;
- every column carries a `def:Origin`, which [REQ-0887](metadata.md#req-0887) already requires; and
- every codelist that declares `items` and is drawn from a standard whose
  name is `CDISC/NCI` declares `alias`, and so does every one of its items. An
  external codelist is excluded: it represents its dictionary rather than
  CDISC controlled terminology, and has no items to identify.

### Serialization

<a id="req-1005"></a>

**REQ-1005.** The document is encoded UTF-8 and carries no byte-order mark;
`U+000A` terminates every line, including the last. `U+000D` is never written.

<a id="req-1006"></a>

**REQ-1006.** The remaining byte-level decisions are fixed exactly:

- the first line is `<?xml version="1.0" encoding="UTF-8"?>`;
- a declared `stylesheet` produces a second line,
  `<?xml-stylesheet type="text/xsl" href="..."?>`, and no stylesheet produces
  no line;
- each element begins on its own line, indented by two spaces per level of
  nesting below `ODM`, which is unindented;
- an element whose content is text carries that text between its tags on one
  line, opening tag, text, and closing tag together; text holding a `U+000A`
  therefore spans lines, and the following element still begins at its own
  indentation;
- an element with no content is written as a self-closing tag with no space
  before `/>`;
- attributes are written on the element's opening line, separated by one
  space, each as `Name="value"`, in the order this contract's mapping tables list
  them, with namespace declarations first on `ODM`;
- in an attribute value, `&` is written `&amp;`, `<` is written `&lt;`, `>` is
  written `&gt;`, and `"` is written `&quot;`;
- in text content, `&` is written `&amp;`, `<` is written `&lt;`, and `>` is
  written `&gt;`; and
- no comment, processing instruction other than the stylesheet, CDATA
  section, or blank line is written.

<a id="req-1007"></a>

**REQ-1007.** `U+000D` and `U+0009` must not appear in any generated attribute
value or text, and `U+000A` must not appear in an attribute value. An XML
parser normalizes each forbidden character on the way back in, so a document
carrying one does not read back as written, and a byte contract over text
that does not survive parsing is not a contract.

<a id="req-1008"></a>

**REQ-1008.** The bytes are fixed rather than the information. Two conforming
implementations produce byte-identical files, and a golden document is
compared byte for byte.

<a id="req-1009"></a>

**REQ-1009.** Publication follows [Artifact publication](../storage/publication.md)'s procedure exactly: the complete
document is written to a temporary regular file in the target's directory,
flushed and closed, and atomically moved onto the target; a failure leaves the
previous file and no residue. Generation completes before publication begins,
so a failure at any point publishes nothing.

<a id="req-1010"></a>

**REQ-1010.** The generated document must be valid against the Define-XML 2.1
schema. Conformance to this contract is not a substitute for that validation, and
an implementation that produces a schema-invalid document has a defect
regardless of which requirement above it satisfied.

### What this contract does not generate

<a id="req-1011"></a>

**REQ-1011.** The following are outside this contract. None has a field in
the schema [Submission metadata](metadata.md), [Controlled terminology](terminology.md), and `schema_define.yaml` close, so a
specification that tries to declare one is rejected as an unknown field rather
than generating a document that quietly omits it.

- `arm:AnalysisResultDisplays`: an analysis-results metadata design
- Split datasets and their `Alias` domain description: a specification that
  produces one dataset in several files
- `def:IsNonStandard` on a dataset or column: a conformance-profile rule
  defining non-standard
- A comment on a standard, codelist, or metadata version: a shared comment
  identifier space

<a id="req-1012"></a>

**REQ-1012.** Two further constructs have a declaration but no generation, and
each is stated rather than silently dropped. An `Alias` carrying a transport
name longer than eight characters is not generated, so [REQ-1002](define-xml.md#req-1002) omits the
attribute and [REQ-1022](define-xml.md#req-1022) fails under `Submission` instead; a naming rule that owns
transport names would supply it. A `FormalExpression` is generated only from a
declared `method.expression` and never from a derivation.

<a id="req-1013"></a>

**REQ-1013.** Value-level metadata now has its construct: a `rows` entry's
`submission` map (see [REQ-1162](metadata.md#req-1162) through [REQ-1168](metadata.md#req-1168)) declares per-value
metadata against statically enumerated `--TESTCD` literals. The mapping to
Define-XML is therefore fixed: for each column carrying value-level
metadata, the document generates one `def:ValueListDef` per distinct test
code value, referenced from the column's `ItemRef`; each value's entry
becomes a per-value `ItemDef` carrying the column-level declaration merged
with the row-level overrides; each value list entry carries a
`def:WhereClauseDef` selecting rows whose `<DOMAIN>TESTCD` equals the
literal code; and each per-value `ItemDef` carries the row-level entry's
`def:Origin`.

<a id="req-1169"></a>

**REQ-1169.** No generator consumes the row-level `submission` map yet, so a
study document that includes a specification carrying value-level metadata
fails loudly under the `Submission` context rather than generating a
document that quietly omits the per-value definitions. Declared and
unconsumed is an error; the failure names the row and column whose metadata
has no generated form.

### Interface behavior

<a id="req-1061"></a>

**REQ-1061.** The `define_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `define_class.schema_version` | Schema bundle this document is written against; [Schema language](../reference/schema-language.md) requires an exact match. |
| `define_class.define_version` | Define-XML release the generated document conforms to. |
| `define_class.context` | Context the document is used in; [Define-XML](define-xml.md) states which requirements it adds. |
| `define_class.language` | Language tag every generated translated text carries. |
| `define_class.file_oid` | Identity of the generated file, declared rather than derived from a run. |
| `define_class.creation_datetime` | Creation timestamp the document carries; declaring it keeps generation reproducible. |
| `define_class.originator` | Party submitting the document. |
| `define_class.source_system` | Application that generated the document. |
| `define_class.source_system_version` | Version of that application; declared only with source_system. |
| `define_class.stylesheet` | Stylesheet the generated document references, relative to the document. |
| `define_class.study` | Study the datasets belong to. |
| `define_class.metadata_version` | Identity and name of this metadata version. |
| `define_class.standards` | Standards the represented datasets and codelists conform to. |
| `define_class.default_standard` | Foundational standard every dataset follows unless its entry names another. |
| `define_class.documents` | Supporting documents referenced by datasets, origins, methods, and comments. |
| `define_class.datasets` | Represented datasets in the order the document presents them. |
| `define_class.output` | File this document produces. |

<a id="req-1062"></a>

**REQ-1062.** The `study_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `study_class.id` | Identifier the study's generated identity is built from. |
| `study_class.name` | Name of the study. |
| `study_class.description` | Description of the study. |
| `study_class.protocol_name` | Protocol the study runs under. |

<a id="req-1063"></a>

**REQ-1063.** The `metadata_version_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `metadata_version_class.id` | Identifier this metadata version's generated identity is built from. |
| `metadata_version_class.name` | Name of this metadata version. |
| `metadata_version_class.description` | Description of what this metadata version covers. |

<a id="req-1064"></a>

**REQ-1064.** The `standard_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `standard_class.id` | Name datasets and codelists refer to this standard by. |
| `standard_class.name` | Published standard name. |
| `standard_class.type` | Whether the standard is an implementation guide or controlled terminology. |
| `standard_class.publishing_set` | Publishing set of a controlled-terminology standard. |
| `standard_class.version` | Version identifier exactly as the standard publishes it. |
| `standard_class.status` | Publication status of that version. |

<a id="req-1065"></a>

**REQ-1065.** The `document_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `document_class.id` | Name references to this document use. |
| `document_class.kind` | Where the document is listed; an other document is listed only where it is referenced. |
| `document_class.href` | File the document is, relative to the directory holding the generated document. |
| `document_class.title` | Label a reader sees for the document. |

<a id="req-1066"></a>

**REQ-1066.** The `define_dataset_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `define_dataset_class.id` | Name the generated document gives this dataset. |
| `define_dataset_class.spec` | Resolved specification whose output this dataset is. |
| `define_dataset_class.standard` | Foundational standard this dataset follows; defaults to default_standard. |
| `define_dataset_class.has_no_data` | Whether the dataset was planned but holds no records; [Define-XML](define-xml.md) requires a comment with it. |
| `define_dataset_class.dataset_json` | Dataset-JSON file this document produces for the dataset; [Dataset-JSON](dataset-json.md) owns its contents and bytes. |

<a id="req-1067"></a>

**REQ-1067.** The `define_output_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `define_output_class.path` | File this document produces; its extension must be .xml. |

<a id="req-1068"></a>

**REQ-1068.** The `define_id` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `define_id` | Identifier a generated OID is built from. |

<a id="req-1069"></a>

**REQ-1069.** The `define_version` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `define_version` | Define-XML release, written as Define-XML 2.1 publishes it. |

<a id="req-1070"></a>

**REQ-1070.** The `relative_href` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `relative_href` | Location referenced from the generated document, relative to the directory holding it and never leaving that directory. [Define-XML](define-xml.md) does not require the file to exist when the document is generated. |

<a id="req-1071"></a>

**REQ-1071.** The `define_path` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `define_path` | Relative file this document produces, named with an xml extension. |

<a id="req-1072"></a>

**REQ-1072.** The `creation_datetime` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `creation_datetime` | Local civil datetime resolved to a whole second, as [Temporal values](../values/temporal.md) writes one. |

<a id="req-1073"></a>

**REQ-1073.** The `language_tag` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `language_tag` | Language tag carried by every generated translated text. |

<a id="req-1074"></a>

**REQ-1074.** The `standard_name` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `standard_name` | Standard name, in the exact case Define-XML 2.1 publishes. |

<a id="req-1075"></a>

**REQ-1075.** The `module` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `Scope` | Separately validated entry point for the study document [Define-XML](define-xml.md) defines. One document selects the resolved dataset specifications a Define-XML 2.1 document represents, declares the standards, supporting documents, and codelists they share, and fixes every value the generated document carries that no specification supplies. A specification stays a single-dataset contract. Nothing here changes what a specification derives. |

## Error conditions

<a id="req-1014"></a>

**REQ-1014.** An `output.path` whose extension is not `.xml`: fail validation.

<a id="req-1015"></a>

**REQ-1015.** A specification named by an entry that declares no
`root.submission`, or an output column with no `label` or no
`column.submission`: fail with `submission_metadata_missing`, reporting the
dataset and the column.

<a id="req-1016"></a>

**REQ-1016.** A `standard`, `codelist`, or document reference naming no
declaration: fail with `unknown_standard`, `unknown_codelist`, or
`unknown_document`, reporting the referencing position.

<a id="req-1017"></a>

**REQ-1017.** A duplicate dataset `id`, standard `id`, codelist `id`, or
document `id`; or a document `id` equal to a dataset `id`: fail with
`duplicate_define_identifier`.

<a id="req-1018"></a>

**REQ-1018.** Two entries naming the same specification, or resolving to the
same artifact path: fail with `duplicate_dataset_entry`.

<a id="req-1019"></a>

**REQ-1019.** More than one document of kind `annotated_crf`, or none where
[REQ-0984](define-xml.md#req-0984) requires one: fail validation.

<a id="req-1020"></a>

**REQ-1020.** A `PublishingSet` declared on an `IG` standard or absent from a
`CT` standard: fail validation.

<a id="req-1021"></a>

**REQ-1021.** An artifact path that cannot be expressed relative to the
document's directory without a parent traversal: fail with
`artifact_outside_document`, reporting the dataset and both paths.

<a id="req-1022"></a>

**REQ-1022.** A `has_no_data` dataset with no `submission.comment`: fail
validation and report the dataset.

<a id="req-1023"></a>

**REQ-1023.** A transport name [REQ-1002](define-xml.md#req-1002) does not admit, under `Submission`: fail
with `sas_name_too_long`, reporting the name.

<a id="req-1024"></a>

**REQ-1024.** Any requirement [REQ-1004](define-xml.md#req-1004) adds that is unmet under `Submission`:
fail, reporting the requirement and the position.

<a id="req-1025"></a>

**REQ-1025.** A `U+000D`, `U+0009`, or attribute-value `U+000A` in generated
text: fail with `untransportable_text`, reporting the declaration that carries
it.

<a id="req-1026"></a>

**REQ-1026.** A generated document that is not valid against the Define-XML 2.1
schema: fail and report the schema violation. The run publishes nothing.

<a id="req-1027"></a>

**REQ-1027.** A failed publication: fail and report the target, exactly as
[Artifact publication](../storage/publication.md) does. The previous document is unchanged.

## Conformance examples

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Compose study metadata into deterministic Define-XML 2.1 documents. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
