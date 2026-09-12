---
id: R026
title: Define-XML 2.1 Composition and Serialization
status: normative
applies_to: [define_class, define.datasets, define.standards, define.documents,
  define.output, root.submission, column.submission]
---

# Define-XML 2.1 composition and serialization

## Intent

Compose several resolved specifications and the metadata they carry into one
Define-XML 2.1 document: select the datasets, resolve the names their
submission metadata uses, construct every identifier, fix every element and
attribute and the order they appear in, and write the exact bytes. Two
implementations given the same inputs produce the same file.

## Boundaries

This rule owns the study document, the composition, the mapping to Define-XML
2.1, the generated identifiers, the document order, the bytes, and
publication. It does not own the metadata being mapped: R024 owns the governed
dataset and column vocabulary and every consistency requirement over one
specification, and R025 owns codelists and what a binding enforces. This rule
applies both and restates neither.

R017 owns how a specification with `parents` resolves; every specification
this rule reads is already resolved. R005 owns the artifact's columns, their
order, its keys, and its rows. R020 owns the artifact's own bytes and the
procedure by which a completed file replaces its target; this rule writes a
different artifact and reuses that procedure by name. R021 owns the project
root every declared path is confined to, and R019 owns the contents and
ordering of the strings this document carries.

This rule does not own dataset artifact production. A document describes
artifacts R005 and R020 have already decided, and generating one neither runs
a derivation nor reads an artifact's bytes.

## The study document

**R026-1.** A **study document** is a separately validated entry point,
written against `schema_define.yaml`. It declares one study, one metadata
version, the standards, supporting documents, and codelists its datasets
share, the ordered datasets it represents, and the file it produces. A
specification remains a single-dataset contract and declares none of this.

**R026-2.** The study document is the only place the composition is declared.
An implementation must not assemble a document from whatever specifications a
directory holds: which datasets one submission represents is a study decision,
and a directory listing is not one.

**R026-3.** `output.path` names the file the document produces, and its
extension must be `.xml`. R020's extension mapping is not consulted: that
mapping selects a profile for a dataset artifact, and this artifact has one
form.

**R026-4.** `creation_datetime` is declared rather than read from a clock.
Two runs of the same inputs must produce the same bytes, and a timestamp taken
at run time would make every run differ from every other. `file_oid` is
declared for the same reason: an identity minted per run is not reproducible.

**R026-5.** `context` states whether the document is used in a submission.
`Submission` adds the requirements R026-45 lists. Nothing below is relaxed by
`Other`; that context only omits requirements a regulator imposes.

## Composition

**R026-6.** Each `datasets` entry names one resolved specification through
`spec` and gives it an `id`. `id` is the name the document gives the dataset,
it is unique within the document, and it is what every generated identifier
for that dataset is built from.

**R026-7.** The entry order is the document's dataset order. This rule imposes
no other order, because the recommended order of a submission's datasets is a
property of the standard a study follows rather than of this language, and a
study that follows one writes its entries in it.

**R026-8.** Every specification a document names must declare
`root.submission`, and every column in its `output.columns` must declare both
`label` and `column.submission`. A document reports on the whole artifact, so
a column with nothing to report is a gap in the document rather than an
omitted row.

**R026-9.** Composition resolves the names R024 and R025 leave unresolved: a
dataset's `standard`, a column's `codelist`, and every document reference. A
name matching no declaration fails. Resolution also determines each dataset's
family under R024-4, and every family-dependent requirement in R024 is applied
here.

**R026-10.** Two entries must not name the same specification, and two entries
must not resolve to the same artifact path. A document that described one
artifact twice would carry two definitions a reader cannot reconcile.

## Identifiers

**R026-11.** Every generated identifier is a prefix, a period, and the
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

**R026-12.** Collisions are impossible rather than resolved. Dataset,
codelist, standard, and document identifiers are each unique within the study
document; a column name is unique within its specification; and the prefixes
are disjoint. The one identifier space two kinds of object share is
`def:leaf`, so a document identifier must not equal a dataset identifier, and
that is checked rather than disambiguated.

**R026-13.** An implementation must not mint an identifier from a counter, a
hash, or a random value. A generated identifier is a function of declarations
a reviewer can read, so a document regenerated after an unrelated edit keeps
every identifier it had, and two implementations agree without coordinating.

