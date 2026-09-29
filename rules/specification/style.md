---
id: specification/style
title: Specification style
status: normative
---

# Specification style

## Requirements

### Scope

<a id="req-1279"></a>

**REQ-1279.** This contract fixes one written form of a specification and
calls each departure from it a style finding. A style finding is not a
validation diagnostic: it has no phase and no condition, and a
specification with style findings stays valid and runs exactly as its
canonical form does. Every departure named here leaves the parsed value
unchanged: field order has no execution meaning
([REQ-0252](../reference/schema-language.md#req-0252)), block and flow
collections parse to the same value
([REQ-0278](../reference/schema-language.md#req-0278)), and a shorthand
expands before validation
([REQ-0264](../reference/schema-language.md#req-0264)).

<a id="req-1280"></a>

**REQ-1280.** A style check reads one specification file as its author
wrote it, before [Specification composition](composition.md) resolves any
layer. It reports each finding with the file, the one-based line and column
where the departure starts, the finding name, and the requirement that
names it. Repository validation reports every finding in a benchmark
specification as a failure. Elsewhere the check runs when a project runs
it.

### Field order

<a id="req-1281"></a>

**REQ-1281.** The fields of a mapping are written in the order the schema
declares them for its class. This applies to the root of the
specification and to each input dataset, the output, each named
intermediate, each column, and each row template. The root declares its
fields in reading order: what the specification is (`schema_version`,
`parents`, `domain`, `keys`), what it reads (`input`, `base`, `filter`),
what it writes (`output`), how it derives the rows (`windows`,
`intermediates`, `columns`, `rows`), and what it checks and publishes
(`verifications`, `submission`, `metadata`). A field written after one the
schema declares later is reported as `field_order`.

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

### Blank lines

<a id="req-1282"></a>

**REQ-1282.** One blank line precedes every root field from `output` on.
The fields before `output` -- `schema_version`, `parents`, `domain`,
`keys`, `input`, `base`, and `filter` -- may stand together. A comment
written directly above a field belongs to that field, and the blank line
precedes the comment. A missing blank line is reported as `root_spacing`.

<a id="req-1283"></a>

**REQ-1283.** Within `intermediates`, `columns`, and `rows`, one blank line
separates two adjacent entries when either entry spans more than one line.
Entries of one line each may stand together. A comment written directly
above an entry belongs to that entry, and the blank line precedes the
comment. A missing blank line is reported as `entry_spacing`.

<a id="req-1284"></a>

**REQ-1284.** A specification has no blank line before its first line or
after its last, never two blank lines in a row, and no blank line between a
field and the block that field opens. It ends with one line break. A line
inside a multi-line scalar or flow collection is value text, not layout,
and is not counted. A departure is reported as `blank_lines`.

### Line width

<a id="req-1285"></a>

**REQ-1285.** A line holds at most 79 characters. A longer line is
reported as `line_width` at its eightieth character. A flow list may
continue on following lines, and a quoted or plain scalar may continue on
following lines under the YAML folding rules, which read each line break
inside it as one space.

```yaml
    filter: "DS.DSCAT = 'DISPOSITION EVENT' AND DS.EPOCH = 'FOLLOW-UP'
      AND DS.DSDECOD = 'COMPLETED'"
```

### Canonical spellings

<a id="req-1250"></a>

**REQ-1250.** The canonical spelling of a literal expression is the
single-line flow mapping `{literal: X}`, wherever the literal appears: a
`case` branch `then` or `otherwise` result, a `derivation` value, a
row-template value, or any other position. The block form (the parent key on
its own line with `literal: X` nested beneath) parses identically but is
non-canonical and is reported as `literal_form`.

<a id="req-1255"></a>

**REQ-1255.** The canonical spelling of a plain source expression is the
bare string, wherever the source shorthand applies
([REQ-0319](../operations/expressions.md#req-0319)): a `derivation` value,
a row-template or lookup `derivations` entry, or a `case` branch `then` or
`otherwise`. The single-key mapping form `{source: X}`, flow or block,
parses identically but is non-canonical and is reported as `source_form`.
The mapping form remains the valid spelling where the shorthand does not
apply: nested expression arguments, the `value` of a handled expression,
and filtered sources written `{source: {variable: ..., ...}}`.

### Fixes

<a id="req-1286"></a>

**REQ-1286.** A style fix changes how the lines of a file are laid out and
never the written text of a value. It may move a field, with the comments
written directly above it, into the order of
[REQ-1281](#req-1281); insert or remove blank lines; continue a flow list,
a quoted scalar, or a plain scalar on following lines; and write a
one-line flow mapping other than `{literal: X}` or `{source: X}` as a block
mapping, keeping each member's written text. A fix is kept only when the
fixed file parses under the YAML 1.2 core schema to the same value, with
the same types, as the file before it, carries the same comment lines, and
does not change when it is fixed again. Otherwise the file stays as written
and the finding stays reported. A `literal_form` or `source_form` finding
changes the spelling of a value, and its author corrects it.

### Suppression

<a id="req-1287"></a>

**REQ-1287.** A comment `# yamaa-style: allow <name>, ... -- <reason>`
written directly above a field or an entry suppresses the named findings
reported within that field or entry. Other comments may stand between the
suppression and the field, and the suppression states a reason. A
suppression that names no finding, names an unknown finding, or gives no
reason suppresses nothing and is itself reported as `invalid_suppression`,
which no suppression covers.

```yaml
columns:
  # yamaa-style: allow line_width -- the code list is quoted verbatim
  - name: ARM
    type: str
```
