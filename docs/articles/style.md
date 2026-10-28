# Style guide

This guide fixes one written form for each yamaa file: a specification, a
project file, a codelists file and a function file. Each departure from it is
a lint finding. A finding names how a file is written, never what it means: it
has no phase and no condition, and a file runs exactly as its canonical form
does. The [rules](rules.md) define what a file means; this guide
fixes how it is written, and `lint` with `fix` rewrites what it can prove.

## Running the check

`lint()` checks the files it names, and every yamaa file beneath a directory
it names:

=== "R"

    ```r
    findings <- yamaa_lint(c("sdtm/dm.yaml", "adam/"))
    remaining <- yamaa_lint("sdtm/dm.yaml", fix = TRUE)
    ```

=== "Python"

    ```python
    findings = yamaa.lint(["sdtm/dm.yaml", "adam/"])
    remaining = yamaa.lint("sdtm/dm.yaml", fix=True)
    ```

=== "Command line"

    ```sh
    yamaa lint sdtm/dm.yaml adam/
    yamaa lint --fix sdtm/dm.yaml
    ```

The check reads each file as its author wrote it, before any inherited layer
is resolved, and returns a table with one row per finding: the file's absolute
path with forward slashes, the one-based line and column where the departure
starts, the finding, its spec path and a message. Only
[`partial_restatement`](#authoring-findings) also reads the layers a
specification inherits and includes.

| file | line | column | finding | spec_path | message |
| --- | --- | --- | --- | --- | --- |
| `/study/adam/adsl.yaml` | 12 | 1 | `root_spacing` | `output` | leave one blank line before `output` |

The spec path is written as the issues table writes it, so a finding joins
the issues of `check()` and the paths `resolve()` shows: a list entry that
writes `name` or `id` is named by it, as in `columns.AGE.label` or
`rows.screening.derivations.VISIT`, any other entry by its index, as in
`verifications[0]`, and `$` names the root. The command line prints each
finding as `file:line:column: finding message`, then a summary line such as
`2 lint findings in 1 of 3 files`, and exits 1 while a finding remains.

A directory expands, in sorted path order, to the `.yaml` files beneath it
whose root declares the 1.0 header (`yamaa` and a `type` naming a file type);
hidden directories are left out. Rows follow the files, then line, column and
finding. A file that cannot be read or parsed, or a directory that cannot be
listed, refuses the call with the issue that says why, and nothing is
written. Field orders follow the 1.0 schemas. A directory skips a file that
declares `schema_version`; a file the call names that declares it was written
for language version 0.5, and refuses the call as
`unsupported_language_version`, naming version `0.5`
([SCHEMA-2](../../rules/reference/schema-language.md#schema-2)).

## The findings

The findings, in the order the lint lists them:

| Finding | Reported where | Fixed by `fix` |
| --- | --- | --- |
| `field_order` | A field written after one its class orders later | yes |
| `root_spacing` | A root field of a section with no blank line before it | yes |
| `entry_spacing` | Two multi-line list entries with no blank line between | yes |
| `blank_lines` | A leading, trailing, repeated or misplaced blank line | yes |
| `line_width` | A line longer than 79 characters | when it can wrap |
| `invalid_suppression` | A suppression comment that suppresses nothing | no |
| `repeated_row_derivation` | A derivation repeated in every row template | no |
| `redundant_intermediate` | A named intermediate equal to the implicit join | no |
| `missing_label` | An output column without a label | no |
| `redundant_long_form` | A long form that only restates its shorthand | yes |
| `restated_default` | A field or handler that writes its default | yes |
| `scalar_as_text` | Grammar text for a missing value, number or Boolean | yes |
| `unused_input_key` | An input's `key` that no read uses | no |
| `equality_filter` | A lookup filter comparing a donor field and a row value | no |
| `partial_restatement` | A layer restating an inherited mapping with entries left out | no |

## Field order

The fields of a mapping are written in the order the schema declares them for
its class. A specification orders its root, each input, the output, each named
intermediate, each column and each row template. A project, codelists or
function file orders its root and every mapping beneath it that a class of its
schema describes: a project's environments, study, codelist sources and
submission sections, with their standards, documents and metadata version, each
codelist with its items, and a function file's `params` and `tests` entries. A
field written after one the schema declares later is reported as
`field_order`.

A specification's root reads in this order:

- what the specification is (`yamaa`, `type`, `model`, `parent`, `include`,
  `clear`, `domain`, `keys`);
- what it reads (`input`, `base`, `filter`);
- what it writes (`output`);
- how it derives its rows (`windows`, `intermediates`, `columns`, `rows`);
- what it checks and publishes (`verifications`, `submission`, `metadata`).

```yaml
yamaa: "1.0"
type: spec
model: ADaM
domain: ADSL
keys: [STUDYID, USUBJID]
input:
  DM: input/dm.csv
  EX: input/ex.csv
base: DM

output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, TRTSDT]

intermediates:
  - id: FIRSTDOSE
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXSTDTC]
    keep: first

columns:
  - name: STUDYID
    type: str
    label: Study Identifier
    derivation: DM.STUDYID

  - name: USUBJID
    type: str
    label: Unique Subject Identifier
    derivation: DM.USUBJID

  - name: TRTSDT
    type: str
    label: Date of First Exposure to Treatment
    derivation: FIRSTDOSE.EXSTDTC
```

A project file's root reads `yamaa`, `type`, `functions`, `environments`,
`codelists`, `study`, then the submission sections `sdtm`, `adam` and `send`.
A codelists file's root reads `yamaa`, `type`, `standard`, `codelists`, and a
function file's `yamaa`, `type`, `name`, `description`, `params`, `returns`,
`may_return_missing`, `comparison_decimals`, `implementations`, `tests`.

## Blank lines

The root fields of a file's header may stand together, and one blank line
precedes each root field after it, which opens a section. The header ends
where the sections begin: before `output` in a specification, before the
first submission section in a project file, before `tests` in a function file
and before `codelists` in a codelists file. A comment written directly above a
field belongs to that field, and the blank line precedes the comment. A
missing blank line is reported as `root_spacing`.

One blank line separates two adjacent list entries when either spans more
than one line: the entries of a specification's `intermediates`, `columns` and
`rows`, of a function file's `tests` and of a codelists file's `codelists`.
Entries of one line each may stand together. A comment written directly above
an entry belongs to that entry, and the blank line precedes the comment. A
missing blank line is reported as `entry_spacing`.

```yaml
yamaa: "1.0"
type: project
functions: [functions/bmi.yaml]
environments:
  python: {lock: uv.lock}
  r: {lock: renv.lock}

sdtm:
  specs: [sdtm/dm.yaml]
  standards:
    - {name: SDTMIG, version: "3.4", status: Final}

adam:
  specs: [adam/adsl.yaml]
```

A file has no blank line before its first line or after its last, never two
blank lines in a row, and no blank line between a field and the block that
field opens. It ends with one line break. A line inside a multi-line scalar or
flow collection is value text, not layout, and is not counted. A departure is
reported as `blank_lines`.

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

A 1.0 file has one spelling for each value, and the check reads the file
through its file type's schema, as validation reads it, to report the
spellings that depart from it.

**One grammar in expression fields.** A `derivation`, a row template's
`derivations` entry, a `case` result, a handled wrapper's `value`, a key match
value and a function argument hold one expression of the closed grammar: a bare
name is a reference, quoted text such as `'DM'` is a literal, and a YAML number
or null is a literal ([EXPR-5](../../rules/expressions.md#expr-5)). Value
fields, such as a handler, a `dict` value or a label, hold constants, in which
bare text is text.

```yaml
columns:
  - name: DOMAIN
    type: str
    label: Domain Abbreviation
    derivation: "'DM'"

  - name: AGE
    type: int
    label: Age
    derivation:
      case:
        - when: "DM.AGE >= 0"
          then: DM.AGE
        - otherwise: null
```

A missing value, a number and a Boolean can each be written as a YAML scalar
or as grammar text, such as `null` and `"NULL"` or `0` and `"0"`. The YAML
scalar is canonical: an expression field whose whole value is such text is
reported as `scalar_as_text`, and `fix` writes the scalar. Text for a Boolean,
which an expression field refuses
([FORMULA-27](../../rules/formulas.md#formula-27)), is reported too, so `fix`
writes the YAML Boolean. Quoted text, a name, a predicate and a value field
keep their text.

**Null is the missing value.** YAML null is the missing value wherever a value
is written: in an expression field, a handler or a value list
([COLUMN-10](../../rules/columns.md#column-10)). An omitted handler that
answers with missing needs no null: a lookup's `absent`, and `missing` on
`first_available`, `mapping`, `cut`, `str_extract`, `str_replace`,
`str_case`, `str_substr`, `str_length` and `str_decimal`, and a flag's
`false_value` and `missing`, each written as `null`, restate their default.
So does any field that writes the default its schema declares, such as an
input's `relative_to: declaring`, a section's `context: Submission` or a
codelist's `data_type: text`. Each is reported as `restated_default` at its
key, and `fix` removes it. A handler whose omission
fails, such as `unmapped`, declares an answer when it is `null`, a flag's
`missing` beside a declared `false_value` is required
([EXPR-8](../../rules/expressions.md#expr-8)), and an `absent` beside
`require_match: true` conflicts with it
([LOOKUP-24](../../rules/lookup.md#lookup-24)); none is reported. A layer is
not read for restated defaults, since a value it writes may replace one it
inherits. A layer is a file that declares `parent` or `include`, or a fragment
that a checked specification includes, itself or through the parent chain it
inherits, since a fragment composes on whatever includes it. Check a fragment
together with the specifications that use it, so that `fix` keeps the values
it writes.

**`clear:` removes an inherited value.** A layer that removes a value it
inherits lists its spec path in `clear`; null never clears
([LAYER-9](../../rules/inheritance.md#layer-9)).

```yaml
yamaa: "1.0"
type: spec
parent: adsl-base.yaml
clear:
  - columns.TRTSDT.verifications
domain: ADSL
```

**A shorthand is written as itself.** A field that takes several values is a
list even for one, and a long form that holds only what its shorthand fills,
such as `{variable: X}` for the order term `X`, `aggregate: {expr: E}`,
`D: {path: d.csv}` or `- not_missing: {}`, fails validation as
`redundant_long_form`
([SCHEMA-15](../../rules/reference/schema-language.md#schema-15)). The check
reports the same spelling as the finding `redundant_long_form` at the long
form, and `fix` writes the shorthand.

```yaml
input:
  DM: {path: input/dm.csv, relative_to: declaring}
  EX:
    path: input/ex.csv
```

Here `relative_to: declaring` is a `restated_default` and the `EX` mapping a
`redundant_long_form`. `fix` writes `DM: input/dm.csv` and `EX: input/ex.csv`:
the removed default leaves a long form, which the next pass writes as its
shorthand.

## Authoring findings

These findings read a specification's value, not only its layout. Each names a
rewrite that keeps the derivation's behavior, and its author makes it.

**`repeated_row_derivation`.** A derivation that is identical in every row
template belongs at column level, where it becomes the default each template
inherits ([SPEC-6](../../rules/specification.md#spec-6)). The check reports it
at the first template's entry when all of these hold:

- two or more row templates derive the same column with the same derivation;
- the column has no column-level derivation;
- the derivation is row-local.

A derivation is row-local unless it holds an aggregate or a window operation
anywhere, a `case` result included, reads a `SELF` intermediate, or reads a
column whose column-level derivation is not row-local, a structured
`source`'s `between` value among its reads. A read through the implicit join
or of an intermediate an input backs is row-local. A derivation reading a
driver dataset also stays when the templates build from different inputs,
since the driver read and column derivation's implicit join bind different
records; only a literal is driver-independent across drivers: quoted text, a
YAML number or null. A column read by any row `filter` stays as well, since a
filter resolves only columns its own template derives. A single row template,
a column derived in only some templates, differing derivations per template,
and a column that already has a column-level default are never reported.

```yaml
rows:
  - id: screening
    dataset: SV
    derivations:
      DOMAIN: "'SV'"
      VISIT: SV.VISIT

  - id: unplanned
    dataset: UV
    derivations:
      DOMAIN: "'SV'"
      VISIT: UV.VISIT
```

`DOMAIN` is reported: the same literal in every template, whose drivers
differ. `VISIT` reads each template's own driver and is not.

**`redundant_intermediate`.** A named intermediate equal to the implicit join
to its input is reported at its `id`, with the advice to read the input
directly ([LOOKUP-1](../../rules/lookup.md#lookup-1)). It is one that narrows
nothing (no `filter`, `between`, `order_by`, `keep`, `derivations`,
`verifications` or `columns`), answers absence as the join does (`absent`
omitted or `null`), requires no match on itself or on its input, and whose
`key` pairs exactly the columns of its input's declared `key`, each with the
same-named value. Its `key` written as that list fails validation as
`rename_only_intermediate` ([LOOKUP-4](../../rules/lookup.md#lookup-4)), so
only the mapping that spells the same pairs is a lint finding. An unread
intermediate is unused, not redundant, and an input without `key` has no
implicit join to read instead.

```yaml
input:
  ADSL: {path: adsl.csv, key: [STUDYID, USUBJID]}
intermediates:
  - id: SUBJECT
    dataset: ADSL
    key: {STUDYID: STUDYID, USUBJID: USUBJID}
```

Read `ADSL.TRTSDT` instead of `SUBJECT.TRTSDT` here. While only the
intermediate reads `ADSL`, its `key` is also reported as `unused_input_key`;
the implicit join then uses it.

**`missing_label`.** Every output column declares `label:`. A column named in
`output.columns` and declared in the same file without a non-empty `label` is
reported at its `columns` entry. A column not declared in the same file, and
any layer, a file with a `parent` or an `include` or a fragment a checked
specification includes, is never reported: the label may come from an
inherited layer. Labels carry the reader-facing description a submission
document is generated from.

**`unused_input_key`.** An input's declared `key` that no read uses is reported
at that `key` ([LOOKUP-5](../../rules/lookup.md#lookup-5)). A read uses it when
it is an implicit join: a qualified read of the input that is not a read of the
current row's driver, a qualified `aggregate` over the input that declares no
`key` of its own, or a `copy` entry from the input. A named intermediate over
the input reads its records by its own `key`, and when every row is built from
the input, each qualified read of it reads the driver. Validation admits such a
key, so a shared parent layer may declare it for the layers that inherit it and
suppress the finding there; a layer, a file that declares `parent` or
`include` or a fragment a checked specification includes, is not read, since
its columns may come from the layers it composes on.

```yaml
input:
  DM: {path: input/dm.csv, key: [STUDYID, USUBJID]}
base: DM
```

Every row comes from `DM` here, so `DM.AGE` reads the driver and the key is
unused.

**`equality_filter`.** A lookup filter conjunct that compares a donor field
with a current-row value by equality is reported at the filter: such a
comparison is a key pair, which `key` states. The filters of a named
intermediate, a structured `source` and a qualified `aggregate` are read, each
over the input whose records it narrows. A conjunct that compares a donor field
with a constant stays in `filter`, as do other comparisons, disjunctions and
negations.

```yaml
intermediates:
  - id: RANGE
    dataset: RANGES
    key: [LBTESTCD]
    filter: "RANGES.SEX = LB.SEX AND RANGES.LOW IS NOT NULL"
```

`RANGES.SEX = LB.SEX` is reported: write
`key: {LBTESTCD: LBTESTCD, SEX: LB.SEX}` and keep `RANGES.LOW IS NOT NULL` in
the filter.

**`partial_restatement`.** A layer that writes a member it inherits merges
with it, a mapping key by key, so an entry the layer leaves out of an
inherited mapping comes back from its parent or from a fragment it includes
([LAYER-6](../../rules/inheritance.md#layer-6),
[LAYER-8](../../rules/inheritance.md#layer-8)). A layer that restates such a
mapping, writing it empty or writing again an entry exactly as it inherits
it, and leaves out entries that then come back is reported at the mapping,
and the message names the entries that come back. The mappings read are an
input's `types`, a named intermediate's `key` written as a mapping and its
`derivations`, a row template's `derivations`, the settings of a named window
and the fields of `output` and `submission`
([LAYER-5](../../rules/inheritance.md#layer-5)); a required field of
`output` or `submission` that comes back is not named, since a layer writes
it to replace it. A layer that writes only what it adds or changes is patching and is
never reported, and neither is a mapping the layer clears: clearing a mapping
and writing it in the same layer replaces it
([LAYER-9](../../rules/inheritance.md#layer-9)).

```yaml
yamaa: "1.0"
type: spec
parent: scores-base.yaml

rows:
  - id: scores
    derivations:
      USUBJID: SC.USUBJID
      VISIT: SC.VISIT
      AVAL: SC.SCORE
```

When `scores-base.yaml` also derives `NOTE` in the `scores` template, `NOTE`
comes back and `rows.scores.derivations` is reported. List the mapping in
`clear`, as `clear: [rows.scores.derivations]`, to replace it, or write only
the derivations this layer changes to keep the parent's. A named window is a
keyed member, which `clear` never names, so each setting the layer leaves
out is cleared by its own path, such as `windows.BYSUBJ.filter`, and so is
each field a layer leaves out of `output` or `submission`, such as
`output.report`. Either
meaning may be intended, so `fix` makes neither rewrite. The check follows
the file's `parent` chain and the fragments its layers include through the
same resolver and composes them as the engine does; a file whose layers
cannot be read or composed is not read for this finding, and validation
reports why.

## Fixes

With `fix`, the check first applies the fixes it can prove, then reports what
remains. A layout fix changes how the lines of a file are laid out, never the
written text of a value. It may:

- move a field, with the comments written directly above it, into field order;
- insert or remove blank lines;
- continue a flow list, a quoted scalar or a plain scalar on following lines;
- write a one-line flow mapping as a block mapping, keeping each member's
  written text.

A layout fix is kept only when all of these hold for the fixed file:

- it parses under the YAML 1.2 core schema to the same value, with the same
  types, as the file before it;
- it carries the same comment lines;
- it does not change when it is fixed again.

A canonical rewrite, for `restated_default`, `scalar_as_text` or
`redundant_long_form`, changes the written value and keeps its meaning: it
removes the restated entry, writes the YAML scalar the text stands for, or
writes the shorthand in place of the long form. It is kept only when the fixed
file reads as exactly the values before it with that one rewrite applied, with
the same comment lines, and does not change when it is fixed again. A rewrite
that would lose a comment, or that has no single line to write the shorthand
on, is not made. A fixed 1.0 file must also give no validation finding that
the file as written did not, so a removed default whose long form cannot be
rewritten leaves the file as written.

Otherwise the file stays as written and the finding stays reported. A fixed
file is replaced atomically, keeping its permissions; the named files are the
only files the check writes. The authoring findings name a rewrite their
author makes, so `fix` offers none.

## Suppression

A comment `# yamaa-lint: allow <finding>, ... -- <reason>` written directly
above a field or an entry suppresses the named findings reported within that
field or entry. Other comments may stand between the suppression and the
field, and the suppression states a reason. A suppression that names no
finding, names an unknown finding or gives no reason suppresses nothing and is
itself reported as `invalid_suppression`, which no suppression covers.

```yaml
input:
  # yamaa-lint: allow unused_input_key -- the inheriting layers join LB
  LB: {path: input/lb.csv, key: [STUDYID, USUBJID, LBSEQ]}
```