**R026-14.** One `ItemDef` is generated per dataset and column, and never
shared between datasets. Define-XML admits a shared definition, and sharing
one would require every dataset that carries the column to agree on its label,
length, origin, and codelist forever. Two specifications can drift, and a
shared definition turns that drift into a silently wrong document. The cost is
stated: a document carries as many `STUDYID` definitions as it has datasets.

## Document structure

**R026-15.** The generated document is exactly this structure, and every list
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
      <def:CommentDef>*            see R026-17
      <def:leaf>*                  documents, in declaration order
    </MetaDataVersion>
  </Study>
</ODM>
```

**R026-16.** The element order is the order the Define-XML 2.1 schema
requires, so a document that departs from it is not merely different but
invalid. Within each repeated element the order above is this rule's choice,
and it is fixed so that two implementations agree.

**R026-17.** Comment definitions are emitted per dataset in entry order: the
dataset's own comment first, then its columns' comments in `output.columns`
order.

**R026-18.** A dataset's `def:leaf` is a child of its `ItemGroupDef`, and a
supporting document's `def:leaf` is a child of `MetaDataVersion`. That is
where the schema places each.

**R026-19.** The namespaces declared on `ODM` are the ODM 1.3 namespace as the
default, `def` for `http://www.cdisc.org/ns/def/v2.1`, and `xlink` for
`http://www.w3.org/1999/xlink`. No other namespace is declared and no
extension element is generated.

## Element mapping

### ODM, Study, and MetaDataVersion

**R026-20.** `ODM` carries its namespace declarations, then these
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

**R026-21.** `Study` carries `OID` alone. Its `GlobalVariables` child
carries `StudyName`, `StudyDescription`, and `ProtocolName`, in that order,
from `study.name`, `study.description`, and `study.protocol_name`.

**R026-22.** `MetaDataVersion` carries `OID`, `Name` from
`metadata_version.name`, `Description` when declared, and `def:DefineVersion`
from `define_version`, in that order.

### Standards and documents

**R026-23.** Each `def:Standard` carries `OID`, `Name`, `Type`,
`PublishingSet`, `Version`, and `Status`, in that order. `PublishingSet` must
be declared for a `CT` standard and must not be declared for an `IG`
standard.

**R026-24.** Each document produces one `def:leaf` whose `ID` is its generated
identifier, whose `xlink:href` is the document's `href`, and whose
`def:title` is its `title`. A document of kind `annotated_crf` is also listed
in `def:AnnotatedCRF`, one of kind `supplemental` in `def:SupplementalDoc`,
and one of kind `other` in neither; every kind may be referenced from an
origin, a method, or a comment. `href` names a file the submission package
carries, and this rule does not require it to exist when the document is
generated: supporting documents are assembled outside this language, and a
document generated before its annotated case report form is finished is still
the correct document.

**R026-25.** At most one document may declare kind `annotated_crf`. A study
has one annotated case report form, and R024-37's requirement names it without
ambiguity only while that holds.

### Dataset

**R026-26.** Each dataset produces one `ItemGroupDef`:

| Attribute or child | Source |
|---|---|
| `OID` | generated |
| `Name` | the entry's `id` |
| `Domain` | `submission.domain`, defaulting to the specification's `domain`; omitted for the `adam` family |
| `Purpose` | `Tabulation` for `sdtm` and `send`, `Analysis` for `adam` |
| `SASDatasetName` | the entry's `id`, when R026-43 admits it |
| `Repeating` | `Yes` when `submission.repeating`, else `No` |
| `IsReferenceData` | `Yes` when `submission.reference_data`, else `No` |
| `def:Structure` | `submission.structure` |
| `def:ArchiveLocationID` | the dataset's generated leaf identifier |
| `def:StandardOID` | the resolved standard's generated identifier |
| `def:CommentOID` | generated, when `submission.comment` is declared |
| `def:HasNoData` | `Yes` when the entry declares it, else omitted |
| `Description` | `submission.label` |
| `ItemRef` | one per `output.columns` entry, in that order |
| `def:Class` | `submission.class`, with `def:SubClass` when declared |
| `def:leaf` | the artifact, per R026-28 |

