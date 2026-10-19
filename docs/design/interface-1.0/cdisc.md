# CDISC alignment, reserved spellings and vocabularies

Part of the [1.0 interface RFC](../interface-1.0.md) (#3030 Phase 0 item 5,
section 2.5's RFC items, D16, D17, D19, D23 to D26 and D29, section 2.7's
vocabularies, and the two CDISC items of #3048).

Every CDISC statement here is a paraphrase with a section citation; nothing is
copied from an implementation guide. Measured at main `1758ad145` by the
submission walker of the [method page](method.md), whose corpus extracts
fewer test strings than the spelling walker's, so its totals can differ
slightly (174 origins against 202, for example).

## Sources

| Source | Where it was read |
| --- | --- |
| Define-XML 2.1 specification | the CDISC reference archive |
| Dataset-JSON 1.1 specification | the CDISC reference archive |
| SDTM v2.0, SDTMIG 3.4, SDTM-MSG 2.0 | the CDISC reference archive |
| ADaMIG 1.3, ADaM OCCDS 1.1, ADaM-MSG 1.0 | the CDISC reference archive |
| SENDIG 3.1.1 | the CDISC reference archive |
| Define-XML 2.1.11 schemas, whose enumerations come from the Define-XML controlled terminology package of 2026-03-27 | in this repository: `rust/tests/schemas/define/2.1/` |
| ODM 1.3.2 schema | in this repository: `rust/tests/schemas/odm/1.3.2/` |
| Dataset-JSON 1.1 JSON Schema | in this repository: `rust/tests/schemas/dataset.schema.json` |
| ODM 1.3.2 and ODM 2.0 specifications | public CDISC documents; section numbers marked "to verify" |
| FDA Study Data Technical Conformance Guide | public FDA document; section numbers marked "to verify" |
| SAS transport format (TS-140) | public SAS document |
| CDISC controlled terminology releases | public; the package the bundled schema embeds is cited where possible |

No Pinnacle 21 rule is cited.

## Alignment table

Status: **aligned**; **derived** (generated from other declarations, no
field); **gap** (CDISC has it, yamaa has no field); **differs** (yamaa
departs from the guide, deliberately or not); **reserve** (a spelling is
reserved below).

### Project file, study and section options

| yamaa field | CDISC counterpart | Citation | Status and note |
| --- | --- | --- | --- |
| `study.id` | `Study/@OID` (`STDY.<id>`, DEFINE-9); Dataset-JSON `studyOID` | Define-XML 2.1 section 5.3.4; Dataset-JSON 1.1 top-level attributes | aligned |
| `study.name`, `.description`, `.protocol_name` | `GlobalVariables` `StudyName`, `StudyDescription`, `ProtocolName` | Define-XML 2.1 sections 5.3.4.1 to 5.3.4.4 | aligned |
| `codelists` | `CodeList` elements; a `def:Standard` of type CT per source | Define-XML 2.1 sections 4.1, 5.3.13 | aligned |
| section key (`sdtm`, `adam`, `send`) | one Define-XML document per model; `ItemGroupDef/@Purpose` | Define-XML 2.1 section 5.3.11 | aligned; `Purpose` derived (DEFINE-15) |
| `<section>.specs` | `ItemGroupDef` order | SDTM-MSG 2.0 section 2 recommends class order then name; ADaM-MSG 1.0 section 2 sets no order | aligned; DEFINE-5 leaves order to the listing |
| `<section>.standard` (1.0: `standards`) | `def:Standards/def:Standard` of type IG, one or more | Define-XML 2.1 sections 4.1, 4.1.2.2, 4.1.2.3, 5.3.6, 5.3.6.1 | gap closed by D16 (P-STD-1) |
| `standard.name` | `def:Standard/@Name` | Define-XML 2.1 section 4.1.1; bundled schema `StandardName` (11 values) | aligned with the schema; section 4.1.1 also lists SDTMIG-PGx, which the schema's package lacks |
| `standard.version` | `def:Standard/@Version` | Define-XML 2.1 section 4.1.2 | aligned |
| `standard.status` (`Draft`, `Final`) | `def:Standard/@Status`: Draft, Provisional, Final | Define-XML 2.1 sections 4.1.1, 4.1.2.2; schema `StandardStatus` | **gap**: `Provisional` missing (P-STD-1, AF-1) |
| none | `def:Standard/@def:CommentOID`, `MetaDataVersion/@def:CommentOID` | Define-XML 2.1 sections 5.3.5, 5.3.6.1 | gap; excluded by DEFINE-26 (keep) |
| `<section>.define` | the document, named `define.xml` | SDTM-MSG 2.0 section 2; ADaM-MSG 1.0 section 2 | aligned (DEFINE-2) |
| `<section>.documents` | `def:leaf`, `def:AnnotatedCRF`, `def:SupplementalDoc` | Define-XML 2.1 sections 4.6, 5.3.7, 5.3.8, 5.3.16 | aligned; DEFINE-14's single annotated CRF is stricter than Define-XML and matches the single `acrf.pdf` practice (keep) |
| `<section>.datasets` (1.0: `transport`) | Dataset-JSON 1.1 or SAS Transport v5 files | Dataset-JSON 1.1; TS-140; FDA TCG (to verify) | aligned; spelling P-PRJ-1 |
| `<section>.context` | `ODM/@def:Context`: Submission, Other | Define-XML 2.1 sections 5.3.3, 4.9 | aligned (DEFINE-4) |
| `<section>.xml_lang` | `TranslatedText/@xml:lang` | Define-XML 2.1 section 5.3.9.1.1 | aligned |
| `<section>.file_oid` | `ODM/@FileOID`; Dataset-JSON `fileOID` | Define-XML 2.1 section 5.3.3 | aligned |
| `<section>.metadata_version` | `MetaDataVersion` `@OID`, `@Name`, `@Description` | Define-XML 2.1 section 5.3.5 | aligned |
| `<section>.originator` | `ODM/@Originator`; Dataset-JSON `originator` | Define-XML 2.1 section 5.3.3 | aligned |
| `<section>.stylesheet` | the `xml-stylesheet` processing instruction | Define-XML 2.1 section 5.3.2 | aligned |
| `<section>.source_system_version` | `ODM/@SourceSystemVersion`; Dataset-JSON `sourceSystem.version` | Define-XML 2.1 section 5.3.3 | aligned |
| none | `ODM/@AsOfDateTime`; Dataset-JSON `dbLastModifiedDateTime` | Define-XML 2.1 section 5.3.3; Dataset-JSON 1.1 | gap; optional in both; additive later |

Measured: 103 project documents (27 benchmark, 72 inline in Rust, 3 in
markdown, 1 in an R test); 68 sections (35 `sdtm`, 32 `adam`, 1 `send`); 27
declare a standard, each exactly one; 10 `annotated_crf` and 4 `supplemental` documents; contexts 50 default,
16 `Submission`, 2 `Other`; transports 8 `json`, 11 `xpt`.

### Dataset metadata (root `submission`)

| yamaa field | CDISC counterpart | Citation | Status and note |
| --- | --- | --- | --- |
| `label` | `ItemGroupDef/Description`; Dataset-JSON `label` | Define-XML 2.1 sections 5.3.11, 4.9; SDTMIG 3.4 section 3.2 | aligned |
| `class` | `def:Class/@Name` | Define-XML 2.1 sections 4.2, 5.3.11.2; schema `ItemGroupClass` (16 values) | aligned: the 16 values equal the schema's |
| `subclass` | `def:SubClass/@Name`; nested subclasses use `@ParentClass` | Define-XML 2.1 section 5.3.11.2.1; schema (5 values) | aligned for one level |
| `structure` | `@def:Structure` | Define-XML 2.1 section 5.3.11; SDTMIG 3.4 section 3.2.1 | aligned |
| `repeating` | `@Repeating` | Define-XML 2.1 section 5.3.11 | aligned |
| `reference_data` | `@IsReferenceData`, non-repeating | Define-XML 2.1 section 5.3.11 | aligned (SUB-4) |
| `domain` | `@Domain`, required for SDTM and SEND except RELREC and POOLDEF; the parent domain for SUPP-- and split datasets | Define-XML 2.1 sections 5.3.11, 4.9 | aligned (SUB-5) |
| `has_no_data` | `@def:HasNoData`, with a comment | Define-XML 2.1 section 5.3.11 | aligned (DEFINE-17) |
| `comment` | `@def:CommentOID` and `def:CommentDef` | Define-XML 2.1 sections 5.3.11, 5.3.15 | aligned |
| derived | `@Purpose`, `@SASDatasetName`, `@def:ArchiveLocationID`, `@def:StandardOID`, `def:leaf`, `ItemRef` list | Define-XML 2.1 sections 5.3.11, 5.3.16 | derived (DEFINE-15, DEFINE-16, DEFINE-18) |
| root `keys` | `ItemRef/@KeySequence`; Dataset-JSON `keySequence` | Define-XML 2.1 section 5.3.9.2; SDTMIG 3.4 sections 3.2.1.1, 4.1.9 | aligned, with one deliberate difference: yamaa keys are non-missing and unique (OUTPUT-3), so natural keys with nullable parts are not expressible; keep (an invariant) |
| none | `@def:IsNonStandard` on a custom domain or a non-ADaM analysis dataset | Define-XML 2.1 sections 4.1, 5.3.11, 5.3.11.2 | gap; excluded by DEFINE-26; **reserve** `submission.non_standard` |
| none | `Alias` with Context DomainDescription on split and SUPP-- datasets | Define-XML 2.1 sections 4.2.1.3, 4.9, 5.3.11.1 | gap; the guide is internally inconsistent on whether submissions use it; later, with D29, derived from the DOMAIN codelist |
| (1.0) `submission.standard` | `@def:StandardOID` naming one of several standards | Define-XML 2.1 section 4.1.2.2 | new field (P-STD-1) |

Measured: 24 specifications declare root `submission` (19 benchmark, 5
inline). Classes: SPECIAL PURPOSE 10, FINDINGS 4, BASIC DATA STRUCTURE 3,
SUBJECT LEVEL ANALYSIS DATASET 2, and one each of OCCURRENCE DATA STRUCTURE,
EVENTS, RELATIONSHIP and TRIAL DESIGN.

### Column metadata (`column.submission`)

| yamaa field | CDISC counterpart | Citation | Status and note |
| --- | --- | --- | --- |
| `core` | the guides' Core column; no Define-XML attribute | SDTMIG 3.4 section 4.1.5; ADaMIG 1.3 section 3; OCCDS 1.1 section 3.2 says Core is not define.xml metadata | aligned (SUB-10) |
| `mandatory` | `ItemRef/@Mandatory` | Define-XML 2.1 sections 4.2.1.1, 5.3.9.2 | aligned (SUB-10, SUB-11) |
| `role` | `ItemRef/@Role`: an SDTM or SEND role, not used for ADaM | Define-XML 2.1 section 5.3.9.2; SDTM v2.0 section 2.1; SENDIG 3.1.1 section 2.4 | **differs**: SUB-9 leaves roles open; P-VOC-1 closes them. `RoleCodeListOID` has no field (gap, rarely used) |
| `data_type` | `ItemDef/@DataType` | Define-XML 2.1 section 4.3.1; ODM 1.3.2 schema | aligned |
| `length` | `ItemDef/@Length`, the planned maximum | Define-XML 2.1 sections 5.3.12, 4.5; Dataset-JSON 1.1 | **differs, deliberately**: with a transport, SUB-24 writes the longest published value (#2855 decision 18; FDA TCG practice, to verify). A value-level length above the column's is refused by no card (gap, to verify in code) |
| `significant_digits` | `ItemDef/@SignificantDigits`, required with float | Define-XML 2.1 section 5.3.12 | aligned (SUB-7) |
| `display_format` | `@def:DisplayFormat`, a SAS format | Define-XML 2.1 section 5.3.12 | aligned (SUB-9, XPT-9) |
| `codelist` | `CodeListRef` | Define-XML 2.1 section 5.3.12.1 | aligned (CT-1) |
| `inventory_vocabulary` | none; supports SRCDOM, whose values name a domain or dataset | ADaMIG 1.3 section 3.3.9 | yamaa-only check (CT-11); keep |
| `origin` (1.0: `origins`) | `def:Origin`, zero or more per ItemDef | Define-XML 2.1 sections 4.3.2, 5.3.12.3; schema (unbounded) | gap closed by D16 (P-ORIGIN-1) |
| `method` | `ItemRef/@MethodOID` and `MethodDef`, required for Derived | Define-XML 2.1 sections 5.3.9.2, 5.3.14 | aligned (SUB-18, SUB-21) |
| `comment` | `ItemDef/@def:CommentOID` | Define-XML 2.1 section 5.3.12 | aligned |
| `value_level` | `def:ValueListRef`, `def:ValueListDef`, `def:WhereClauseDef` | Define-XML 2.1 sections 4.5, 5.3.9, 5.3.10, 5.3.12.2 | aligned for EQ and IN (P-VLM-1) |
| none | `ItemRef/@def:HasNoData`, required in submissions for a variable without data | Define-XML 2.1 section 5.3.9.2; SDTMIG 3.4 section 4.1.5 | **gap**; **reserve** column `submission.has_no_data` |
| none | `ItemRef/@def:IsNonStandard` for a non-standard SDTM or SEND variable | Define-XML 2.1 sections 4.1.2.3, 5.3.9.2 | gap; **reserve** column `submission.non_standard` |
| none | `ItemDef/Alias` for a SAS name longer than eight characters | Define-XML 2.1 sections 4.3.3.6, 5.3.11.1 | excluded by DEFINE-22 and DEFINE-26 (keep) |

Measured: 170 columns carry `submission` (134 benchmark). Fields: `origin`
170, `core` 136, `length` 135, `role` 88, `mandatory` 33, `codelist` 17,
`value_level` 15, `method` 12, `significant_digits` 7, `display_format` 4,
`data_type` 3, `inventory_vocabulary` 2, `comment` 1. Roles used: Record
Qualifier 35, Identifier 34, Topic 11, Synonym Qualifier 4, Variable
Qualifier 2, Timing 2, all SDTM v2.0 section 2.1 roles.

### Origins

| yamaa field | CDISC counterpart | Citation | Status |
| --- | --- | --- | --- |
| `type` | `def:Origin/@Type` | Define-XML 2.1 sections 4.3.2, 5.3.12.3; schema `OriginType` (7 values) | aligned: the 7 values equal the schema's |
| `source` | `def:Origin/@Source` | Define-XML 2.1 sections 4.3.2, 5.3.12.3; schema (4 values) | aligned values; pairs below |
| `description` | `def:Origin/Description`; a Predecessor names `<dataset>.<variable>` | Define-XML 2.1 section 4.3.2.2 | aligned (SUB-16) |
| `documents` | `def:DocumentRef` with `def:PDFPageRef`; required for Collected from Investigator or Subject | Define-XML 2.1 sections 5.3.7.1, 5.3.7.1.1, 5.3.12.3 | aligned (SUB-17, DEFINE-14) |

Pair rules (SUB-15) against the guide:

| Model | Define-XML 2.1 | yamaa today | Status |
| --- | --- | --- | --- |
| SDTM | section 4.3.2.1: Collected from Subject, Investigator or Vendor; Derived and Assigned from Vendor or Sponsor; Protocol from Sponsor; Predecessor without a source; Not Available not valid | the same, plus Not Available without a source, plus Other with any source | **differs**: Not Available admitted (#2855 decision 17), owner question Q7; Other is in the controlled terminology but in neither section 4.3.2 table |
| SEND | section 4.3.2.3: only the type is used, Not Available for legacy data; section 5.3.12.3: the source is not required | every type; source absent or an SDTM pair | the guide is internally inconsistent; yamaa follows section 5.3.12.3 (keep, document) |
| ADaM | section 4.3.2.2: Derived and Assigned from Sponsor, Predecessor without a source; Collected and Protocol generally not used; Not Available not valid | Derived, Assigned, Predecessor, Other; source derived | aligned except Other, as for SDTM |

Define-XML 2.1 section 5.3.12.3 says a parent ItemDef should carry no origin
when every value-level definition carries one; DEFINE-19 always writes the
column's origin (a "should", later: AF-8). SDTMIG 3.4 section 4.1.8.2 and
SDTM-MSG 2.0 section 2 describe mixed provenance through value-level
metadata, and section 5.3.12.3 admits several origins only when no value list
can separate the rows: several origins will be rare.

Measured: 174 origin declarations; `type` 174, `source` 117, `documents` 42,
`description` 32. Other is used 0 times; Not Available once, in an inline test.

### Methods, comments and document references

| yamaa field | CDISC counterpart | Citation | Status |
| --- | --- | --- | --- |
| `method` | `MethodDef`, one per distinct method, referable from several variables | Define-XML 2.1 sections 4.7, 5.3.14 | aligned in content; **differs** in sharing: DEFINE-9 mints one MethodDef per column (valid, less economical; P-POOL-1) |
| `method.name`, `.type`, `.description` | `@Name`, `@Type` (Computation, Imputation), `Description` | Define-XML 2.1 section 5.3.14 | aligned |
| `method.expression` | `FormalExpression`, zero or more | Define-XML 2.1 sections 4.7.4, 5.3.14.1 | **gap**: one only; **reserve** `expressions` |
| `method.documents` | `def:DocumentRef` | Define-XML 2.1 section 5.3.14 | aligned |
| `comment` | `def:CommentDef`, one per distinct comment | Define-XML 2.1 sections 4.8, 5.3.15 | aligned in content; sharing excluded by SUB-22 and DEFINE-26 |
| `pages.type`, `.refs`, `.title` | `def:PDFPageRef` `@Type`, `@PageRefs`, `@Title` | Define-XML 2.1 section 5.3.7.1.1 | aligned |
| none | `@FirstPage` and `@LastPage`; several `def:PDFPageRef` | Define-XML 2.1 section 5.3.7.1.1 | gap; **reserve** `first` and `last`; several references are several `documents` entries |

Measured: 12 methods, all text, all distinct; 2 comments, distinct; 42
document references, all with pages.

### Value-level metadata

| yamaa field | CDISC counterpart | Citation | Status |
| --- | --- | --- | --- |
| `id` | the per-value OIDs `IT.`, `WC.`, `MT.`, `COM.<ds>.<col>.<id>` | Define-XML 2.1 section 3.5.1 | aligned (DEFINE-9) |
| `where` | `def:WhereClauseDef` with one `RangeCheck` per column: EQ for one value, IN for several | Define-XML 2.1 sections 4.5.1, 5.3.10, 5.3.10.1, 5.3.10.2 | aligned for EQ and IN |
| several columns | several RangeChecks, combined with AND | Define-XML 2.1 sections 4.5.1, 5.3.10.1 | aligned |
| none | comparators NE, NOTIN, LT, LE, GT, GE | Define-XML 2.1 section 5.3.10.1 | gap; **reserve**, NE and NOTIN admitted if Q8 is approved |
| none | several `def:WhereClauseRef` on one ItemRef, combined with OR | Define-XML 2.1 sections 4.5.1, 5.3.9.2.1 | gap; **reserve** `where_any` |
| none | where columns of another subject-level dataset, with a comment describing the join | Define-XML 2.1 sections 4.5.1, 5.3.10 | gap; **reserve** qualified where columns |
| `label` | per-value `ItemDef/Description` | Define-XML 2.1 section 5.3.12 | aligned (DEFINE-27) |
| other fields | per-value ItemDef and ItemRef; a value-level length must not exceed the parent's; Role is not redefined at value level except for non-standard variables | Define-XML 2.1 sections 4.5, 5.3.12 | **differs**: SUB-12 admits a value-level `role` (SDTMIG 3.4 section 3.2.1.2 lets roles differ per test; keep); length and codelist compatibility unchecked (AF-9) |
| none | a SUPP-- dataset describes QVAL with a value list | Define-XML 2.1 section 5.3.9 | gap; later, with D29 |

Measured: 23 entries (16 benchmark) on AVAL 5, LBORRES 4, RESULT 2, and one
each on AVALC, AVALU, LBORRESU and QVAL. Where columns: LBTESTCD 10, PARAMCD
8, QNAM 2, LBSPEC 2, ATPT 1, VSTESTCD 1. 22 clauses name one column; 21
column lists hold one value (EQ) and 3 several (IN).

### Codelists and terminology standards

| yamaa field | CDISC counterpart | Citation | Status |
| --- | --- | --- | --- |
| `standard.name` (CT) | `def:Standard/@Name` of type CT (`CDISC/NCI`) | Define-XML 2.1 section 4.1.1; schema `StandardName` | **differs**: free text in the schema, refused only at generation; close it in the vocabulary data (P-VOC-1, AF-3) |
| `standard.publishing_set` | `@PublishingSet` | Define-XML 2.1 section 4.1.1; schema (5 values) | aligned |
| `standard.version` | `@Version`, a date for CT | Define-XML 2.1 section 4.1.2.1 | aligned |
| none | `@Status` of a CT standard | Define-XML 2.1 section 5.3.6.1 | fixed `Final`; additive later |
| `id` | `CodeList/@OID` (`CL.<id>`) | Define-XML 2.1 section 5.3.13 | aligned |
| `name` | `CodeList/@Name` | Define-XML 2.1 section 5.3.13 | aligned (CT-1) |
| `data_type` | `CodeList/@DataType` | Define-XML 2.1 section 5.3.13 | aligned |
| `extensible` | none; a property of the published codelist | CDISC Controlled Terminology | yamaa-only, drives CT-6; keep |
| `alias` | `Alias` with Context nci:ExtCodeID, required in submissions for CDISC CT | Define-XML 2.1 sections 4.4, 5.3.11.1, 4.9 | aligned (DEFINE-4); the name stays (P-PRJ-3) |
| `format_name` | `@SASFormatName`, which starts with `$` for a text codelist | Define-XML 2.1 section 5.3.13 | **gap**: the `$` rule is unchecked (AF-7) |
| `items` | `EnumeratedItem` or `CodeListItem` | Define-XML 2.1 sections 5.3.13.1, 5.3.13.2 | aligned (DEFINE-20) |
| `items[].value`, `.decode`, `.rank`, `.alias`, `.extended` | `@CodedValue`, `Decode`, `@Rank`, `Alias`, `@def:ExtendedValue` | Define-XML 2.1 sections 5.3.13.1 to 5.3.13.3 | aligned (CT-4 to CT-6); document order instead of `@OrderNumber` (allowed) |
| `external.dictionary`, `.version`, `.href` | `ExternalCodeList` `@Dictionary`, `@Version`, `@href` | Define-XML 2.1 section 5.3.13.4 | aligned; `@ref` has no field (**reserve** `external.ref`) |
| none | `CodeList/Description`, item `Description`, `CodeList/@def:CommentOID` | Define-XML 2.1 section 5.3.13 | gap; additive later |
| derived | `@def:StandardOID` or `@def:IsNonStandard` | Define-XML 2.1 section 5.3.13 | derived (DEFINE-20) |

Measured: 48 codelists, 66 items; sources citing CDISC/NCI 10 (publishing
set SDTM 9, ADaM 1), the rest sponsor-defined.

### Dataset-JSON 1.1 and SAS Transport v5

| yamaa rule | Counterpart | Citation | Status |
| --- | --- | --- | --- |
| JSON-7, member order and sources | top-level attributes in the documented order | Dataset-JSON 1.1 | aligned; `dbLastModifiedDateTime` never written |
| JSON-3, no file for `has_no_data` | `records` may be 0 | Dataset-JSON 1.1 | differs, deliberately (matches DEFINE-16) |
| JSON-9, column members | `itemOID`, `name`, `label`, `dataType`, `targetDataType`, `length`, `displayFormat`, `keySequence` | Dataset-JSON 1.1 | aligned |
| JSON-10, data types | supported combinations; ADaM dates as text with `targetDataType: integer` | Dataset-JSON 1.1 | aligned |
| JSON-4, single-object form | an NDJSON representation | Dataset-JSON 1.1 | NDJSON excluded by design (keep) |
| XPT-5, XPT-7, DEFINE-22 | names up to 8 characters, labels up to 40, values up to 200 bytes, ASCII | TS-140; FDA TCG (to verify) | aligned |
| XPT-10 | files above 5 GB are split | FDA TCG (to verify) | aligned as a refusal; splitting is D29 |

### Alignment findings

| # | Finding | Evidence | Disposition |
| --- | --- | --- | --- |
| AF-1 | Standard `status` lacks Provisional | Define-XML 2.1 sections 4.1.1, 4.1.2.2; schema `StandardStatus` | P-STD-1, or a separate additive PR now |
| AF-2 | DEFINE-9 cites `STD.<standard.id>`, but standards have no `id`; colliding CT sources get a counter | DEFINE-9; `submission_composition.rs` | P-OID-1: separate fix PR |
| AF-3 | A CT standard's name is free text in the schema, closed at generation | schema `StandardName`; `submission_xml.rs` | P-VOC-1 |
| AF-4 | Roles are open (SUB-9) | SDTM v2.0 section 2.1 | P-VOC-1 |
| AF-5 | Not Available admitted for SDTM | Define-XML 2.1 section 4.3.2.1 | Q7 |
| AF-6 | Other admitted for SDTM and ADaM; absent from the section 4.3.2 tables but in the controlled terminology | schema `OriginType` | keep: the terminology is newer, and SDTMIG 3.4 section 4.1.8.1 defers to Define-XML for origin values |
| AF-7 | The `$` prefix of a text `SASFormatName` is unchecked | Define-XML 2.1 section 5.3.13 | small validation fix; no golden affected |
| AF-8 | The column origin is written even when every value-level entry has one | Define-XML 2.1 section 5.3.12.3 | later; document the deviation |
| AF-9 | A value-level length above the column's, and value-level codelists outside the column's, are not refused | Define-XML 2.1 sections 4.5, 5.3.12 | later; verify the code first |
| AF-10 | Variable-level `def:HasNoData` | Define-XML 2.1 section 5.3.9.2; SDTMIG 3.4 section 4.1.5 | reserve |
| AF-11 | `def:IsNonStandard` on datasets and variables | Define-XML 2.1 sections 4.1, 5.3.9.2, 5.3.11 | reserve |
| AF-12 | SUPP-- QVAL value lists and the SUPP-- and split Alias are not required under Submission | Define-XML 2.1 sections 4.9, 5.3.9, 5.3.11.1 | later, with D29 |
| AF-13 | SUB-24 detects SUPP-- datasets by the `SUPP` prefix; SDTMIG names some `SQ` | SDTMIG 3.4 section 8.4.2 | later, with D29 |
| AF-14 | SDTMIG-PGx is listed in Define-XML 2.1 section 4.1.1 but not in the schema's terminology | schema `StandardName` | keep; the vocabulary data adds it when the terminology does |

## Section 2.5 spellings

The decisions page gives each recommendation; this section records the
CDISC evidence behind them.

- **P-JOIN-1.** The inferred key lists include VSDY, DOMAIN and EXTRT, which
  come from the output keys rather than anything written at the read; D23
  makes them visible.
- **P-MODEL-1.** "Model" is CDISC's word for SDTM and ADaM. SENDIG is based
  on the SDTM model (Define-XML 2.1 section 1.5.2), so SEND names a standard
  family rather than a model; it stays distinct because its origins follow
  Define-XML 2.1 section 4.3.2.3 and a SEND section is its own document. The
  vocabulary data holds each model's Purpose: Tabulation for SDTM and SEND,
  Analysis for ADaM.
- **P-COPY-1.** ADaMIG 1.3 section 2.3.1 requires a variable present in ADSL
  and another ADaM dataset to have the same values, type and label; OCCDS 1.1
  section 3.2 lists ADSL and SDTM variables copied without change; ADaM-MSG
  1.0 section 2 asks that SDTM predecessor codelists be carried into ADaM.
  Hence content metadata comes from the producer.
- **P-ORIGIN-1.** Define-XML 2.1 allows zero or more origins per ItemDef
  (section 5.3.12.3) and the schema leaves them unbounded.
- **P-STD-1.** Define-XML 2.1 section 4.1.2.2 shows SDTMIG beside SDTMIG-MD
  (different names), and section 4.1.2.3 shows two SDTMIG versions in one
  document under `Other`, a case section 4.1 asks sponsors to agree with the
  regulator first; hence unique names, with the long form reserved. The
  schema requires `Status`, although section 5.3.6.1 says it defaults to
  Final, so it stays required.
- **P-ODMIN-1.** ODM defines `FileType` (Snapshot or Transactional) and
  `TransactionType` (Insert, Update, Remove, Upsert, Context) on the clinical
  data elements in versions 1.3.2 and 2.0 (section numbers to verify). The
  two ODM fixtures in the repository declare no `FileType`, one carries
  `TransactionType="Insert"`, and none carries an audit record.

## Reserved spellings

A reserved spelling is documented now, admitted by no 1.0 file, and taken by
no other meaning. A 1.0 engine refuses it through ordinary schema admission;
the version rule admits it only in files declaring the version that
introduces it. The rename table's `reserved` block lists each one. No
reserved spelling is a union of one value and a list, which SCHEMA-10 makes a
bundle error.

### Value-level comparators and OR (D17, P-VLM-1)

| Option | Spelling | Assessment |
| --- | --- | --- |
| **V1** | **`{comparator: NE, values: [AVERAGE]}` beside the list of values** | **Define-XML's own words, `Comparator` and its values (section 5.3.10.1; I3); SCHEMA-10's shorthand shape** |
| V2 | `{not: [AVERAGE]}`, `{ge: 18, lt: 65}` | Invents `not`; ranges overlap P-ORD-2's `closed` |
| V3 | A predicate string, `"DTYPE <> 'AVERAGE'"` | A second form, against D17 |

- The list stays the spelling of EQ (one value) and IN (several); the long
  form restating them is refused as `redundant_long_form`.
- `comparator` takes NE, NOTIN, LT, LE, GT or GE, closed and held in the
  vocabulary data. NE, LT, LE, GT and GE take one value; NOTIN two or more.
- A missing value matches no listed value (SUB-12), so EQ and IN never select
  it and NE and NOTIN always do.
- LT to GE admit `int`, `float` and `date` where columns, with values written
  as YAML numbers or dates; SUB-12's `not_str` reason then applies to the list
  forms only. Validation's overlap test gains complement reasoning: an NE
  clause on X and the list `[X]` on one column cannot overlap.
- **OR across clauses.** Define-XML combines several `def:WhereClauseRef` on
  one value-list ItemRef with OR (sections 4.5.1, 5.3.9.2.1). Reserved:
  `where_any`, a list of two or more clause mappings, written instead of
  `where`. Clause *k* of entry *id* generates `WC.<ds>.<col>.<id>.<k>`; a
  position in one entry's own list is a declared position, as `OrderNumber`
  is, so DEFINE-9's ban on counters holds.

```yaml
value_level:
  - id: GLUC_URINE_OR_KETONES
    where_any:
      - {LBTESTCD: [GLUC], LBSPEC: [URINE]}
      - {LBTESTCD: [KETONES]}
```

- **Cross-dataset where columns.** Reserved: qualified keys (`DM.COUNTRY`) for
  columns of another subject-level dataset, with a required comment
  describing the join (Define-XML 2.1 sections 4.5.1, 5.3.10).

| Keyed case | Typical where | 1.0 form | Needs a reserved form when |
| --- | --- | --- | --- |
| ADaM BDS by PARAMCD | `{PARAMCD: [ALT, AST]}` | yes | derived records differ by DTYPE (NE, NOTIN) or by visit ranges (LT to GE) |
| `--TESTCD` with qualifiers | `{LBTESTCD: [GLUC], LBSPEC: [URINE]}` | yes | a qualifier is missing on some rows (NE, NOTIN), or two slices share a definition (OR) |
| SUPP-- by QNAM | `{QNAM: [RACE1, RACE2]}` | yes | never in the corpus |
| TS by TSPARMCD | `{TSPARMCD: [AGEMIN]}` | yes | never in the corpus |

### Shared definitions and pools (D24, P-POOL-1)

Define-XML 2.1 expects one MethodDef and one CommentDef per distinct method or
comment, referable from several variables (sections 5.3.14, 5.3.15), and
shares ItemDefs across split datasets with identical metadata (section
4.2.1.3). yamaa's per-column definitions are valid, only less economical, and
the corpus shows no demand. Reserved:

| Feature | Reserved spelling | Generated identity |
| --- | --- | --- |
| Method pool | project `methods: [{id, name, type, description, expressions, documents}]` | `MT.<id>`, one segment, disjoint from `MT.<ds>.<col>` |
| Method reference | `method: {id: STUDY_DAY}`, a mapping holding only `id`; a mapping with `description` stays an inline method | |
| Comment pool | project `comments: [{id, text, documents}]` | `COM.<id>` |
| Comment reference | `comment: {id: X}`, on datasets, columns and value-level entries | |
| Shared column definition | project `columns: [{name, type, label, submission}]` and a column entry `use: NAME` | `IT.<name>` |

The text forms of `method` and `comment` cannot double as references, because
bare text is prose today; hence the `{id: ...}` mapping, as codelists are
declared with `id` and bound by it.

### SUPPQUAL and split datasets (D29, P-SUPP-1)

SDTMIG 3.4 sections 8.4 and 8.4.1 define SUPP-- with STUDYID, RDOMAIN,
USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG and QEVAL, one dataset
per parent (section 8.4.2); Define-XML describes QVAL by QNAM with value-level
metadata (sections 4.5.2.3, 5.3.9).

| Reserved spelling | Meaning in 1.x |
| --- | --- |
| column `submission.non_standard: true` | a non-standard SDTM or SEND variable: generation moves it to SUPP-- (QNAM its name, QLABEL its label, QVAL its value, QORIG its origin type, one per record), or under `context: Other` it stays in the parent with `def:IsNonStandard` (Define-XML 2.1 section 5.3.9.2) |
| column `submission.evaluator` | QEVAL for that qualifier |
| dataset `submission.supplemental: {dataset: SUPPAE, idvar: AESEQ}` | overrides of the derivable defaults: `SUPP` plus the dataset name (or `SQ` for long names, section 8.4.2), IDVAR the `--SEQ` column |
| dataset `submission.non_standard: true` | a custom domain or a non-ADaM analysis dataset: `def:IsNonStandard="Yes"`, no `def:StandardOID`, and for ADaM no `def:Class` (Define-XML 2.1 sections 4.1, 5.3.11, 5.3.11.2) |
| dataset `submission.split: {by: LBCAT, datasets: {LBCA: [CHEMISTRY], LBHA: [HEMATOLOGY]}}` | split datasets defined once: the transport writes the split files into `split/` beside `define.xml`, and the document describes the domain once |

SDTMIG 3.4 section 4.1.7 splits by `--CAT` or, for FA, by parent domain;
DOMAIN is unchanged and `--SEQ` unique across the splits. SDTM-MSG 2.0
sections 5.5.1 and 5.5.3 show two packaging modes: a domain split for size
and defined once (reserved above), and splits defined separately, which one
specification per split dataset with `submission.domain` already expresses
(SUB-5). The FDA TCG's guidance on large datasets is the regulator's side
(to verify); XPT-10 already refuses files above 5 GB.

### Other reserved spellings (P-CDISC-1)

| Reserved spelling | CDISC counterpart | Citation |
| --- | --- | --- |
| column `submission.has_no_data: true`, with a required comment | `ItemRef/@def:HasNoData` | Define-XML 2.1 section 5.3.9.2; SDTMIG 3.4 section 4.1.5 |
| `pages: {type, first, last}`; several page references as several `documents` entries | `@FirstPage`, `@LastPage`; several `def:PDFPageRef` | Define-XML 2.1 section 5.3.7.1.1 |
| method `expressions: [...]`, instead of `expression` | several `FormalExpression` | Define-XML 2.1 section 5.3.14.1 |
| `external.ref` | `ExternalCodeList/@ref` | Define-XML 2.1 section 5.3.13.4 |
| `submission.standard: {name, version}` | two versions of one guide in one document | Define-XML 2.1 section 4.1.2.3 |
| ODM input `transactions: replay`, `archive_member` | transactional ODM; TAR inputs | P-ODMIN-1 |

## Closed vocabularies as versioned data (P-VOC-1)

| Vocabulary | Values today | Schema source | Check against the bundled schema |
| --- | --- | --- | --- |
| dataset class | 16 | `yaml/schema_metadata.yaml` `dataset_class_name` | equal to `ItemGroupClass` |
| dataset subclass | 5 | `dataset_subclass_name` | equal to `ItemGroupSubClass` |
| standard name | 11 | `yaml/schema_project.yaml` `standard_name` | equal to `StandardName` |
| standard status | 2 | `implementation_guide_class.status` | missing Provisional |
| publishing set | 5 | `terminology_standard_class.publishing_set` | equal to `StandardPublishingSet` |
| origin type | 7 | `origin_type` | equal to `OriginType` |
| origin source | 4 | `origin_source` | equal to `OriginSource` |
| role | open | `role: str` | not in the schema; SDTM v2.0 section 2.1 and SENDIG 3.1.1 section 2.4 define nine |
| core designation | 4 | `core_designation` | not in the schema; SDTMIG 3.4 section 4.1.5, ADaMIG 1.3 |
| submission data type | 15 | `define_data_type` | within the ODM 1.3.2 `DataType` |
| method type | 2 | `method_type` | Define-XML 2.1 section 5.3.14 |
| page reference type | 2 | `page_reference_class.type` | equal to `PDFPageType` |
| context | 2 | `submission_section_class.context` | equal to `ODMContext` |

Each list also has hand-written copies in the core crate's schema rules, the
submission document model and its decoder, and the composition code, so a new
value needs four or five edits today.

| Option | Layout |
| --- | --- |
| L1 | Keep the values in the schema modules |
| **L2** | **One vocabulary file that schemas, Rust enums, JSON Schema, reference pages and rule tables are generated from or checked against** |
| L3 | Read the vocabularies from the bundled schema at build time |

L2 follows the grammar precedent (`yaml/grammar/`). A sketch:

```yaml
# yaml/vocabulary/cdisc.yaml (proposed; not part of this pull request)
version: "1.0"
source: {define_xml: "2.1.11", ct_package_date: "2026-03-27"}
vocabularies:
  model:
    - {value: SDTM, purpose: Tabulation, since: "1.0"}
    - {value: SEND, purpose: Tabulation, since: "1.0"}
    - {value: ADaM, purpose: Analysis, since: "1.0"}
  standard_status:
    - {value: Draft, since: "1.0"}
    - {value: Provisional, since: "1.0"}
    - {value: Final, since: "1.0"}
  role:
    - {value: Identifier, models: [SDTM, SEND], since: "1.0"}
    - {value: Record Qualifier, models: [SDTM, SEND], since: "1.0"}
```

- **Versioned.** `since` is the language version that admitted a value; a file
  declaring an older version that uses it fails under the version rule. Values
  are never removed in 1.x; a value CDISC retires gets `deprecated_since` and
  stays admitted.
- **Anchored.** `source` names the Define-XML schema release and the
  terminology package. A repository check compares each schema-backed list
  with `rust/tests/schemas/define/2.1/define-enumerations.xsd`, so a
  terminology update is one reviewed data change. Roles, core designations
  and models cite the guide sections instead.
- **One source.** Schema modules refer to a vocabulary by name; the Rust
  enums, the per-model tables of SUB-3, SUB-10 and SUB-15, and the reference
  pages are generated from it.
- **Roles closed.** The nine SDTM v2.0 section 2.1 roles (Identifier, Topic,
  Timing, Rule, and the Grouping, Result, Synonym, Record and Variable
  Qualifiers) for SDTM and SEND; none for ADaM, where SUB-9 already refuses a
  role.

## #3048 example metadata

| Finding | Where | Guide | Check on main | Disposition |
| --- | --- | --- | --- | --- |
| SDTMIG orders STUDYID, DOMAIN, USUBJID; the repository check puts DOMAIN first | `tools/validation/repository_artifacts.py` (`KEY_COLUMN_ORDER`, applied to declared columns, `output.columns` and golden CSV headers) | SDTM v2.0 section 3.1.4 lists the identifiers in that order; SDTMIG 3.4 section 4.1.4 asks for the SDTM order | confirmed: 61 benchmark specs and 57 golden CSV headers put DOMAIN first; `docs/articles/api.md` already shows the SDTMIG order | Q6: kept for 1.0. Reordering changes 61 benchmarks' expected files (58 CSV, 3 define.xml, 1 Dataset-JSON, 1 XPT, 1 Parquet) and 100 Harbor prompt files. State the deviation in `benchmarks/AGENTS.md` meanwhile |
| AEDECOD's role | `benchmarks/sdtm-ae-coding/spec.yaml`: Record Qualifier | SDTMIG 3.4 section 6.2.1: Synonym Qualifier | confirmed | separate fix PR, spec only: no expected file holds the role |
| VS EPOCH is Perm | `benchmarks/sdtm-vs-epoch-from-subject-elements/spec.yaml`: Exp, Record Qualifier, Assigned with a method | SDTMIG 3.4 section 6.3.13: Timing, Perm; Define-XML 2.1 section 4.3.2: Derived means computed from other data | confirmed, also the role | separate fix PR: Perm, Timing, Derived from Sponsor keeping the method; `mandatory` stays false |
| VSORRES and VSDTC are Collected | same file: Assigned from Sponsor, Record Qualifier | Define-XML 2.1 section 4.3.2; SDTMIG 3.4 section 6.3.13 (Result Qualifier and Timing) | confirmed, also the roles | separate fix PR, with test updates: Collected from Investigator needs an annotated CRF document in that benchmark's project (DEFINE-14), and the AE and VS define tests in Rust, Python and R expect none for VS; no expected file changes |
| ADSL TRT01A is Cond | `benchmarks/adam-adsl-xpt/spec.yaml`: Req, mandatory | ADaMIG 1.3 section 3.2: TRTxxP Req, TRTxxA Cond | confirmed | separate fix PR, spec only: define.xml carries no Core and `Mandatory` comes from the declared `mandatory`, so the goldens stay byte-identical |

Also checked, no change proposed: AETERM is Exp in `sdtm-ae-coding` against
SDTMIG 3.4 section 6.2.1's Req, and the spec explains why (one blank term
demonstrates the NOT CODED fallback); every other SDTM core and role in the
composed benchmark specs matches SDTMIG 3.4.

## #3048 value-level gaps

| Finding | Check on main | Disposition |
| --- | --- | --- |
| Rows that differ only by DTYPE | Confirmed. For AVAL with `{PARAMCD: [SYSBP]}` for observed rows and `{PARAMCD: [SYSBP], DTYPE: [AVERAGE]}` for derived rows, SUB-12 fails `value_level_overlap`; observed rows have DTYPE missing, and a missing value matches no listed value, so they cannot be selected positively. Per-parameter entries beside a DTYPE-only entry pass validation and fail `value_level_multiple_matches` on every derived row (SUB-13) | The RFC, through P-VLM-1 (Q8): `{PARAMCD: [SYSBP], DTYPE: {comparator: NE, values: [AVERAGE]}}`. Workaround if not: one entry per parameter whose method describes both the copied and the derived values |
| Where values not checked against the where column's codelist | Confirmed: SUB-12 checks the where column's declaration, type and output membership, not its codelist; a misspelt value surfaces only as the run-time warning `value_level_unmatched`. None of the 10 where values in the composed benchmark specs sits on a codelist-bound column | Separate fix PR: a value outside the bound codelist's items fails `value_level_where_invalid` with a new reason `not_in_codelist` (external codelists skipped; extensible lists count their extended items). D32 guarantees the codelist resolves when validation passes. No golden affected; the PR adds a negative benchmark |
