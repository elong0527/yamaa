# Specification style guide

This guide fixes one written form of a yamaa specification, so every
specification in a project reads the same way. Each departure from it is a
lint finding. A lint finding is advice, not a validation diagnostic: it has
no phase and no condition, and a specification with lint findings stays valid
and runs exactly as its canonical form does. The layout departures leave the
parsed value unchanged: field order has no execution meaning, and block and
flow collections parse to the same value. The authoring lints name a rewrite
that keeps the derivation's behavior. The [rules](rules.md) define what a
specification means; this guide only fixes how it is written.

## Running the check

`lint()` checks specification files against this guide in both packages:

=== "R"

    ```r
    findings <- yamaa_lint(c("study/adsl.yaml", "study/adae.yaml"))
    findings <- yamaa_lint("study/")
    remaining <- yamaa_lint("study/adsl.yaml", fix = TRUE)
    ```

=== "Python"

    ```python
    findings = yamaa.lint(["study/adsl.yaml", "study/adae.yaml"])
    findings = yamaa.lint("study/")
    remaining = yamaa.lint("study/adsl.yaml", fix=True)
    ```

The check reads each specification file as its author wrote it, before any
inherited layer is resolved. It returns a table with one row per finding: the
file's absolute path with forward slashes, the one-based line and column where
the departure starts, the finding name, the spec path of the departure (`$`
for the root) and a message.

| file | line | column | finding | spec_path | message |
| --- | --- | --- | --- | --- | --- |
| `/study/adsl.yaml` | 17 | 1 | `blank_lines` | `$` | remove the second blank line |

A directory expands to the yamaa files beneath it, of every file type, in
sorted path order; rows follow the files, then line, column and finding. A file that
cannot be read or parsed, or a directory that cannot be listed, refuses the
call with the issue that says why, and nothing is written. Repository
validation reports every finding in a benchmark specification as a failure;
elsewhere the check runs when a project runs it.

## Field order

The fields of a mapping are written in the order the schema declares them for
its class. This applies to the root of the specification and to each input
dataset, the output, each named intermediate, each column, each row template
and each separately authored function definition. The root declares its fields
in reading order:

- what the specification is (`schema_version`, `parent`, `domain`, `keys`);
- what it reads (`input`, `base`, `filter`);
- what it writes (`output`);
- how it derives the rows (`windows`, `intermediates`, `columns`, `rows`);
- what it checks and publishes (`verifications`, `submission`, `metadata`).

A field written after one the schema declares later is reported as
`field_order`.

```yaml
schema_version: "1.0"
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv
  EX: input/ex.csv
base: DM

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGEGR1]

intermediates:
  - id: FIRSTDOSE
    dataset: EX
    order_by: [EX.EXSTDTC]
    keep: first

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID
```

A specification that declares `yamaa` is written in the order of the 1.0
schema: its root opens with the header, `yamaa`, `type` and `model`, and writes
`clear` between `parent` and `domain`. A file that declares `schema_version`
keeps the order of the 0.5 schema. A 1.0 project, codelists or function file
writes every mapping its schema's classes describe in field order: its root,
a project's environments, study, codelist sources and submission sections
with their standards and documents, every codelist and item, and a function
file's `params` and `tests` entries.

## Blank lines

One blank line precedes every root field from `output` on. The fields before
`output` (`schema_version`, `parent`, `domain`, `keys`, `input`, `base` and
`filter`) may stand together; in a specification that declares `yamaa` they
are `yamaa`, `type`, `model`, `parent`, `clear`, `domain`, `keys`, `input`,
`base` and `filter`. In a 1.0 project file the fields before the first
submission section (`sdtm`, `adam`, `send`) stand together, in a function file
those before `tests`, and in a codelists file those before `codelists`; one
blank line precedes each later root field. A comment written directly above a
field belongs to that field, and the blank line precedes the comment. A
missing blank line is reported as `root_spacing`.

Within `intermediates`, `columns` and `rows`, and within a function file's
`tests` and a codelists file's `codelists`, one blank line separates two
adjacent entries when either entry spans more than one line. Entries of one
line each may stand together. A comment written directly above an entry belongs
to that entry, and the blank line precedes the comment. A missing blank line is
reported as `entry_spacing`.

A specification has no blank line before its first line or after its last,
never two blank lines in a row, and no blank line between a field and the block
that field opens. It ends with one line break. A line inside a multi-line
scalar or flow collection is value text, not layout, and is not counted. A
departure is reported as `blank_lines`.

## Line width

A line holds at most 79 characters. A longer line is reported as `line_width`
at its eightieth character. A flow list may continue on following lines, and a
quoted or plain scalar may continue on following lines under the YAML folding
rules, which read each line break inside it as one space.

```yaml
    filter: "DS.DSCAT = 'DISPOSITION EVENT' AND DS.EPOCH = 'FOLLOW-UP'
      AND DS.DSDECOD = 'COMPLETED'"
```

## Canonical spellings

A literal expression is written as the single-line flow mapping
`{literal: X}`, wherever it appears: a `case` branch `then` or `otherwise`
result, a `derivation` value, a row-template value or any other position. The
block form, with the parent key on its own line and `literal: X` nested beneath
it, parses identically. The check reports no spelling of a literal, since the
1.0 language retires `{literal: X}`.