**R026-27.** `Purpose` is derived rather than declared. It is a function of the
family alone, and a declared value could only agree with the derivation or
contradict it.

**R026-28.** The dataset's `def:leaf` names the artifact its specification
produces. `xlink:href` is the specification's `output.path` expressed relative
to the directory holding the generated document, written with `/` separators,
and `def:title` is that path's final component. The href must not begin with a
parent traversal: a document that pointed outside its own directory would
describe a file that is not in the package it belongs to.

**R026-29.** The leaf names the artifact this language produced. This design
writes no transport file, so a document that named one would assert a file no
run wrote. A package that converts its artifacts to another container
regenerates the document against the converted paths, and that conversion is
outside this language.

**R026-30.** A `def:HasNoData` dataset must declare `submission.comment`,
which states why a planned dataset holds no records.

### Column reference

**R026-31.** Each `output.columns` entry produces one `ItemRef` inside its
`ItemGroupDef`:

| Attribute | Source |
|---|---|
| `ItemOID` | the column's generated identifier |
| `OrderNumber` | the column's one-based position in `output.columns` |
| `Mandatory` | `Yes` when R024's mandatory resolution is true, else `No` |
| `KeySequence` | the column's one-based position in `keys`, omitted when it is not a key |
| `Role` | `submission.role`, when declared |
| `MethodOID` | the column's generated method identifier, when `submission.method` is declared |

**R026-32.** `OrderNumber` is written for every `ItemRef` of a container or for
none of them, and this rule writes it for every one. Define-XML makes the
attribute optional and falls back to document order, and writing it always
means the presented order survives any later reordering of the document.

### Column definition

**R026-33.** Each column produces one `ItemDef`:

| Attribute or child | Source |
|---|---|
| `OID` | generated |
| `Name` | the column name |
| `DataType` | R024's resolved submission data type |
| `Length` | `submission.length`, when R024 admits it |
| `SignificantDigits` | `submission.significant_digits`, when R024 admits it |
| `SASFieldName` | the column name, when R026-43 admits it |
| `def:DisplayFormat` | `submission.display_format`, when declared |
| `def:CommentOID` | generated, when `submission.comment` is declared |
| `Description` | the column's `label` |
| `CodeListRef` | the bound codelist's generated identifier, when declared |
| `def:Origin` | per R026-34 |

**R026-34.** `def:Origin` carries `Type` and, when R024 admits one, `Source`.
Its `Description` child carries `origin.description` when declared, and one
`def:DocumentRef` child follows per declared document reference, in
declaration order. Exactly one `def:Origin` is generated per column: a second
would describe a column whose rows have different provenance, which needs the
value-level metadata R026-48 defers.

**R026-35.** A `def:DocumentRef` carries `leafID`, the referenced document's
generated leaf identifier, and one `def:PDFPageRef` child when `pages` is
declared, carrying `PageRefs`, `Type`, and `Title` when declared.

### Terminology

**R026-36.** Each codelist produces one `CodeList` carrying `OID`, `Name`,
`DataType`, then `def:StandardOID` when `standard` is declared, then
`def:IsNonStandard="Yes"` when `standard` is not declared and the codelist
declares `items`, then `SASFormatName` from `format_name` when declared. A
codelist declaring `items` therefore always carries exactly one of the two,
which is the choice Define-XML requires of it. An external codelist that names
no standard carries neither: `def:IsNonStandard` marks items a sponsor
defined, and an external codelist has none.

**R026-37.** A codelist declaring `items` produces `CodeListItem` children
when its items declare `decode` and `EnumeratedItem` children when they do
not. R025 requires the choice to be uniform within a codelist, so the element
is determined rather than chosen per item.

**R026-38.** Each item carries `CodedValue` from `value`, `Rank` when
declared, and `def:ExtendedValue="Yes"` when `extended`. A `CodeListItem`
carries a `Decode` child holding the decode text, and either element carries
an `Alias` child with `Context="nci:ExtCodeID"` when the item declares
`alias`. Items keep the codelist's declaration order, and `OrderNumber` is not
written: the document order already presents them in it, and a second ordering
could disagree.

**R026-39.** A codelist declaring `alias` carries an `Alias` child with
`Context="nci:ExtCodeID"` after its items. A codelist declaring `external`
carries one `ExternalCodeList` child with `Dictionary`, `Version`, and `href`
when declared, and no items.

