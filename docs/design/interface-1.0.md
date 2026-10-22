# yamaa 1.0 interface design (RFC)

This is the Phase 0 RFC of
[#3030](https://github.com/elong0527/yamaa/issues/3030): the field-by-field
design of the yamaa 1.0 interface. It settles each RFC item of #3030
section 2 with a measurement and a recommendation, which the owner approved
on #3030 (D58 to D71). Decided items (D1 to D53 in #3030 section 4) are
fixed; this page restates them only where it fills in their detail, and it
re-proposes nothing under "Decided against".

**Status: approved.** On 2026-10-19 the owner approved the rename table as a
whole (#3030 D58), accepted the recommended answer to each question below
(D59 to D70) and approved the proposed card text (D71). This page changes no
language, schema, rule card, engine or benchmark by itself: Phase 1 writes
the schemas and cards, and Phase 2's converter applies the approved table.

**Measured at main `1758ad145`.** Every count names its corpus: "benchmark"
counts come from the files under `benchmarks/`, "in all" adds the Rust, host,
docs and tools corpora. The [method page](interface-1.0/method.md) says how
each number was taken.

| Page | What it holds |
| --- | --- |
| This page | The decisions: summary, RFC table, questions for the owner, and the design of each section of #3030 |
| [Rename table](../../tools/migrate/renames.yaml) | `tools/migrate/renames.yaml`: every field, keyword and value of today's schemas with its 1.0 spelling and decision, plus conditions, diagnostic paths and reserved spellings |
| [Spelling review](interface-1.0/spelling.md) | The measurements and options behind section 2.6 |
| [Field kinds](interface-1.0/field-kinds.md) | The table of every field's kind (D31), generated from the rename table |
| [CDISC alignment](interface-1.0/cdisc.md) | Each submission field against Define-XML 2.1, Dataset-JSON 1.1 and the implementation guides; reserved spellings; closed vocabularies; the CDISC items of #3048 |
| [Rules](interface-1.0/rules.md) | The disposition of each rule contradiction, the rules items of #3048, and the proposed card text |
| [Card text beyond D71](interface-1.0/card-text.md) | Proposed titles and statements for the cards 1.0 changes that D71 did not cover, for the owner's approval (#3162) |
| [Card text beyond D78](interface-1.0/card-text-2.md) | Proposed titles and statements for the twelve cards D78 sent back and a new ODM source-profile card, for the owner's approval (#3193) |
| [API and command line](interface-1.0/api.md) | `status`, printed text, `lint()`, `info()`, the command line, the issues table, and the bindings draft |
| [Stability contract](interface-1.0/stability.md) | The contract text and how `make check-repository` enforces it |
| [Converted examples](interface-1.0/examples.md) | Six benchmarks converted by hand to the proposed surface, with notes |
| [Method](interface-1.0/method.md) | How each measurement was taken, and the converter footprint |

The bindings draft is
[`rust/interface/bindings-1.0.yaml`](https://github.com/elong0527/yamaa/blob/yamaa-v0.5.0/rust/interface/bindings-1.0.yaml)
at the yamaa-v0.5.0 tag; `rust/interface/bindings.yaml` replaced it in 1.0
(#3434).

## Summary

- **Most of 0.5.0 stays.** Of 636 entries in the rename table, 572 keep
  today's spelling. 32 changes are decided by #3030 and 32 are proposed here:
  9 spellings for decided features, and 23 changes to today's surface, each
  with a stated reason (I4). Those touch operand names, one handler name,
  endpoint inclusion, `tie_break`, the aggregate `derive` list,
  `minimum_source_precision`, `datasets` and empty mappings.
- **Join keys.** An input declares `key: [STUDYID, USUBJID]`; implicit joins
  and qualified aggregates without their own `key` match on it, and a named
  intermediate requires `key` unless it drives a row template. The converter
  writes today's inferred keys: 86 input declarations and 26 intermediate
  keys in 92 benchmark specifications.
- **New spellings for decided features:** `model: SDTM | SEND | ADaM`,
  `copy: {dataset: ADSL, columns: [...]}`, `origins: [...]`,
  `submission: {standard: NAME}`, `type: codelists`, and a root `clear:` list
  of dotted spec paths.
- **No union of one value and a list.** SCHEMA-10 forbids it, so several
  values always take a plural list field. This decides the origins form and
  corrects three reserved spellings of the drafts.
- **Diagnostics and API.** `status` takes `succeeded`, `failed` or
  `skipped`; Rust renders one summary line and a `first issue:` line for every
  host; `lint()` returns six columns; `info()` six fields; the command line
  has eight commands and exits 0, 1 or 2.
- **Provenance.** The run report stays version-free, as CHECK-24 requires;
  versions and platform come from `info()`, `selftest()` and define.xml.
- **Expected data.** D28 and three other decided items change expected
  files beyond spec paths, which #3030 section 1 does not permit. They are
  owner questions Q1 and Q2, not changes made here.
- **Findings.** Eleven defects and gaps found while measuring are recorded
  for separate fixes, among them an unregistered `cancelled` issue row,
  36 registry citations to fix before the card-ID rewrite, and DEFINE-9's
  description of standard OIDs.

## How to review

1. Read the [RFC table](#rfc-items-and-recommended-answers) and the
   [questions for the owner](#questions-for-the-owner).
2. Read the changed entries of the [rename table](../../tools/migrate/renames.yaml):
   every entry whose `kind` is not `keep`, then its `conditions`, `paths`
   and `reserved` blocks.
3. Compare one or two [converted examples](interface-1.0/examples.md) with
   their benchmarks.
4. Read the section of this page, and the reference page, for any item you
   want to question.

The owner approved the table as a whole on #3030 (D58), so every `proposed`
entry is decided.

## RFC items and recommended answers

Each item has an id (P-...) that the rename table's `ref` field and the
sections below use. "Benchmarks" counts what the converter or a rule change
touches under `benchmarks/`.

| Id | #3030 | Item | Recommended answer | Benchmarks touched |
| --- | --- | --- | --- | --- |
| P-TYPE-1 | 2.1 | Name of the codelist file type | `type: codelists`; inline sources in the project file stay | none (no benchmark uses a codelist file) |
| P-VER-1 | 2.1 | Version a composed chain needs | The newest version any layer declares; each layer is admitted against its own version; a file newer than the engine fails as `unsupported_language_version` | none |
| P-DICT-1 | 2.1 | Mapping dictionary files | Stay header-less data files named by `mapping.dict` | none |
| P-PRJ-1 | 2.2 | `datasets` against `transport` | `transport: xpt` or `transport: dataset_json` | 14 project files |
| P-PRJ-2 | 2.2 | Group the Define-only options | Keep them flat | none |
| P-PRJ-3 | 2.6 | `alias` against `code` | Keep `alias` | none |
| P-STD-1 | 2.2, 2.5 | Selecting one of several standards | `submission: {standard: NAME}`; names unique in a section; `Provisional` joins `status` | none; OIDs unchanged |
| P-FN-1 | 2.3 | Tagged dates in function files | Bare text, typed by the declared parameter or return type | none |
| P-FN-2 | 2.3 | Conditions D8 makes stale | `function_implementation_missing` and `function_file_invalid` | none |
| P-KIND-1 | 2.4 | Table of every field's kind | [Field kinds](interface-1.0/field-kinds.md), generated from the rename table | none |
| P-CLR-1 | 2.4 | Path syntax of `clear:` | A root list of dotted spec paths, keyed members by identity, `['...']` for other segments, no list indexes | 2 child-layer nulls |
| P-CLR-2 | 2.4, 2.7 | One path notation | Diagnostics address row templates and intermediates by `id` | 37 negative spec paths |
| P-NULL-1 | 2.4 | YAML null outside value and expression fields | Refused | deletions only |
| P-JOIN-1 | 2.5 | The input join-key declaration | `key: [COLUMN, ...]` on the input, list form only | 86 input declarations |
| P-KEY-1 | 2.5 | Qualified aggregates without `key` | Match on the input's declared `key` | covered by P-JOIN-1 |
| D23-R1 | 2.5 | D23 against ROW-2 | An intermediate that drives a row template declares no `key` | 5 intermediates stay keyless |
| P-MODEL-1 | 2.5 | `model` | `model: SDTM`, `SEND` or `ADaM`; required with submission metadata or a section listing | 415 entry specs gain it |
| P-COPY-1 | 2.5 | `copy` | `copy: {dataset: ADSL, columns: [...]}`, with content metadata from the producer | 10 columns qualify |
| P-ORIGIN-1 | 2.5 | Several origins | `origins: [...]`, always a list | 145 origins in 28 files |
| P-ODMIN-1 | 2.5 | ODM input policy | Snapshot reading; transactional content refused; audit records skipped | none |
| P-VLM-1 | 2.5 | Value-level comparators and OR | `{comparator, values}` and `where_any` reserved; NE and NOTIN admitted in 1.0 (Q8) | none |
| P-POOL-1 | 2.5 | Shared definitions and pools (D24) | Not in 1.0; spellings reserved | none |
| P-SUPP-1 | 2.5 | SUPPQUAL and split datasets (D29) | Spellings reserved | none |
| P-CDISC-1 | 2.5 | Other gaps the alignment found | Spellings reserved | none |
| P-OP-1 | 2.6 | `study_day.date` | `source` | 17 in 14 files |
| P-OP-2 | 2.6 | `baseline_flag.value` | `source` | 12 in 9 files |
| P-OP-3 | 2.6 | `baseline_flag.reference_date` | `reference` | 12 in 9 files |
| P-HND-1 | 2.6 | One name per handler meaning | Intermediate `no_match` becomes `absent` | 104 written; after P-HND-3, 15 values in all are renamed |
| P-HND-2 | 2.6 | One absence policy | Keep the answer field beside `require_match: true` | none |
| P-HND-3 | 2.6 | Restated defaults | A lint finding with a fix; the converter deletes `{literal: null}` restatements in single-layer specs | deletions only (about 320 in all) |
| P-HND-4 | 2.6 | Empty mappings | Bare `- not_missing`; `rows[].derivations` optional | 60 and 10 |
| P-HND-5 | 2.6 | An unknown variable answering `absent` | Removed (Q9) | none |
| P-BIND-1 | 2.6 | One shape for per-record bindings | Aggregate `derive` becomes a `derivations` map | 13 bindings in 7 files |
| P-BIND-2 | 2.6 | Untyped intermediate `{value: E}` | Removed | none |
| P-IMP-1 | 2.6 | `impute` fields | Drop `minimum_source_precision` (Q3) | 15 in 11 files; 2 negatives retire |
| P-ODM-1 | 2.6 | One idiom for ODM reads | Teach `odm`; reserve `odm.scope`; convert no benchmark (Q12) | none |
| P-WORD-1 | 2.6 | Overloaded words | Keep `standard` on codelists, `environments`, `domain`, `type`, `intermediates`, `dataset` | none |
| P-ORD-1 | 2.6 | `tie_break` | Folds into `window.order_by`, after the date | 2 files |
| P-ORD-2 | 2.6 | Endpoint inclusion | `closed` on `diff` and `cut`; `baseline_flag.inclusive` stays | 17 in 14 files |
| P-ORD-3 | 2.6 | Equality matches as key matches | A stated rule and a lint finding; constant equalities stay in `filter` | none |
| P-LONG-1 | 2.6 | Long forms | Keep the `redundant_long_form` refusal; `lint --fix` rewrites | none |
| P-UNU-1 | 2.6 | Unused surface | Keep `encoding` (SAS inputs), `stylesheet`, codelist files and the zero-use fields | none |
| P-RULE-1 | 2.7 | `requirement` becomes `rule` (D28) | Fix 36 registry citations first; extend the codemod to every authored file (Q1) | 189 negatives, 4 report goldens |
| P-ISSUE-1 | 2.7 | `file`, `line`, `column` | The layer that wrote the deepest written node of the first spec path, the one whose value took effect | none |
| P-MSG-1 | 2.7 | `message` | One template per condition in `yaml/conditions.yaml`, rendered in Rust | none |
| P-PHASE-1 | 2.7 | Phases and stages | The 12 registered phases, defined by a new COLUMN card; "stage" only for COLUMN-8's lifecycle | none |
| P-PROV-1 | 2.7 | Versions and platform in the run report | The report stays version-free (Q4) | none |
| P-VOC-1 | 2.7 | CDISC vocabularies as versioned data | One data file checked against the bundled Define-XML enumerations; roles closed to the nine SDTM roles | none |
| P-OID-1 | 2.7 | DEFINE-9 against the code | Fix DEFINE-9's table and refuse a colliding CT standard, in a separate PR | none |
| P-STATUS-1 | 2.8 | Values of `status` | `succeeded`, `failed`, `skipped` | none |
| P-PRINT-1 | 2.8 | Printed text | One summary line, then `first issue:`, rendered in Rust | none |
| P-LINT-2 | 2.8 | Columns of the `lint()` table | `file`, `line`, `column`, `finding`, `spec_path`, `message` | none |
| P-LINT-1 | 2.8 | The suppression comment | `# yamaa-lint: allow <finding> -- <reason>` | none |
| P-INFO-1 | 2.8 | Fields of `info()` | `version`, `language_versions`, `features`, `host`, `host_version`, `platform` | none |
| P-CLI-1 | 2.8 | Command-line refinements | `--spec NAME`; `--section` on `submission`; exit status 2 only for the command line's own input and output (Q10) | none |
| P-API-1 | 2.8 | Members beyond D36 to D46 | `yamaa.Error.paths` and `Submission.created` (Q11) | none |
| P-STAB-1 | 2.11 | Stability contract | The contract text, seven manifests, and a check against the latest release tag | none |
| P-RULE-2 to P-RULE-9 | Phase 0 item 6 | Rule contradictions | [Dispositions](#rule-contradictions-phase-0-item-6) and card text on the [rules page](interface-1.0/rules.md) | none here |
| P-RULE-10 | #3048 | Rules tightening | New titles and statements, a headline card for missing values, principle P11 | none |
| P-3048-1 | #3048 | Example specification metadata | Four fixes in a separate PR; the DOMAIN-first order kept (Q6) | 3 specs, 1 project file |
| P-3048-2 | #3048 | Value-level where-clause gaps | Codelist check in a separate PR; DTYPE through P-VLM-1 | none |

## Questions for the owner

Each question names its recommendation. Q1, Q2, Q4 and Q5 concern decided
text; the others choose between options or widen a decision. The owner
accepted every recommendation on 2026-10-19, recorded on #3030 as D59 (Q1)
to D70 (Q12), in order.

1. **D28 and the expected files (P-RULE-1).** Renaming `requirement` to
   `rule` rewrites the `requirement:` key of all 189 negative
   `expected/error.yaml` files and 147 REQ values in them, and the RULE
   column of four run-report goldens: `schema-adsl-age-quality`,
   `schema-row-count-fractions`, `schema-verification-log` and
   `schema-parquet`, whose Parquet file must be re-authored independently.
   Two negatives also move from EXPR-1 to WINDOW-9 by hand
   (`negative-tied-baseline`, `negative-baseline-unresolved-tie`). #3030
   section 1 permits only spec-path edits; AGENTS.md makes a golden change an
   owner decision with independent reproduction. **Recommendation:** record
   the decision, with the values taken from the cards' `Replaces` lines and
   every changed file listed in the Phase 2 pull request.
2. **Negatives whose fault changes under decided items.** Three negative
   benchmarks cannot keep their expected condition after conversion:
   `negative-version-mismatch` (D4: the parent's `2.0` is now a version the
   engine does not read), `negative-literal-structure` (D18: `literal` is
   gone) and `negative-lookup-unknown-key` (D23: an intermediate without
   `key` fails as `missing_required_field`). **Recommendation:** re-author
   each `expected/error.yaml` independently for the 1.0 fault in the Phase 2
   pull request, as a golden change.
3. **Retiring two negatives (P-IMP-1).** Dropping `minimum_source_precision`
   retires `negative-impute-missing-month`,
   `negative-impute-unreachable-month` and the conditions `month_required`,
   `month_not_permitted`, `day_required` and `day_not_permitted`, whose only
   fault disappears. **Recommendation:** approve. If no benchmark should
   retire, keep the fields flat as today.
4. **The run report's provenance sentence (P-PROV-1).** #3030 section 2.7
   says the run report records versions and platform. CHECK-24 forbids a
   version or timestamp in the report, I8 requires the same bytes on every
   OS, and 2.11 keeps every 1.0 benchmark's expected bytes in 1.x; four
   benchmarks pin run reports. **Recommendation:** amend the sentence to
   "records each input's path, profile, byte length, record count and
   producer"; versions and platform come from `info()`, the selftest report
   and define.xml's `SourceSystemVersion` (DEFINE-12).
5. **D23's text (D23-R1, P-KEY-1).** ROW-2 forbids `key` on an intermediate
   that drives a row template, and LOOKUP-5 also infers the key of a
   qualified aggregate, which D23 does not mention. D23 cites LOOKUP-1,
   LOOKUP-3 and a third card, number 21 of the LOOKUP series, which #3021
   retired. **Recommendation:** exempt driver intermediates, let keyless
   qualified aggregates use the input's declared `key`, and read D23's card
   list as LOOKUP-1, LOOKUP-2, LOOKUP-3, LOOKUP-4, LOOKUP-5 and LOOKUP-25.
6. **DOMAIN-first key order (P-3048-1).** SDTMIG orders STUDYID, DOMAIN,
   USUBJID; the repository check `KEY_COLUMN_ORDER` puts DOMAIN first, and 61
   benchmarks follow it. Reordering changes their expected files (58 CSV,
   3 define.xml, 1 Dataset-JSON, 1 XPT, 1 Parquet) and 100 Harbor prompt
   files. **Recommendation:** keep the order for 1.0 and state the deviation
   in `benchmarks/AGENTS.md`; this RFC proposes no change.
7. **`Not Available` for SDTM.** SUB-15 admits a `Not Available` origin for
   SDTM, as #2855 decision 17 chose; Define-XML 2.1 section 4.3.2.1 says it
   is not valid for SDTM, and section 4.3.2.3 keeps it for legacy SEND data.
   One inline test uses it. **Recommendation:** refuse it for SDTM and ADaM,
   keep it for SEND.
8. **NE and NOTIN in 1.0 (P-VLM-1).** D17 reserves more comparators. Rows
   that differ only by DTYPE cannot be described today (#3048), and ADaMIG
   1.3 section 3.3.5 asks for each DTYPE derivation in the analysis metadata.
   **Recommendation:** admit NE and NOTIN in 1.0, keep LT, LE, GT, GE and OR
   reserved.
9. **An unknown variable answering `absent` (P-HND-5).** COLUMN-10's
   handler table lets a declared `absent` answer for a variable that does not
   exist, instead of failing. P1 says nothing is inferred and I9 says fail
   instead of choosing; no benchmark relies on it. **Recommendation:** remove
   it, so `absent` answers only a lookup with no record.
10. **Command-line refinements (P-CLI-1).** Add `--spec NAME`, without which
    the command line cannot check, run or resolve one spec of a project; make
    `submission`'s section a `--section` flag, as on `run`; and read D12's
    "IO errors" as the command line's own arguments and standard output, so
    a file problem the engine reports as an issue exits 1, as the hosts
    report it. **Recommendation:** approve all three.
11. **Surface beyond the decision log (P-API-1).** `yamaa.Error.paths` and
    the R condition's `paths` list the files written before a refusal (a
    failed run's report); `Submission.created` records the creation time
    used. Ten new or renamed conditions are in the rename table's
    `conditions` block. **Recommendation:** approve.
12. **Choices between the drafts.** `copy: {dataset: ...}` against #3030's
    sketch `copy: {from: ...}` (P-COPY-1); `origins` against keeping `origin`
    for the list (P-ORIGIN-1); and P-ODM-1, which keeps the benchmarks'
    four ODM idioms instead of converting 38 benchmark files by hand.
    **Recommendation:** `dataset`, `origins`, and keep the idioms.

## Decided changes, measured

| Decision | Change | Benchmark files | In all |
| --- | --- | --- | --- |
| D4 | `schema_version` becomes `yamaa` plus `type` | 484 (440 spec, 27 project, 16 environment, 1 layer) | 2,126 documents in 636 files |
| D8 | Environment files become `type: function` files; locks move to `environments.<host>.lock` | 16 environment files (8 pairs), 8 project files | 69 environment documents |
| D16 | `standard` becomes `standards`; several origins | 17 sections; 145 origins in 28 files | 31 sections; 202 origins |
| D18 | `literal`, `compute` and the string form of `source` merge into the grammar; `{literal: null}` becomes `null` | 891 `literal` in 167 files; 156 `{literal: null}` in 95; 84 `compute` in 55; 10 `{source: X}` in 9 | 1,525; 387; 181; 25 |
| D19 | `copy` entry | 10 same-named copies of an input linked through `schema` | |
| D23 | Required intermediate `key`; input key declarations | 26 intermediate keys and 86 input declarations in 92 specs, 24 of them negative | |
| D25 | `model` on the spec | 415 entry specs have a model source; 10 have none | |
| D26 | ODM XML as an input | no field; the extension selects the profile | |
| D28 | `rule` replaces `requirement`; five new issue columns | 189 `error.yaml` files, 4 report goldens | 4,703 REQ citations in 501 files |

In every benchmark pair, the Python and R definitions of a function differ
only in `function` and in the lock, so the D8 merge is mechanical. Of 1,525
`literal` payloads, 991 are text, 335 numbers, 190 null and 9 Boolean. The
[method page](interface-1.0/method.md#converter-footprint) has the full
converter footprint by area.

## 2.1 Files and versions

**Decided.** Every yamaa document starts with `yamaa: "1.N"` and `type:`
(D4); a 1.M engine reads 1.M and older 1.x files and admits a feature only
in files that declare the version introducing it; every layer of a chain
declares the same `type`; paths stay as in 0.5.0 (D5).

**P-TYPE-1. The codelist file type: `codelists`.** The type names what the
file holds, several codelists of one source, and matches the project field
that lists it; the other types hold one thing each, hence their singular.
Codelist sources are written inline 26 times (9 in benchmarks) and as file
paths 45 times (none in benchmarks). Both forms stay: D3 keeps the project
file's shape, and a file is the only way to share one terminology release
across studies. Alternatives: `codelist`, `terminology`, `ct`.

**P-VER-1. A chain needs the newest version any layer declares.**

| Option | Rule | Why not |
| --- | --- | --- |
| 1 | Every layer declares the same version (today's LAYER-4) | A parent written for 1.0 must change whenever a child uses a newer feature |
| 2 | The entry's version, parents at or below it | Refuses a valid chain whose parent is newer than its child |
| **3** | **The newest declared version; each layer admitted at its own version** | |
| 4 | The oldest declared version | Under-reports what the chain uses |

Under option 3, a layer declaring 1.0 that writes a 1.2 field fails in that
layer, as `unknown_field` with context naming the version that introduced
the field (or `value_not_permitted` for a value); one fault keeps one
condition. A file that declares a version the engine does not read fails as
`unsupported_language_version`, which replaces `schema_version_mismatch`
(SCHEMA-2). The resolved specification records the chain's version, and a
`clear:` path is checked against it. A call that receives a file of the
wrong `type`, or a chain mixing types, fails as `unexpected_file_type`.
Every layer declares 1.0 today, so nothing converts.

**P-DICT-1. Mapping dictionary files stay header-less.** The file a
`mapping.dict` path names (EXPR-11) holds the value of one field, like a CSV
input it is data, and it is used 8 times against 124 inline dictionaries. D4's
"every file" reads "every yamaa document".

## 2.2 Project file

**Decided.** `functions` lists function files and `environments` holds
each host's lock (D8); `standards` is a list whose first entry is the default
(D16); everything else keeps its 0.5.0 shape (D3).

**P-PRJ-1. `transport: xpt | dataset_json`.** #2855 decision 18 named the
field `transport`; the field selects a transport format, not datasets; and
`json` alone does not name Dataset-JSON, whose 1.1 also defines an NDJSON
representation. 20 uses, 15 in benchmarks; JSON-1 and XPT-1 change.

**P-PRJ-2. Define-only options stay flat.** PROJECT-3 already says which
fields only Define-XML reads, and a group could not hold everything
Define-only: `documents` and `standards` are also read when a spec is
validated.

**P-PRJ-3. `alias` stays.** It holds the NCI concept code that Define-XML
2.1 writes as an Alias element with Context nci:ExtCodeID. A rename to
`code` was conditional on reserving `alias` for Define-XML aliases in
general; the [CDISC alignment](interface-1.0/cdisc.md) reserves no such
field (the split-dataset domain description is derived), so the name keeps
one meaning.

**P-STD-1. Selecting a standard.**

| Option | Spelling in the spec | Assessment |
| --- | --- | --- |
| **S1** | **`submission: {standard: SDTMIG-MD}`** | **One token; unambiguous when names are unique within a section** |
| S2 | `submission: {standard: {name: SDTMIG, version: "3.3"}}` | Verbose; reserved as S1's SCHEMA-10 long form for two versions of one guide |
| S3 | An `id` on each `standards` entry | A new required field in every entry |
| S4 | The selection in the section's `specs` list | Contradicts D16 |

Names are unique within a section, so generated OIDs stay `STD.<name>` and
no define.xml golden changes. An omitted selection takes the first entry; a
selection must name an entry of the composing section (`unknown_standard`,
DEFINE-8, which D16 makes reachable) and an implementation guide of the
spec's model. `status` gains `Provisional`, which Define-XML 2.1 and the
bundled schema admit. Controlled-terminology standards stay with the codelist
sources (decided).

## 2.3 Function files

**Decided (D8).** One contract per function with each host's callable under
`implementations`; activation runs the same tests in either host; lock
verification is unchanged; a call fails before data when the running host has
no implementation.

The converter writes each function to `functions/<name>.yaml` beside the
project file, lists it under `functions`, and moves a declared lock to
`environments.<host>.lock`, rewritten relative to the project file. A host
that declared no lock gets no `environments` entry: the lock is optional
(FUNC-3), and the entry holds nothing else.

**P-FN-1. Function-file values are not tagged.** Test arguments, results
and defaults are value fields whose type the definition declares (FUNC-5,
FUNC-9), so `{date: 2024-01-01}` restates the type; 5 test values use the
tag. Optional for the owner; dates as bare text are proposed.

**P-FN-2. Conditions.** `project_environment_missing` becomes
`function_implementation_missing` (a call in a host whose function file
gives no implementation), and `project_environment_invalid` becomes
`function_file_invalid` (an invalid function file, test, lock or duplicate
definition). No negative benchmark pins either.

## 2.4 Expressions, missing values and clearing

**Decided.** One expression grammar in expression fields, where a bare name
is a reference, text is quoted and YAML numbers are literals (D18); value
fields keep bare text (D31); YAML null is the missing value, and clearing is
an explicit `clear:` list (D18).

### Field kinds (D31)

**P-KIND-1.** The [field-kind table](interface-1.0/field-kinds.md) lists
every field with its kind, generated from the rename table so that the two
cannot drift. Of 281 spec fields, 28 are expression fields and 53 name fields
(D31's expression fields); 133 value, 37 identifier and 5 path fields (its
value fields); 25 are structure. Three positions change kind with D18 and
must be stated: the handled wrapper's `value`, key match values and function
arguments. FORMULA-27 needs a row for the derivation field: the constructs of
`compute` plus quoted text, temporal literals and `NULL`, without comparisons
or reducers.

An expression field admits both a YAML scalar and grammar text, so missing,
numbers and Booleans each have two spellings (`null` and `"NULL"`, `0` and
`"0"`). The YAML scalar is canonical; validation admits both, and the lint
finding `scalar_as_text` rewrites the text form.

### P-NULL-1. Where YAML null is admitted

| Null written today | Count | Today | 1.0 |
| --- | --- | --- | --- |
| `intermediates[].no_match: null`, single layer | 22 benchmark, 15 Rust | absent, so missing | delete (restated default) |
| `columns[].derivation: null`, single layer | 17 tests | absent | delete: in 1.0 it would mean "always missing" |
| Structural nulls (`columns`, `input`, `rows`, `intermediates`) | about 40, all tests | absent | refused |
| `parent: null` | 9 tests | no parent | refused |
| Child-layer nulls | 2 benchmark files, 10 tests | clear | `clear:` entries |

Null is admitted only where a value or an expression can be missing. In a
structure, identifier, path, name, enumeration, Boolean or number field it
fails as `invalid_field_type`: absence is spelled by omission and clearing by
`clear:`. The converter never turns a child-layer null into `null`: today
`unmapped: null` in a child clears back to "fails", and in 1.0 `null` would
answer missing.

### P-CLR-1. The path syntax of `clear:`

```yaml
clear:
  - columns.AVAL.verifications
  - columns.DCSREAS.derivation.mapping.dict.PD
  - columns.RACEN.derivation.mapping.dict['NOT REPORTED']
  - rows.collected-occurrence.filter
  - intermediates.DEATHEV.absent
  - input.ADSL.require_match
  - metadata.scope
```

```text
spec_path := "$" | name ( "." segment | "[" quoted "]" | "[" index "]" )*
segment   := ( letter | digit | "_" | "-" )+
quoted    := "'" ( any character but "'" | "''" )* "'"
index     := digit+        (diagnostics only, never in clear)
```

- `clear` is a root field, written after `parent`, holding a list of paths.
  Composition consumes it as it consumes `parent`, and the resolved
  specification holds none (LAYER-14).
- Keyed members are addressed by identity: `input` and `windows` by key,
  `columns` by `name`, `intermediates` and `rows` by `id`, as LAYER-5
  composes them. A list replaces whole (LAYER-6), so no path needs an index.
- A path must name a value the layer inherits; naming nothing fails as
  `invalid_clear`, so a misspelt path never clears silently (I9). A required
  field, an identity field, `yamaa`, `type` and a keyed member itself cannot
  be cleared (LAYER-9). A cleared field with a schema default takes the
  default again (LAYER-7).
- Clears apply to the inherited value before the layer's own fields
  compose, so clearing a mapping and writing it in one layer replaces it
  instead of merging, which no layer can state today.
- A path and one of its prefixes in one list fail, and a finding about a
  clear is reported at `clear[i]`. `$`, the root, appears only in
  diagnostics and lint findings.

| Option | Form | Assessment |
| --- | --- | --- |
| **A** | **Dotted spec paths, the diagnostic notation** | **What users already read in diagnostics** |
| B | JSON Pointer, `/columns/AVAL/verifications` | List segments are indexes; members by identity would be a dialect |
| C | A nested mirror, `clear: {columns: {AVAL: [verifications]}}` | A second tree shape to learn and diff |
| D | A `clear` field on each member | `clear` becomes a field of five classes |

Row ids are free text and 9 benchmark ids contain hyphens; 104 benchmark
mapping keys are not identifiers (`NOT REPORTED`), hence the quoted form.

**P-CLR-2. One path notation.** Diagnostics address row templates and
intermediates by `id`, as they already address columns and inputs:
`rows.week2.filter`, `intermediates.DEATHEV.key`. Lists without identity keep
indexes (`verifications[0]`, `case[1]`). This changes 37 spec paths in 35
negative `error.yaml` files, which #3030 section 1 permits through the rename
table, and about 700 path strings in Rust and host tests.

Removed wrappers also leave diagnostic paths: `derivation.compute.expr`
and `<field>.literal` end at the expression field (9 negative paths). A bare
reference keeps today's canonical `source` binding, so `derivation.source`
paths do not change. The rename table's `paths` block lists every rewrite.

## 2.5 Joins, model, metadata, copy and ODM

**Decided.** Declared join keys, with `base`, `filter` and `rows` unchanged
(D23); `model` on the spec (D25); submission metadata in the `submission`
block (D7) with several origins (D16) and #2986's value-level form (D17); a
copy entry (D19); ODM XML as an input (D26); a spec missing a project binding
fails when run alone (D32); today's column-order rules (D6, D33).

### P-JOIN-1, P-KEY-1 and D23-R1. Join keys

| Option | Spelling | Assessment |
| --- | --- | --- |
| **K1** | **`ADSL: {path: adsl.parquet, key: [STUDYID, USUBJID]}`** | **LOOKUP-5's own word, used by 128 benchmark intermediates; #3030's sketch** |
| K2 | `keys: [...]` | Root `keys` means the output identity; a join key need not be the input's identity |
| K3 | `join: {key: [...]}` | One more level for no gain |
| K4 | No field when `schema` links a producer: use its `keys` | Inference again (P1) |

- `key` on an input is a list of its columns, each paired with the current
  row's same-named value (LOOKUP-5's list form). The mapping form stays on
  intermediates and aggregates, and is reserved for inputs.
- It is required on every input that an implicit join reads, including an
  input that drives some row templates and is joined on the rows others build
  (NAME-4). A qualified aggregate without its own `key` uses it (P-KEY-1),
  and fails when the input declares none; an aggregate's own `key` overrides.
  A read of an input without `key` fails as `no_applicable_keys`, restated.
- A named intermediate requires its own `key` (D23) and never falls back to
  its input's. **D23-R1:** an intermediate that drives a row template declares
  none, as ROW-2 requires; 5 benchmark intermediates are such drivers.
- A `key` on an input no read uses is admitted and reported by the lint
  finding `unused_input_key`, so a shared parent layer can declare it.
- LOOKUP-4's `rename_only_intermediate` compares an intermediate's key with
  its input's declared key, so `negative-lookup-rename-intermediate` keeps
  its condition.

The converter writes today's inferred keys: for implicit joins
`[STUDYID, USUBJID]` 45 times, `[USUBJID]` 3, and one each of `[STUDYID]`,
`[STUDYID, USUBJID, AESEQ]` and `[STUDYID, USUBJID, CMSEQ]`, plus 8 joins on
rows another template's driver built; for keyless qualified aggregates
`[STUDYID, USUBJID]` 38 times, `[STUDYID, USUBJID, EXTRT]` 8,
`[USUBJID]` 7 and `[STUDYID, USUBJID, AVISIT]` 2. Every field list came from
one of 140 CSV headers, and no input infers two keys in one spec. Only
`negative-lookup-unknown-key` has no inferable key (Q2).

### P-MODEL-1. `model`

`model: SDTM`, `SEND` or `ADaM`, the CDISC spellings, written after `type`.
It is required when the spec declares root or column `submission` (checked on
the spec alone) and when a project section lists it (`sdtm` with SDTM, `send`
with SEND, `adam` with ADaM). It is optional otherwise: 390 benchmark specs
and about 750 test specs are language tests, and a model there would assert
something false. The declared class must belong to the model. With the model
known, SUB-23 moves the core vocabulary (SUB-10), origin pairs (SUB-15), the
ADaM prohibitions (SUB-5, SUB-9) and the ADaM date types (SUB-6, SUB-25) to
specification time. The rules say "model" instead of "family", and
`unknown_standard_family` becomes `standard_model_mismatch`. The converter
writes `model` from the listing section, else from `benchmark.yaml`
`standard`, on the entry layer only: 415 entry specs, none for the 10 labelled
CDISC or TEST. Alternatives: lowercase values (`sdtm`), or `family:`.

### P-COPY-1. `copy`

```yaml
columns:
  - copy:
      dataset: ADSL
      columns: [TRT01A, AGE, SEX]
      submission: {core: Perm, mandatory: false}
```

| Option | Spelling | Assessment |
| --- | --- | --- |
| C1 | `copy: {from: ADSL, columns: [...]}` | #3030's sketch; reads naturally; a new word |
| **C2** | **`copy: {dataset: ADSL, columns: [...]}`** | **`dataset` already names the relation an intermediate or row template reads (290 benchmark uses); no new word** |
| C3 | `copy: [ADSL.AGE, ADSL.SEX]` | Repeats the dataset per column |

- Same-name copies of an input linked through `schema`, as decided; a renamed
  copy changes its label and stays an ordinary column (SUB-20). A copy from a
  non-driver input is an implicit join and needs the input's `key`.
- From the producer: `type` and `label` (decided), and `data_type`,
  `length`, `significant_digits`, `display_format` and `codelist` when it
  declares them, except that a `date` or `datetime` column is submitted as the
  consumer's model requires (SUB-6, SUB-25).
- Origin `Predecessor`, described as `<producer domain>.<column>`.
- `core`, `mandatory` and `role` describe the column in the consumer's
  standard: the optional `submission` holds only those three. Without it, an
  ADaM dataset fails SUB-10 as an ordinary column without `core` does.
- `comment`, `method`, `value_level` and `origins` are not allowed.
- The entry expands to ordinary columns in the resolved specification, so a
  later layer overrides a copied column by name and `clear:` addresses it.
  Copied columns are published only when `output.columns` lists them
  (decided).

The corpus barely exercises it: 10 benchmark columns in 4 specs read a
same-named column of a linked input, all with the producer's type and label.
Another 40% of benchmark columns read a same-named column of an input with no
`schema` link, which D19 does not cover.

### P-ORIGIN-1. Several origins

| Option | Form | Assessment |
| --- | --- | --- |
| O1 | `origin` is one mapping or a list of two or more | SCHEMA-10 forbids a union of a value and a list: a field that takes several values is a list even when it holds one; needs a rule change |
| O2 | `origin`, always a list | Keeps the name; a singular name for a list field |
| **O3** | **`origins`, always a list** | **Follows SCHEMA-10 and D16's own `standard` to `standards`** |

Every origin converts mechanically to a one-entry list (202 in all, 145 in
28 benchmark files); no negative path names an origin. The rules, in
either form: at least one origin (SUB-14); two with the same type and source,
or `Predecessor` or `Not Available` beside another, fail as
`conflicting_origins`; SUB-15 to SUB-17 apply to each; `method` is required
when any origin is `Derived` (SUB-18); SUB-19 checks each declared type
against the constructions; DEFINE-19 writes one def:Origin per origin, in
order. Define-XML 2.1 section 5.3.12.3 admits several origins only when no
value list can separate the rows, which bounds their use.

### P-ODMIN-1. The ODM input profile

An input whose path ends in `.xml` selects the ODM profile, for ODM 1.3 and
2.0. Today's reader never reads `FileType`, keeps `TransactionType` only on
item groups, and so reads a removed item as an ordinary row.

| Content | 1.0 behavior |
| --- | --- |
| `FileType="Snapshot"`, or absent | read |
| `FileType="Transactional"`, or `TransactionType="Remove"` at any level | refused before any record is read, as `source_odm_transactional` |
| Other transaction types | read as the present value |
| `AuditRecord`, `Signature`, `Annotation`, `ReferenceData`, `AdminData` | skipped and documented |
| No `ClinicalData` element (a define.xml, a metadata-only export) | refused, as `source_odm_no_clinical_data` |
| Archives | not an input in 1.0; `read_odm()` keeps reading them |

`read_odm()` applies the same refusals, so the function and the input give
one table. `transactions: replay` and `archive_member` are reserved. No
benchmark reads ODM XML (15 read item tables from CSV), so the policy rests on
I9: fail instead of choosing.

### Reserved spellings (P-VLM-1, P-POOL-1, P-SUPP-1, P-CDISC-1)

A reserved spelling is documented now, admitted by no 1.0 file and taken by
no other meaning (I5). The full list is the rename table's `reserved` block;
the [CDISC page](interface-1.0/cdisc.md#reserved-spellings) gives each
citation.

| Feature | Reserved spelling |
| --- | --- |
| Value-level comparators | `{comparator: NE \| NOTIN \| LT \| LE \| GT \| GE, values: [...]}` beside the list of values; NE and NOTIN admitted in 1.0 if Q8 is approved |
| OR across value-level clauses | `where_any: [{...}, {...}]` instead of `where` |
| Shared definitions and pools (D24) | project `methods`, `comments` and `columns`; `method: {id: X}`, `comment: {id: X}`, a column entry `use: NAME` |
| SUPPQUAL (D29) | column `submission.non_standard: true` and `submission.evaluator`; dataset `submission.supplemental` |
| Split datasets (D29) | dataset `submission.split: {by: LBCAT, datasets: {...}}` |
| Non-standard datasets | dataset `submission.non_standard: true` |
| Variable without data | column `submission.has_no_data: true` |
| Page ranges and formal expressions | `pages: {type, first, last}`; method `expressions: [...]` |
| ODM | `transactions: replay`, `archive_member`, `odm: {item, scope}` |

**SCHEMA-10 correction.** The CDISC draft reserved `where` as a list,
`pages` as a list and `method.expression` as a list beside today's single
forms, and proposed `origin` as one value or a list. Each is a union of a
value and a list, which SCHEMA-10 makes a bundle error. This RFC reserves a
separate list field instead (`where_any`, `expressions`), writes several page
references as several `documents` entries, and chooses `origins`. The
comparator form `{comparator, values}` is SCHEMA-10's sanctioned shorthand:
the list is the shorthand of a class with one required field, and the long
form restating the list is refused (SCHEMA-15).

**P-VLM-1 in brief.** A missing value matches no listed value (SUB-12), so
EQ and IN never select it and NE and NOTIN always do, which is what a DTYPE
slice needs: `{PARAMCD: [SYSBP], DTYPE: {comparator: NE, values: [AVERAGE]}}`
selects the observed rows. NE and NOTIN reuse the `str` where columns and the
membership logic and render one-to-one as `RangeCheck/@Comparator`.

**P-POOL-1.** Not in 1.0: the corpus holds 12 methods and 2 comments, all
distinct, and 14 of 64 columns in multi-spec benchmark studies repeat a name,
type and label, none with its submission metadata. `parent` and `copy`
already share declarations.

### CDISC vocabularies, OIDs and the alignment table

The [CDISC alignment](interface-1.0/cdisc.md) maps every project, dataset,
column, origin, method, value-level and codelist field to its Define-XML 2.1,
Dataset-JSON 1.1 or implementation-guide counterpart, with citations, and
marks each as aligned, derived, gap, differs or reserved.

**P-VOC-1.** The closed CDISC vocabularies (models, dataset classes and
subclasses, standard names and statuses, publishing sets, origin types and
sources, roles, core designations, data types, comparators) move into one
data file, proposed as `yaml/vocabulary/cdisc.yaml`, with each value's `since`
version. Schemas, Rust enums, JSON Schema, reference pages and the per-model
tables of SUB-3, SUB-10 and SUB-15 are generated from it or checked against
it, and a repository check compares the XSD-backed lists with the bundled
`rust/tests/schemas/define/2.1/define-enumerations.xsd`. Today a new value
needs four or five hand edits. Roles close to the nine SDTM v2.0 section 2.1
roles for SDTM and SEND (SUB-9 changes from open); all six roles the corpus
uses are among them.

**P-OID-1.** OIDs stay generated from declared ids (decided). DEFINE-9's
table gives `STD.<standard.id>`, but standards have no `id`: the code writes
`STD.<name>` for an implementation guide and `STD.CT_<publishing set>` for a
terminology source, and mints `CT_<set>_<n>` from a position count when two
terminology sources share a publishing set, which DEFINE-9's statement
forbids. **Recommendation:** a separate fix PR corrects the table (and
DEFINE-10's "standard id"), and makes the colliding case fail as
`duplicate_define_identifier` instead of minting a counter. No golden changes:
they hold `STD.SDTMIG`, `STD.ADaMIG` and `STD.CT_SDTM`.

## 2.6 Spelling review

The [spelling page](interface-1.0/spelling.md) gives each measurement and
option. In brief:

- **Operands (P-OP-1 to P-OP-3).** The convention already holds: 17
  single-input verbs call their input `source`, and three several-input verbs
  call them `sources`. The outliers are `study_day.date` and
  `baseline_flag.value`, which become `source`, and
  `baseline_flag.reference_date`, which becomes `reference` as on
  `study_day`. Kept: `variable` in bindings and order terms (renaming gives
  `source: {source: X}`), `odm.item`, `diff.start` and `end`,
  `between.value` (#2934's fresh forms), and the distinct roles of `dtc`,
  `impute` and `shift`. Cost: 41 occurrences in benchmarks, 93 in all.
- **Handlers (P-HND-1 to P-HND-5).** One name per meaning: a lookup's absence
  answer is `absent` everywhere (the intermediate's `no_match` is renamed),
  while `str_extract.no_match` keeps meaning a pattern that did not match. The
  family reads `absent`, `missing`, `no_match`, `unmapped`, `invalid` and
  `unconvertible`. `require_match: true` stays beside the answer: a single
  policy field would need a reserved word in a value field or the `{value: L}`
  wrapper D18 just removed. Restated defaults become a lint finding with a fix
  (refusing them breaks files for no gain); the converter deletes
  `{literal: null}` restatements in single-layer specs only, since in a child
  layer the line overrides an inherited answer. `- not_missing` replaces
  `- not_missing: {}` (70 uses), and `rows[].derivations` becomes optional
  (42 empty maps).
- **Bindings (P-BIND-1, P-BIND-2).** The aggregate `derive` list becomes a
  `derivations` map, `{NAME: {type: T, derivation: E}}`, the intermediate
  shape; a map composes by key (LAYER-6) where a list is replaced whole. The
  untyped `{value: E}` intermediate wrapper goes: it cannot carry
  `unconvertible` (LOOKUP-15), so it only restates `E`.
- **Temporal (P-IMP-1).** TIME-10 requires `month` exactly when the minimum
  precision is year and `day` exactly when it is year or month, so the
  minimum always follows from the rules supplied. Dropping it declares each
  fact once (I2): 29 deletions, and a spec with neither `month` nor `day`
  becomes valid with minimum day, an admission. It retires two negatives and
  four conditions (Q3).
- **ODM reads (P-ODM-1).** Four idioms read the same item `Value` at a scope.
  They are general constructs and stay valid, and converting them is not
  mechanical: equivalence holds only where the multiple-match failure does
  not arise. The docs teach `odm`; `odm.scope` is reserved for reading a
  subject-level item from an event- or form-level row. This departs from the
  spelling draft, which proposed converting 38 benchmark files by hand (Q12).
- **Overloaded words (P-WORD-1).** `domain` (1,785 uses), `type` (tied to its
  class, as Define-XML's Type is), `environments`, `intermediates` (an
  intermediate can drive rows, which is not a lookup), `dataset` and the
  codelist source's `standard` all stay; `datasets` is P-PRJ-1.
- **Ordering (P-ORD-1 to P-ORD-3).** `baseline_flag` admits `window.order_by`,
  defaulting to `[date]`, and `tie_break: [ATM, LBSEQ]` becomes
  `window.order_by: [ADT, ATM, LBSEQ]`; eligible rows have a non-missing date,
  so prepending it reproduces today's order, and `window_order_by_forbidden`
  retires. `diff.bounds` and `cut.right` become `closed` (`left`, `right`,
  `both`, `neither`, the interval vocabulary of pandas and Polars):
  `exclusive` is `right`, `inclusive` `both`, `between` `neither`;
  `cut.right: true` is `closed: right` and the default is `left`.
  `baseline_flag.inclusive` stays. No lookup filter compares a donor field
  with a current-row value by equality, so P-ORD-3 states the rule as a lint
  finding (`equality_filter`); constant equalities stay in `filter`.
- **Long forms (P-LONG-1).** Under 2.11 a refused form can be admitted in
  1.x but an admitted one can never be refused again, so the refusal stays and
  `lint --fix` rewrites. No positive benchmark writes a long form.
- **Unused surface (P-UNU-1).** `encoding` stays SAS-only (SAS-2), as CSV
  stays UTF-8 (D15); `stylesheet` is the only way to link an XSL stylesheet
  (DEFINE-23); codelist files stay (P-TYPE-1); file-backed function
  definitions go with D8; 25 fields with no parsed use and 78 unused
  enumerated values stay, since an unused optional field costs nothing and a
  removed one cannot return in 1.x.

## 2.7 Diagnostics, run report and provenance

**Decided (D28).** Twelve frozen issue columns, `rule` replacing
`requirement`; the 14 run-report columns of #2998; every condition cited by a
card; one phase vocabulary; closed CDISC vocabularies; OIDs from declared ids;
byte-identical artifacts; provenance without digests.

**P-RULE-1. `requirement` becomes `rule`.** The codemod #3048 mentions is
`tools/rules/migrate.py`: it maps every REQ ID through the cards' `Replaces`
lines but rewrites only markdown, and its docstring says other citations keep
resolving through `Replaces`. 4,703 REQ citations sit in 501 authored files
(1,557 in `.rs`, 1,349 in `.md`, 903 in `.json`). The plan, after the owner
answers Q1:

1. Fix the 36 registry causes whose cited card would lack the condition on
   its Fails line, so the rewrite lands with `check_diagnostics.py` green
   ([finding F-2](#findings-recorded-for-separate-fixes)).
2. Add a `citations` subcommand to the codemod that rewrites every REQ ID in
   authored text files, refuses an unresolvable ID other than four test
   sentinels, and lists binary files for manual handling.
3. Rename the field in the facade, core, adapters, hosts and tools, and the
   `requirement:` key of the negative contracts.
4. Drop the `Replaces` lines and the legacy ID pattern, and rename the
   generated "requirement index" page.
5. Edit the three CSV report goldens and re-author the Parquet one.

Diagnostic identity: 363 distinct (phase, condition, citation) triples today
become 343, citing 215 cards.

**P-ISSUE-1. `file`, `line` and `column`.** Take the first spec path; walk
it through the resolved specification's provenance to the deepest node a
layer wrote (a path into an expanded long form stops at the written node);
among the layers that wrote it, take the one whose value took effect. A path
that starts in the project file continues into the listed spec. `file` is
that layer's absolute path with forward slashes, `line` and `column` are
one-based, and a position never points into a data file. The YAML decoder
already keeps every node's position. Pointing at the entry file would hide
every error made in a parent layer; pointing at the first declaring ancestor
could name a value a child overrode.

**P-MSG-1. Messages.** One template per condition in `yaml/conditions.yaml`,
`{key}` placeholders naming context keys, rendered in Rust; every cause must
supply each placeholder, which the Rust tests that reach every cause check.
202 conditions have 374 causes, so per-cause texts would put 374 strings in
Rust, away from the language-neutral registry. Messages are not frozen
(2.11).

**P-PHASE-1. One vocabulary.** `phase` keeps the 12 registered values, so no
issue or expected file changes, and a new COLUMN card defines them. COLUMN-10's
handler table renames its "Stage" column "Phase" and writes `validation` and
`mapping` for its four non-phase names; "stage" stays only for COLUMN-8's
numbered lifecycle; "row phase" and "column phase" become "row construction"
and "column derivation"; `engine_rejected`'s context key `stage` becomes
`step`. Registering the four handler names as phases would change `phase` in
issues and expected files.

**P-PROV-1. Provenance (Q4).** The run report already records each input's
path, profile and byte length (`DETAILS`), its record count (`COUNT`) and a
linked input's producer. It records no version or platform, and cannot
without breaking CHECK-24, I8 and the 2.11 benchmark rule. define.xml and
Dataset-JSON carry the engine version as `SourceSystemVersion` unless a
section overrides it (DEFINE-12), and `info()` and the selftest report give
versions and platform. The staleness guard of #2855 decision 13 (#3048) checks
declared identities only and needs no version; it becomes its own child issue.

## 2.8 R, Python and command-line API

**Decided.** The 0.5.0 API stays (D10); problems are values from `check()`
and `run()`; `status` on every result (D36); readable printing rendered in
Rust (D37); `save()` returns the paths written (D38); error text with the
first message and a count (D39); `lint` (D40); `info()` (D41); native paths
(D42); `yamaa.Table` (D11, D43); base data frames (D44); `p.names` (D45);
clean interrupts (D46); a standalone binary with exit statuses 0, 1 and 2
(D12). The [API page](interface-1.0/api.md) has the mock-ups.

**P-STATUS-1. `succeeded`, `failed`, `skipped`.** `skipped` is only a
project-run member whose run-scope producer failed or was skipped
(PROJECT-11). There is no `cancelled`: an interrupt raises the host's own
interrupt and returns no result (D46). No `partial` or `warned`: a warning
never changes the status. `succeeded` and `failed` are already the run
report's `OUTCOME` words. Rust computes the status from the issues and the
build.

**P-PRINT-1. Printed text.**

```text
yamaa run of ADAE: failed, no output, 3 issues
first issue: adam/adae.yaml:57:9: column ASTDY reads TRTSDT, which is declared after it [forward_reference, COLUMN-5]
```

One summary line, `yamaa <kind> of <subject>: <status>, <size>, <n>
issue(s)`, then `first issue:` with location, message and
`[condition, rule]`; a project run lists its members between them. A check
has no size segment, a selftest counts the cases that passed, and a lazy
spec or project prints its path. ASCII and LF only, no colour, nothing
depends on the terminal width. Every form's layout freezes at 1.0.0; message
text does not.

**P-LINT-2. `lint()` columns.** `file`, `line`, `column`, `finding`,
`spec_path` and `message`. `spec_path` costs nothing, since the checker
computes it, and joins a finding with `issues` and `resolve()`. No `fixable`
(the type mapping has no Boolean column, and fixability is a property of the
finding name) and no `severity` (style findings have none). The 1.0 findings
are today's ten less `literal_form`, plus `redundant_long_form`,
`restated_default`, `scalar_as_text`, `unused_input_key` and
`equality_filter`. **P-LINT-1:** the suppression comment becomes
`# yamaa-lint: allow <finding> -- <reason>`, since the verb is `lint`.

**P-INFO-1. `info()` fields.** `version` (the engine's, identical in every
host: under D53 the R package version would differ during candidates),
`language_versions`, `features` (the four Cargo features), `host`,
`host_version` and `platform` (`<os>-<arch>`). Plain data: a Python dict, an R
named list, JSON with `--format json`. No build time or commit, since the
same source gives the same bytes.

**P-CLI-1. The command line (Q10).** Eight commands: `check`, `run`,
`submission`, `resolve`, `read-odm`, `lint`, `schema`, `selftest`, plus
`--version`, `--help` and `--format text|json`. Exit 0 for `succeeded`;
1 for findings, a `failed` or `skipped` status, or a refusal that carries
issues; 2 when the command cannot run as asked (usage, an unknown section or
file type, a file whose type the command does not take, a failed write to
standard output). A missing input or a failed publication is an issue the
hosts report too, so it exits 1. Ctrl+C ends the process by the signal.

**P-API-1. New members (Q11).** `yamaa.Error.paths` and the R condition's
`paths`: a failed run's `save()` writes its report, then refuses, and today
the caller cannot learn what was written. `Submission.created`: section 2.8
says the result records the creation time used, and no member does. A
project run's member lookup refuses with `unknown_specification_name`
(PROJECT-6), and saving a run that built nothing with `nothing_to_save`, so
every refusal carries an issue for D39's text.

## 2.9, 2.10 and 2.12

These sections have no RFC items. Distribution and Windows (2.9) are the
Phase 4 child issues #3056 to #3063; the engine architecture (2.10) is
rewrites 2 and 3 (#2844, #2845); the plaintext workflow (2.12) follows from
`schema`, `lint`, `check --format json` and `resolve` above.

## 2.11 Stability contract

**P-STAB-1.** The [contract text](interface-1.0/stability.md) goes into
`docs/release.md` before 1.0.0 (D20). It freezes the four file types, the
grammars, verbs and vocabularies; condition names, phases and each existing
failure's condition and rule; the issue and report columns; host names,
arguments, members, `status` values and type mappings; command-line commands,
flags, exit statuses and JSON keys; the facade's public Rust items; saved
bytes per profile; and the summary-line layout. Messages are not frozen.

`make check-repository` compares seven manifests with the newest
`yamaa-v<M>.<m>.<p>` tag of the workspace's major version: the four decided
ones (the `yaml/` schemas, `yaml/conditions.yaml`, the bindings file extended
to the command line and tables, the facade's public API inventory) and three
recommended ones (the registry's (phase, condition, rule) triples, rule card
IDs, and the benchmarks present at the tag). It enforces nothing until
`yamaa-v1.0.0` exists; at a new major's `M.0.0` it lists the breaks against
the previous major for the release notes. It needs `fetch-depth: 0` in CI's
repository job. Its only exemption is a committed list of reviewed defect
fixes, each naming the rule it restores and the owner's decision; message
text never reaches a benchmark. Platform floors such as Python 3.12 and R 4.4
move only in minor releases, announced in the release notes.

## Rule contradictions (Phase 0 item 6)

Card text for each is on the [rules page](interface-1.0/rules.md). A title or
statement change needs the maintainer's approval (`rules/README.md`), which
the owner gave for the proposed text (D71); a detail change needs ordinary
review. Phase 1 makes the changes.

| Id | Contradiction | Disposition | Change |
| --- | --- | --- | --- |
| P-RULE-2 | JSON-15 restates INPUT-1's extension list, stale since the SAS inputs | JSON-15 cites INPUT-1 instead and gains `Fails: source_profile_unknown` | statement |
| P-RULE-3 | CHECK-14 cites a nonexistent `unmatched: fail` | Cite `require_match: true` and `unmatched_key` (LOOKUP-24) | detail |
| P-RULE-4 | SPEC-6's undefined "uses a lookup" makes every implicit join non-row-local, against its own detail and the code | Restate SPEC-6 as the implementation decides; rename the context value `named_intermediate` to `self_intermediate`; the style rule follows | statement |
| P-RULE-5 | Six conditions no card's Fails line names; `unknown_standard` never raised; the unregistered `cancelled` row | Add the six to the cards their citations map to; wire `unknown_standard` (P-STD-1); remove the row (F-1) | metadata |
| P-PHASE-1 | Stage against phase | A new COLUMN card defines the 12 phases; editorial changes | statement (new card) |
| P-JOIN-1 | P1 against LOOKUP-1 (D23) | LOOKUP-1, LOOKUP-2, LOOKUP-3, LOOKUP-4, LOOKUP-5 and LOOKUP-25 change; "applicable keys" leaves the rules | statement |
| P-RULE-7 | CT-9 and CT-11 (D32) | A new CT card: a project binding without a project file fails as `project_binding_missing`; CT-1, CT-9, CT-11 and FUNC-1 follow | statement (new card) |
| P-RULE-8 | COLUMN-5 against LAYER-13 (D33) | Scope LAYER-13 to layered specs; one paragraph in the schema introduction | statement |
| P-RULE-9 | PROJECT-12 and PROJECT-13 fail without a condition | `nothing_to_save` on both Fails lines | metadata |

**P-RULE-10. Rules tightening (#3048).** New titles for NAME-5 and EXPR-5,
which read as contradictory, with their D18 statements; clearer titles for
FORMULA-14, FORMULA-24, TIME-5, WINDOW-9, CHECK-5, SUB-10 and INPUT-8; a
headline VALUE card for missing values, with a table of 17 operations and
the rule that fixes each one's treatment; principle P11, "a package is
consistent and traceable"; and FUNC-6's null argument, which D18 resolves,
with LAYER-9's new title.

**#2934's P1 to P5.** `between.value` keeps its name and stays a name field,
so P1 and P3 carry over word for word. Key match values become expression
fields, and the bare, dataset-qualified and lookup-qualified forms read the
same in a name field and inside expression text, so P2's refusal holds for
expression text, as LOOKUP-5 already applies it. P4 gains one case: an
aggregate without `key` reports its key refusals at the read, with context
naming `input.<X>.key`. P5 is unchanged, and P1's driver-qualified reads are
not joins, so no input key applies to them.

## #3048: example metadata and value-level gaps

| Finding | Disposition |
| --- | --- |
| AEDECOD's role is Record Qualifier in `sdtm-ae-coding`; SDTMIG 3.4 section 6.2.1 gives Synonym Qualifier | separate fix PR, spec only |
| VS EPOCH is Exp, Record Qualifier and Assigned; SDTMIG 3.4 section 6.3.13 gives Perm and Timing | separate fix PR: Perm, Timing, Derived from Sponsor with the method |
| VSORRES and VSDTC are Assigned; they are Collected, with roles Result Qualifier and Timing | separate fix PR, with an annotated CRF document and the AE and VS define tests in three hosts |
| ADSL TRT01A is Req; ADaMIG 1.3 section 3.2 gives Cond | separate fix PR, spec only; goldens unchanged |
| DOMAIN-first key order | Q6: kept for 1.0 |
| Rows that differ only by DTYPE cannot be described | the RFC, through P-VLM-1 (Q8) |
| Where values are not checked against the where column's codelist | separate fix PR: `value_level_where_invalid` gains reason `not_in_codelist`, with a negative benchmark; no golden affected |

No expected file changes for the four metadata fixes; the
[CDISC page](interface-1.0/cdisc.md#3048-example-metadata) gives each check.

## Findings recorded for separate fixes

Found while measuring; none is fixed in this pull request.

| # | Finding | Evidence | Proposed fix |
| --- | --- | --- | --- |
| F-1 | The facade builds an issue row with the unregistered condition `cancelled` in the unregistered phase `check`; `check_diagnostics.py` scans only registered names, so it misses it | `rust/crates/yamaa/src/internal.rs` (line 504) | remove the row: an interrupt raises the host's interrupt (D46), and a Rust embedder gets `Error::Cancelled` |
| F-2 | After the REQ-to-card rewrite, 36 registry causes would cite a card whose Fails line lacks their condition | `rust/crates/yamaa-core/src/diagnostic/mod.rs`; [rules page](interface-1.0/rules.md#registry-citations) | add each condition to its cited card (22 causes), re-cite 13 schema-constraint causes to SCHEMA-12 or SCHEMA-13, and re-cite one catch-all to WINDOW-9 |
| F-3 | DEFINE-9 and DEFINE-10 cite a nonexistent `standard.id`; colliding terminology standards get a counter | `rust/crates/yamaa-core/src/submission_composition.rs` | P-OID-1 |
| F-4 | Four facade refusals carry text but no issue | `rust/crates/yamaa/src/api.rs` | `nothing_to_save`, `unknown_specification_name`, `engine_rejected` |
| F-5 | `unknown_standard` is registered and on DEFINE-8's Fails line, but nothing raises it | registry | wire it with P-STD-1 |
| F-6 | Six registered conditions appear on no card's Fails line | `tools.rules.check` notes | P-RULE-5 |
| F-7 | Three definitions of row-local: SPEC-6, the code and the style guide | `rules/specification.md`, `rust/crates/yamaa-core/src/specification/rows.rs`, `docs/articles/style.md` | P-RULE-4 |
| F-8 | #3030's D23 text cites a LOOKUP card that #3021 retired | `rules/lookup.md` | Q5 |
| F-9 | Standard `status` lacks `Provisional` | Define-XML 2.1 section 4.1.1; the bundled XSD | P-STD-1, or a separate additive PR now |
| F-10 | The `$` prefix of a text codelist's `format_name` is not checked | Define-XML 2.1 section 5.3.13 | small validation fix; both corpus uses comply |
| F-11 | A terminology standard's name is free text in the schema and closed only when define.xml is generated | `yaml/schema_project.yaml`; `submission_xml.rs` | P-VOC-1 |

## How the drafts were reconciled

Three working drafts prepared this RFC: a spelling review with the rename
table, a CDISC alignment, and an API and rules review. Where they disagreed,
this RFC chose as follows.

| Point | Drafts | Chosen | Why |
| --- | --- | --- | --- |
| `copy` field word | `{from: ...}` in #3030's sketch; `{dataset: ...}` in the CDISC draft | `dataset`, with `from` as the option (Q12) | `dataset` already names the relation a construct reads; no new word |
| Several origins | a list field, assumed by the spelling draft; one value or a list, by the CDISC draft | `origins`, always a list | SCHEMA-10 forbids a union of a value and a list; D16's `standards` precedent |
| Reserved list forms | `where`, `pages` and `method.expression` as one value or a list | `where_any`, several `documents` entries, `expressions` | SCHEMA-10, as above |
| Standard selection | unnamed in the spelling draft; `submission.standard` in the CDISC draft | `submission: {standard: NAME}`; the codelist source's `standard` stays | Both drafts agree once named; one token, OIDs unchanged |
| `alias` | rename to `code` if the CDISC draft reserved `alias` | keep | The CDISC draft reserves no other `alias` |
| Join and aggregate counts | 59 joins and 55 keyless aggregates (walker); 69 and 34 (token scan) | the walker's | The token scan counted shorthand aggregates and prose as joins and missed row-template joins ([method](interface-1.0/method.md)) |
| `model` | written wherever the converter has a source; required only with submission metadata or a section listing | both: the CDISC draft's requirement, the spelling draft's converter rule | A model asserts nothing false where a benchmark is labelled SDTM or ADaM |
| `no_applicable_keys` | retired for intermediates; or `missing_required_field` | kept for an input without `key`; an intermediate without `key` fails as `missing_required_field` | One fault, one condition; LOOKUP-3 already names `missing_required_field` |
| ODM reads | convert 38 benchmark files to `odm` with `scope` | keep the idioms, reserve `scope` (Q12) | The conversion is not mechanical, and the idioms stay valid |
| Lint findings | the API draft's nine; the spelling draft's restated defaults, long forms and equality rule; the CDISC draft's unused input key | all fourteen | Each proposal needs its finding name in the frozen vocabulary |
| Diagnostic paths | today's indexes in the API draft's examples; by `id` in the spelling draft | by `id` (P-CLR-2) | One notation for `clear:` and diagnostics; the location rule (P-ISSUE-1) works on either |
| EXPR-5's statement | five expression fields in the API draft; the field-kind table also marks key match values and the handled wrapper's `value` | EXPR-5 names all seven | The card and the table must agree |
| Titles naming `bounds`, `family`, `tie_break` | the API draft kept today's words | TIME-5 says `closed`, SUB-10 says `model`, WINDOW-9 drops `tie_break` | They follow P-ORD-2, P-MODEL-1 and P-ORD-1 |
| Host lock entries | #3030's example writes both hosts | only hosts that declared a lock | The lock is optional (FUNC-3) and the entry holds nothing else |

The diagnostic paths of the API draft's mock-ups were checked against the
rename table: the CLI and printed-text examples cite `columns.<name>` and
`sdtm.specs[N]`, which do not change; negative paths such as
`intermediates[0].key` become `intermediates.<id>.key` under P-CLR-2; the four
run-report goldens' `SPEC_PATH` values hold no changed segment; a report's
lookup handler rows end in `absent` under P-HND-1. The rename table's `paths`
block lists each rewrite.

## Where #3030 and the repository guides disagree

#3030 says the guide wins where the two disagree, and asks that each case be
stated on the issue.

- **Expected files.** #3030 section 1 permits only a spec-path change in a
  negative `error.yaml`. D28, and the consequences of D4, D18 and D23 above,
  change more. AGENTS.md treats a golden change as an owner decision with
  independent reproduction, so Q1 and Q2 ask for that decision instead of
  widening #3030's rule silently.
- **The bindings file.** Phase 0 item 3 asks for `rust/interface/bindings.yaml`
  drafted for three hosts. Its generator, documented in
  `rust/interface/README.md`, refuses unknown keys and unimplemented exports,
  so the draft is `rust/interface/bindings-1.0.yaml`, which nothing reads.
- **`schema-inheritance` with `clear:`.** Phase 0 item 4 converts
  `schema-inheritance` "with `clear:`", but it has no child-layer null; its
  sibling `schema-inheritance-clear` does, and both are converted.
- **Pull request reference.** AGENTS.md asks for `Closes #N`; Phase 0 has no
  child issue and does not close #3030, so this pull request says
  `Refs #3030`.

## What this RFC does not change

- No rule card, schema, condition registration, engine or host code, test or
  benchmark. The approved card text (D71) is on the rules page; Phase 1
  writes it.
- No expected file. Q1 and Q2 ask for the decisions that Phase 2's golden
  changes need.
- No `rust/interface/bindings.yaml` change; the generator never reads
  `bindings-1.0.yaml`.
- The converted examples are not runnable: they sit under `docs/`, no
  benchmark harness, test or conformance plan reads them, and they carry no
  inputs.
- No measurement script is committed; the [method page](interface-1.0/method.md)
  describes each measurement.