A value has one spelling in the language itself, so the check names
none of the shorthand spellings. A plain source is the bare string wherever
the source shorthand applies, a field that takes several values is a list even
for one, and a long form that holds only the field its shorthand fills, such
as `{source: X}` in a `derivation` or `aggregate: {expr: E}`, fails validation
as `redundant_long_form` rather than standing as a lint finding.

## Authoring lints

These findings read the specification's value, not only its layout. Each one
names a rewrite that keeps the derivation's behavior, and its author makes it.

A derivation that is identical in every row template belongs at column level.
The check reports `repeated_row_derivation` at the first template's entry when
all of these hold:

- two or more row templates derive the same column with the same derivation;
- the column has no column-level derivation;
- the derivation is row-local.

It suggests moving the derivation to `columns[].derivation`, where it becomes
the row-phase default each template inherits. A derivation is row-local as
SPEC-6 defines it: unless it holds an aggregate or a window operation anywhere,
a `case` result included, reads a `SELF` intermediate, or reads a column whose
column-level derivation is not row-local, a structured `source`'s `between`
value among its reads. A read through the implicit join or of an intermediate
an input backs is row-local. Other derivations stay in their row template,
where the row phase evaluates them, and are never reported. A file that
declares `schema_version` keeps the 0.5 reading, in which a read of any named
intermediate also stays. A derivation reading a driver dataset also stays when
the row templates build from different datasets, since the driver read and the
column-phase implicit join bind different records; only a literal is
driver-independent and reported across drivers: quoted text, a YAML number or
null in a 1.0 file, and `{literal: X}` in a 0.5 file. A column read by any row
`filter` stays as well, since a filter resolves only columns its own template
derives. A single row template, a column derived in only some templates,
differing derivations per template, and a column that already has a
column-level default are never reported.

```yaml
columns:
  - name: DOMAIN
    type: str
    label: Domain Abbreviation
    derivation: {literal: DM}

rows:
  - id: first
    derivations:
      USUBJID: DM.USUBJID
  - id: second
    derivations:
      USUBJID: DM.USUBJID
```

The `USUBJID` derivation above is reported: it is the same source in every
template, uses no aggregate, window or intermediate, and has no column-level
default.

A simple named intermediate equivalent to the implicit join is reported as
`redundant_intermediate`. An intermediate with no `filter`, `between`,
`order_by`, `keep`, `derivations`, `verifications`, `columns` or
`require_match: true`, whose `no_match` is omitted or `{literal: null}`, and
with an explicit `key` that restates exactly the output `keys`, reads what
`DATASET.COLUMN` already reads. The check reports the intermediate's `id` and
suggests reading the input dataset directly. An omitted `key` fails validation
as `rename_only_intermediate` instead and is never a lint finding.
`require_match: true` requires a match the implicit join cannot state, and an
input declaring `require_match: true` makes its implicit join fail where the
intermediate answers, so neither is reported. Any other `no_match` literal, a
custom key and a `SELF` intermediate all change behavior and are never
reported. An unread intermediate is unused, not redundant, and is never
reported.

A 1.0 intermediate is judged by its 1.0 meaning, against the implicit join on
its input's declared `key` (LOOKUP-1): `absent` omitted or null stands for the
join's missing value, and the intermediate is reported when its `key` pairs
exactly the columns of its input's `key`, each with the same-named value. Its
`key` written as that list fails validation as `rename_only_intermediate`
(LOOKUP-4), so only the mapping that spells the same pairs is a lint finding.
An input without `key` has no implicit join to read instead.

```yaml
keys: [STUDYID, USUBJID]
intermediates:
  - id: ADSL1
    dataset: ADSL
    key: [STUDYID, USUBJID]
    no_match: {literal: null}
```

Read `ADSL.TRTSDTM` instead of `ADSL1.TRTSDTM` here.

Every output column declares `label:`. A column named in `output.columns` and
declared in the same file without a non-empty `label` is reported as
`missing_label` at its `columns[]` entry. A column not declared in the same
file, and any file with a `parent`, is never reported: the label may come from
an inherited layer. Labels do not change derivation behavior; they carry the
reader-facing description a submission document is generated from.

```yaml
columns:
  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID
```

## Fixes

`fix` changes how the lines of a file are laid out, never the written text of
a value. It may:

- move a field, with the comments written directly above it, into field order;
- insert or remove blank lines;
- continue a flow list, a quoted scalar or a plain scalar on following lines;
- write a one-line flow mapping other than `{literal: X}` as a block mapping,
  keeping each member's written text.

A fix is kept only when all of these hold for the fixed file:

- it parses under the YAML 1.2 core schema to the same value, with the same
  types, as the file before it;
- it carries the same comment lines;
- it does not change when it is fixed again.

Otherwise the file stays as written and the finding stays reported. A fixed
file is replaced atomically, keeping its permissions; these named files are
the only files the check writes. A `repeated_row_derivation`,
`redundant_intermediate` or `missing_label` finding rewrites the
specification's value while keeping its derivation behavior, and its author
corrects it; no fix is offered.

## Suppression

A comment `# yamaa-lint: allow <name>, ... -- <reason>` written directly above
a field or an entry suppresses the named findings reported within that field or
entry. Other comments may stand between the suppression and the field, and the
suppression states a reason. A suppression that names no finding, names an
unknown finding or gives no reason suppresses nothing and is itself reported as
`invalid_suppression`, which no suppression covers.

```yaml
columns:
  # yamaa-lint: allow line_width -- the code list is quoted verbatim
  - name: ARM
    type: str
```