### Methods and comments

**R026-40.** Each declared method produces one `MethodDef` carrying `OID`,
`Name` from `method.name` or, when omitted, `<dataset.id>.<column>`, and `Type`
from `method.type`, in that order. Its `Description`
child carries the method's `description`, one `FormalExpression` child follows
when `expression` is declared, carrying `Context` and the code as its text,
and one `def:DocumentRef` child follows per declared document reference.

**R026-41.** Each declared comment produces one `def:CommentDef` carrying its
generated identifier, a `Description` child holding the comment text, and one
`def:DocumentRef` child per declared document reference.

**R026-42.** Every `Description` and `Decode` holds exactly one
`TranslatedText` whose `xml:lang` is the document's `language`. One document
carries one language. A multilingual document would need a language-keyed text
object in every place this design carries a string, which is a change to R024
and R025 as much as to this rule.

### Transport names

**R026-43.** `SASDatasetName` and `SASFieldName` are written when the name
they carry is at most eight characters and matches
`^[A-Za-z_][A-Za-z0-9_]*$`, which is the form the transport admits. A longer
name is omitted, and under `Submission` it fails instead. Define-XML carries a
long name through an `Alias` element, which R026-48 defers.

## Requirements a submission context adds

**R026-44.** Under `context: Other`, a document is generated from whatever the
declarations supply and is valid against the Define-XML 2.1 schema.

**R026-45.** Under `context: Submission`, each of the following additionally
holds, and each is a conditional requirement the Define-XML specification
places on a regulatory submission:

- every dataset declares `submission.domain` or inherits one, for the `sdtm`
  and `send` families;
- every dataset's `SASDatasetName` and every column's `SASFieldName` is
  written, so R026-43 must admit every name;
- every dataset carries a `def:ArchiveLocationID`, unless it declares
  `has_no_data`;
- every dataset carries a `Description` and a `def:Class`;
- every dataset declares at least one key, which R005 already requires, and
  every key column carries a `KeySequence`;
- every column carries a `def:Origin`, which R024-31 already requires; and
- every codelist that declares `items` and is drawn from a standard whose
  name is `CDISC/NCI` declares `alias`, and so does every one of its items. An
  external codelist is excluded: it represents its dictionary rather than
  CDISC controlled terminology, and has no items to identify.

## Serialization

**R026-46.** The document is encoded UTF-8 and carries no byte-order mark.
`U+000A` terminates every line, including the last. `U+000D` is never written.

**R026-47.** The remaining byte-level decisions are fixed exactly:

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
  space, each as `Name="value"`, in the order this rule's mapping tables list
  them, with namespace declarations first on `ODM`;
- in an attribute value, `&` is written `&amp;`, `<` is written `&lt;`, `>` is
  written `&gt;`, and `"` is written `&quot;`;
- in text content, `&` is written `&amp;`, `<` is written `&lt;`, and `>` is
  written `&gt;`; and
- no comment, processing instruction other than the stylesheet, CDATA
  section, or blank line is written.

**R026-48.** `U+000D` and `U+0009` must not appear in any generated attribute
value or text, and `U+000A` must not appear in an attribute value. An XML
parser normalizes each of them on the way back in, so a document carrying one
does not read back as it was written, and a byte contract over text that does
not survive parsing is not a contract.

**R026-49.** The bytes are fixed rather than the information. Two conforming
implementations produce byte-identical files, and a golden document is
compared byte for byte. Canonical-XML equivalence was the alternative and is
weaker where it matters: a document is reviewed as a diff and submitted as a
file, and two files that differ only in whitespace still differ in review.

**R026-50.** Publication follows R020's procedure exactly: the complete
document is written to a temporary regular file in the target's directory,
flushed and closed, and atomically moved onto the target; a failure leaves the
previous file and no residue. Generation completes before publication begins,
so a failure at any point publishes nothing.

**R026-51.** The generated document must be valid against the Define-XML 2.1
schema. Conformance to this rule is not a substitute for that validation, and
an implementation that produces a schema-invalid document has a defect
regardless of which requirement above it satisfied.

## What this rule does not generate

**R026-52.** The following are deliberately outside this first contract. None
has a field in the schema R024, R025, and `schema_define.yaml` close, so a
specification that tries to declare one is rejected as an unknown field rather
than generating a document that quietly omits it. That closure is the refusal:
a construct is either declared and generated, or has nowhere to be written.

| Construct | Re-entry trigger |
|---|---|
| `def:ValueListDef`, `def:WhereClauseDef`, value-level `def:Origin` | a row-template expansion that lets one specification declare per-value metadata over a `--TESTCD`-style column |
| `arm:AnalysisResultDisplays` | an analysis-results metadata design |
| Split datasets and their `Alias` domain description | a specification construct that produces one dataset in several files |
| `def:IsNonStandard` on a dataset or column | a conformance-profile rule that owns what non-standard means |
| A comment on a standard, a codelist, or the metadata version | a shared comment identifier space |
| Multiple `def:Origin` elements on one column | the value-level metadata above, which is how Define-XML expresses several provenances |

**R026-53.** Two further constructs have a declaration but no generation, and
each is stated rather than silently dropped. An `Alias` carrying a transport
name longer than eight characters is not generated, so R026-43 omits the
attribute and R026-63 fails under `Submission` instead; a naming rule that owns
transport names would supply it. A `FormalExpression` is generated only from a
declared `method.expression` and never from a derivation, for the reason
R024-47 gives.

**R026-54.** Value-level metadata is the most consequential of these. A
findings domain needs one definition per `--TESTCD` value, and R005 forbids a
data-dependent column list, so the construct that would carry it does not
exist yet. Designing the mapping before that construct exists would fix a
shape the construct then has to match, which is the failure this deferral
avoids.

## Rationale

A Define-XML document is study-level and a specification is dataset-level, so
the composition needs a document of its own; deriving it from a directory
would make the contents of a submission a property of a filesystem. Every
identifier is built from declared names because a reviewer reads a regenerated
document as a diff of the previous one, and a counter or a hash makes an
unrelated edit renumber the file. Definitions are not shared between datasets
because sharing silently requires two specifications to agree forever.
Timestamps and file identity are declared because a document generated from a
clock is different on every run, which defeats both the golden fixture and the
review. The bytes are fixed rather than the information for the same reason
R020 fixes the `csv` profile's bytes: an artifact that is read as a diff is
compared as bytes. The deferrals are refusals rather than silences so that a
specification cannot carry metadata the generator quietly drops, which is the
one failure a submission document must never have.

## Errors

**R026-55.** An `output.path` whose extension is not `.xml`: fail validation.
**R026-56.** A specification named by an entry that declares no
`root.submission`, or an output column with no `label` or no
`column.submission`: fail with `submission_metadata_missing`, reporting the
dataset and the column.
**R026-57.** A `standard`, `codelist`, or document reference naming no
declaration: fail with `unknown_standard`, `unknown_codelist`, or
`unknown_document`, reporting the referencing position.
**R026-58.** A duplicate dataset `id`, standard `id`, codelist `id`, or
document `id`; or a document `id` equal to a dataset `id`: fail with
`duplicate_define_identifier`.
**R026-59.** Two entries naming the same specification, or resolving to the
same artifact path: fail with `duplicate_dataset_entry`.
**R026-60.** More than one document of kind `annotated_crf`: fail validation.
**R026-61.** A `PublishingSet` declared on an `IG` standard or absent from a
`CT` standard: fail validation.
**R026-62.** An artifact path that cannot be expressed relative to the
document's directory without a parent traversal: fail with
`artifact_outside_document`, reporting the dataset and both paths.
**R026-63.** A `has_no_data` dataset with no `submission.comment`: fail
validation and report the dataset.
**R026-64.** A transport name R026-43 does not admit, under `Submission`: fail
with `sas_name_too_long`, reporting the name.
**R026-65.** Any requirement R026-45 adds that is unmet under `Submission`:
fail, reporting the requirement and the position.
**R026-66.** A `U+000D`, `U+0009`, or attribute-value `U+000A` in generated
text: fail with `untransportable_text`, reporting the declaration that carries
it.
**R026-67.** A generated document that is not valid against the Define-XML 2.1
schema: fail and report the schema violation. The run publishes nothing.
**R026-68.** A failed publication: fail and report the target, exactly as
R020 does. The previous document is unchanged.
