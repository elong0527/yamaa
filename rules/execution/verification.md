---
id: execution/verification
title: Verification
status: normative
---

# Verification

## Requirements

### This contract has no ordered-frame assertion

<a id="req-0367"></a>

**REQ-0367.** State an ordered-series assertion as a derivation followed by a
row-wise assertion in this contract. Use exactly one of three forms: the adjacent
row, a partition or its history up to the current row, or a derived property at
coarser keys or from an upstream specification.

- **The adjacent row.** `row_value` under [Windows](../operations/windows.md) places another row's value on
  the row and `assert` compares the two.
  `negative-pr-after-cr` uses
  the immediately preceding assessment, so it rejects a partial response next
  to a complete one and passes the same fault with an assessment in between.
- **A partition, or its history up to the current row.** A qualified aggregate
  under [Aggregation](../operations/aggregation.md) reduces a source
  relation. Its `between` narrowing keeps only the records at or before a
  current-row value. Thus, a cumulative property of collected values reaches
  the row it must be asserted about.
- **A derived property at coarser keys or upstream.** The specification that
  derives it publishes it, and [Source ingestion](../storage/ingestion.md)'s producing-specification link makes it an
  ordinary source field of the specification that asserts over it. This is the
  same split every other change of keys already uses.

<a id="req-0368"></a>

**REQ-0368.** A frame assertion enters this vocabulary only when an example
needs that shape and the shape cannot be written as a producer and a consumer.

### Referential integrity is proven where the value is produced

<a id="req-0369"></a>

**REQ-0369.** A supplemental qualifier record points to its parent domain
record. The derivation that produces the cross-dataset link asserts the link.
A verification over the finished artifact does not assert the link.
An intermediate without `no_match` rejects a value matching no record, and
an intermediate read carried by a `not_missing` column does the same.
`sdtm-suppmh-linkage` links `IDVARVAL` to its medical-history
record that way.

<a id="req-0370"></a>

**REQ-0370.** The claim that every parent record has a supplemental record is
not an artifact property, so no verification here can state the claim. A
specification that must assert the claim derives at the parent's keys.

### Registration and timing

<a id="req-0371"></a>

**REQ-0371.** `schema_verification.yaml` registers column checks in
`column_verifications` and completed-dataset checks in
`dataset_verifications`.

<a id="req-0372"></a>

**REQ-0372.** Column verifications infer their declared column. A column
verification runs after its declared column's derivation and conversion.
Dataset verifications run after all column work, output-key validation, and
column verifications finish.

<a id="req-0373"></a>

**REQ-0373.** A failed `error` verification fails execution. Implementations
must report its stable specification path, failure count, and representative
offending keys. Reporting limits may be implementation options but must not
change pass or fail. Every evaluated check, a `warning` violation included, is
a row of the run report under [REQ-0393](verification.md#req-0393), which
records every offending key under [REQ-1176](verification.md#req-1176) rather
than applying the reporting limit.

<a id="req-0374"></a>

**REQ-0374.** Every column, dataset, and intermediate verification may
declare a nonempty `id`. A declared ID names the check for readers and is
included in failure reports; column and dataset IDs also appear in the run
report. When it is omitted, the verification's specification path identifies
the check; the report's `ID` stays missing rather than copying the path. An
explicit ID remains useful when a check needs a name that survives reordering
of a specification.

### Column verifications

<a id="req-0375"></a>

**REQ-0375.** `not_missing` passes only when every value is non-missing. Every other value check passes missing values; combine with `not_missing` when absence is invalid.

<a id="req-0376"></a>

**REQ-0376.** `allowed_values` requires every non-missing value to equal one
  listed value under its type's equality, including [Text values](../values/text.md) for strings.

<a id="req-0377"></a>

**REQ-0377.** `range` requires every non-missing numeric value to be greater
  than or equal to `min` and less than or equal to `max` for the supplied
  bounds. At least one bound is required.

<a id="req-0378"></a>

**REQ-0378.** `max_length` requires every non-missing string to contain at
  most `max` [Text values](../values/text.md) scalar values. It is declared only on a `str`
  column: the text a number or a temporal value renders as is a property of
  [Types and conversion](../values/types.md)'s rendering rather than of the value.

<a id="req-0379"></a>

**REQ-0379.** `matches` requires every non-missing string to match its
  regular expression. [Text operations](../operations/text.md) owns the pattern's syntax, the engine that reads
  it, and whether a match must span the whole value.

<a id="req-0380"></a>

**REQ-0380.** [Text values](../values/text.md) scalar count rather than bytes or UTF-16 units is also the
unit [Schema language](../reference/schema-language.md) uses for `min_length`, so a supplementary-plane scalar counts once
in both R and Python. A length is therefore a separate check rather than
an anchored `matches` pattern. `max_length` counts scalar values directly,
not by regular-expression matching.

### Dataset verifications

<a id="req-0381"></a>

**REQ-0381.** `unique` requires the listed columns to exist and their
  combined values to be unique. Missing values participate as values; use
  column `not_missing` when they are prohibited. Write an unnamed check as
  `unique: [STUDYID, USUBJID]`. The mapping form
  `unique: {columns: [STUDYID, USUBJID], id: subject-key}` also admits an
  optional `severity`.

<a id="req-0382"></a>

**REQ-0382.** `all_or_none` requires at least two distinct columns. For every
  output row, either every listed value must be missing or every listed value
  must be non-missing.

<a id="req-0384"></a>

**REQ-0384.** `assert` evaluates `require` for every output row. Every
  result must be `TRUE`; `FALSE` and `UNKNOWN` fail, except on a row its
  `when` exempts under [REQ-0383](verification.md#req-0383). This verification
  covers row-wise rules that lack a more specific verification type.

<a id="req-0383"></a>

**REQ-0383.** An `assert` may declare `when`, which limits the rows its
  `require` must hold for. It then evaluates `when` and `require` for every
  output row. When `when` is `TRUE`, `require` must be `TRUE`; a `FALSE` or
  `UNKNOWN` result from `require` fails. When `when` is `FALSE` or `UNKNOWN`,
  the row passes. The rule does not apply. `when` selects rows here as it
  selects groups for `row_count` under [REQ-1154](verification.md#req-1154).

<a id="req-0385"></a>

**REQ-0385.** `row_count` requires the output count to meet inclusive `min`
  and `max` bounds, and its proportion to meet inclusive `min_fraction` and
  `max_fraction` bounds. At least one bound is required. Fraction bounds must
  be between 0 and 1. The denominator is the number of unfiltered rows in
  the group, or the whole artifact without `group_by`. An empty artifact has
  proportion zero. `filter` and `group_by` determine the numerator and groups
  as the next section defines. `when` selects groups to check but does not
  change the denominator.

Count bounds are mathematical integers in the specification rather than runtime
column values. Compare the complete output count and authored bounds exactly;
neither may be narrowed to the signed 64-bit runtime carrier.

For example, this warns when more than 5% of constructed output rows have a
missing `VAL`:

```yaml
verifications:
  - row_count:
      id: too_many_missing
      filter: "VAL IS NULL"
      max_fraction: 0.05
      severity: warning
```

### Counting a group

<a id="req-0386"></a>

**REQ-0386.** `row_count` counts constructed output rows. `group_by` names
declared columns and partitions **the artifact's rows** by each value type's
equality, including [Text values](../values/text.md) for strings. Missing values group with
other missing values as [Row construction](rows.md) partitions an input dataset. Both bounds then
apply to every group.

<a id="req-0387"></a>

**REQ-0387.** `filter` is an [Predicates](../operations/predicates.md) predicate over one constructed output row, and
a group's count is how many of its rows the predicate admits. A row counts
only when the predicate is `TRUE`, so `FALSE` and `UNKNOWN` do not
count, as `filter` means everywhere else.

Grouping the artifact rather than the counted rows is what makes an exact
cardinality statable. Exactly one baseline record for each subject and
parameter is a `min` and a `max` of one over the rows whose baseline flag is
`Y`, grouped by subject and parameter. The subject has records, so a group
holding no flagged record fails the `min` instead of
disappearing. `unique` admits no filter and cannot state this. `assert`
sees one row at a time and cannot count the group. A
failure reports the offending groups and their counts.

<a id="req-0388"></a>

**REQ-0388.** Bounds still apply only to the groups the artifact contains. A
subject, visit, or parameter absent from the artifact entirely forms no
group, so no `min` here can discover it. The absent-group assertion belongs to
the derivation, where the relation defining the expected groups is readable: a
record lookup declaring `unmatched: fail` under [Lookup and joins](../operations/lookup.md) rejects an expected
group the data cannot supply. A planning relation at the required keys
gives every expected group an input record under [Row construction](rows.md).

<a id="req-1154"></a>

**REQ-1154.** `row_count` may declare `when`, a [Predicates](../operations/predicates.md)
predicate over one constructed output row. A group is bound when at least one of
its rows evaluates `when` to `TRUE`; the bounds then apply to that group as
[REQ-0386](verification.md#req-0386) and [REQ-0387](verification.md#req-0387) define. A group
no row of which evaluates `when` to `TRUE` is exempt: the bounds do not apply
to it. Without `group_by`, the whole output is one group, bound when any row
satisfies `when`. `FALSE` and `UNKNOWN` do not bind, as `filter` admits only
`TRUE` rows everywhere else. One conditional existence is at least one
baseline record for each subject and parameter that carries a baseline
value. That check is a `min` of one over the rows whose baseline flag is
set, grouped by subject and parameter. Its `when` selects the rows that
carry a baseline value. A group whose rows never carry one is not required
to have one.

### Severity

<a id="req-0389"></a>

**REQ-0389.** Every column and dataset verification may declare `severity`
inside its operation payload. Its value is exactly `error` or `warning`; it
defaults to `error`. Omitting it therefore preserves the behavior of every
version 1.0 specification written before severity existed.

<a id="req-0390"></a>

**REQ-0390.** A violated `warning` does not fail execution, remove or change a
row, or make the primary artifact ineligible for publication. The executor
continues through later column checks, key validation, and dataset checks. The
executor collects warning violations in that order. An `error` still stops at
the same [Execution lifecycle](lifecycle.md) stage as before.
Warning findings before a later error do not make the failed run
successful. No accepted artifact is produced.

### The run report

<a id="req-1173"></a>

**REQ-1173.** Once a specification is admitted, every run builds one run
report, whether it succeeds or fails, and every host returns it;
[REQ-1330](../storage/publication.md#req-1330) states which report the host
returns for a producer graph. The report records what the run read, counted,
and checked. `output.report` names the file the report is written to and has
no default. [Artifact publication](../storage/publication.md) selects the
file's profile from its path, requires it to differ from `output.path`, and
writes it for a failed run too, replacing a stale report from an earlier run.

<a id="req-0391"></a>

**REQ-0391.** A specification declaring any `warning` verification must
declare `output.report`. A specification whose checks are all `error` may omit
it.

<a id="req-1177"></a>

**REQ-1177.** A specification refused while it is prepared, before its run
begins, has no report: its issues are the whole result. Once the run has
begun, every failure is a failed run with a report, including a declaration
finding raised when its inputs are bound. A run that stops before it reads an
input or before execution still reports its run, its inputs, and its row
templates, leaving missing every count it never reached.

The report is diagnostic output, never one of
[Artifact publication](../storage/publication.md)'s artifacts. A failed run's
report records the failure but makes nothing publishable, and
[REQ-0390](verification.md#req-0390) is unchanged: a failed `error`
verification still produces no accepted artifact.

<a id="req-0396"></a>

**REQ-0396.** The report is built after the run's last stage, once the outcome
is final: after every verification and the output declaration checks for a run
that got that far, and after the primary artifact is encoded. A report that
cannot be built, verified, or rendered refuses the build under
[REQ-1178](verification.md#req-1178) instead of recording a failure inside
itself.

### Report rows

<a id="req-0393"></a>

**REQ-0393.** The report's rows come in this order: one `run` row; one `input`
row per input dataset, in declaration order; one `rows` row per row template,
or one for root `base` without row templates; one `handler` row per handler
path, in execution order, zero counts included; one `column` row per output
column, in `output.columns` order, for a successful run only; and one `check`
row per evaluated verification, in execution order. A check a stopped run never
reached has no row.

<a id="req-1175"></a>

**REQ-1175.** `SPEC_PATH` names each row's stable specification item and is
unique within the report:

| Kind | `SPEC_PATH` |
|---|---|
| `run` | `output` |
| `input` | `input.<NAME>` |
| `rows` | `rows[<i>]`, or `base` without row templates |
| `handler` | the handler path [REQ-0361](handlers.md#req-0361) counts |
| `column` | `columns.<NAME>` |
| `check` | the check's stable specification path |

The `rows` row of root `base` names the driver even when `base` is omitted and
the single declared input drives row construction. The handler rows are
exactly the handler paths the run counted under
[REQ-0361](handlers.md#req-0361), in execution order; a run that never reached
execution counted none. Check rows keep execution order: column declaration
order first, then dataset-verification order, each check that ran and held
included. A failed stage contributes the checks it evaluated, at least one of
them `violated` at `error` severity. A check that submission metadata adds,
such as the codelist enforcement of
[REQ-0940](../submission/terminology.md#req-0940), is a row like a declared
verification, at the path of the declaration that adds it. A value-level
codelist check runs once per bound codelist; each run is its own row, at the
path that declares that codelist.

### Report columns

<a id="req-0392"></a>

**REQ-0392.** The report has exactly these fourteen columns, in this order and
with these [Types and conversion](../values/types.md) types. A kind leaves
missing each column the table gives it no value for. The report carries no
version and no timestamp: the same specification and inputs give the same
report bytes.

| Column | Type | Value |
|---|---|---|
| `ARTIFACT` | `str` | the specification's `output.path` |
| `KIND` | `str` | `run`, `input`, `rows`, `handler`, `column`, or `check` |
| `SPEC_PATH` | `str` | the row's stable specification path ([REQ-1175](verification.md#req-1175)) |
| `NAME` | `str` | input dataset name; the driver dataset of a row template; handler name; column name; the check's registered verification name; missing for `run` |
| `ID` | `str` | a check's declared `id`, else missing |
| `TARGET` | `str` | the column a handler or a column check belongs to, else missing |
| `RULE` | `str` | the requirement that a check's condition, or the condition a handler answers, cites under [REQ-1297](lifecycle.md#req-1297), else missing |
| `SEVERITY` | `str` | check: `error` or `warning` |
| `OUTCOME` | `str` | run: `succeeded` or `failed`; check: `held` or `violated` |
| `CONDITION` | `str` | run: the fatal condition of a failed run; check: the failed condition |
| `COUNT` | `int` | run: output rows, missing when failed; input: records read; rows: rows emitted; handler: records answered; column: missing values; check: offending rows or groups, `0` when held |
| `TOTAL` | `int` | rows: driver records read; column: output rows; check: rows or groups evaluated; else missing |
| `KEYS` | `str` | violated check: every offending key or group as canonical JSON; else missing |
| `DETAILS` | `str` | canonical JSON: the condition context of a violated check, the input's file facts, a `str` column's longest value, else `{}` |

<a id="req-1174"></a>

**REQ-1174.** `NAME` of a check is its registered name under
[REQ-0371](verification.md#req-0371), and a column check's `TARGET` is the
column it infers under [REQ-0372](verification.md#req-0372). A check's `RULE`
is the requirement its condition cites, the condition the check reports when
it is violated, and is the same whether the check held or was violated;
`CONDITION` names that condition only on a violated check. A handler's
`TARGET` is the output column whose derivation declares it, missing for a named
intermediate's own derivations and match keys. A handler answers the condition
its operation raises without it, and its `RULE` is the requirement that
condition cites: `unconvertible` cites the conversion rule of
[REQ-0013](../values/types.md#req-0013) and `absent` the unknown-name rule of
[REQ-0103](../specification/binding.md#req-0103). `RULE` is missing whenever
the condition cites no requirement.

<a id="req-1176"></a>

**REQ-1176.** An input's `COUNT` is the records it decoded, and its `DETAILS`
is
`{"bytes": <byte length>, "path": "<path as declared>", "profile": "<profile>"}`,
with `"schema": "<producing specification path as declared>"` for an input
linked under [REQ-0520](../storage/ingestion.md#req-0520).
`bytes` is the byte length of the file snapshot read. For an input that reads
a producer's held serialized artifact under
[REQ-1247](../storage/resources.md#req-1247), it is the length of the encoded
artifact the consumer read. It is `null` for an input supplied as a host table
and for one never read, whose `COUNT` is missing.

A row template's `TOTAL` is every record of its input dataset driver, or the
records a named intermediate driver selects, and its `COUNT` is the rows it
emitted after its filter. Both are missing for a row template that did not
complete. A `column` row's `COUNT` is the column's missing values over the
output rows in `TOTAL`, and a `str` column's `DETAILS` is
`{"max_length": <n>}`, its longest present value in
[Text values](../values/text.md) scalar values (`0` when none is present).

A check's `COUNT` and `TOTAL` count the unit the check itself counts:

- output rows for a row-wise check;
- distinct combinations for `unique`;
- groups for a check that partitions the artifact, where an ungrouped
  `row_count` is the one group [REQ-0385](verification.md#req-0385) bounds.

A row-wise check's `TOTAL` is every output row it evaluated, a row whose
value is missing included even where the missing value passes the check under
[REQ-0375](verification.md#req-0375), and its `COUNT` is the rows that offend
it. `COUNT` is therefore never greater than `TOTAL`, and is `0` exactly when
`OUTCOME` is `held`. A held check leaves `KEYS` missing and `DETAILS` `{}`. A
violated one carries `KEYS` with the complete ordered sequence of offending
keys or groups rather than the bounded sample an error report may show, and
`DETAILS` with that failure's remaining context.

A violated `row_count` without `group_by` has `KEYS` `[{}]` under
[REQ-0394](verification.md#req-0394) and `DETAILS` `{"count": <n>}`, with
`"denominator": <d>` added when the check declares a fraction bound: `<n>` is
the rows its `filter` admits and `<d>` every row of the artifact, as
[REQ-0385](verification.md#req-0385) divides them. Only a grouped `row_count`
uses `counts`: its `DETAILS.counts` is the complete sequence of observed counts
aligned with the offending groups in `KEYS`, and `DETAILS.denominators`,
present under a fraction bound, holds the rows of each of those groups.

<a id="req-0394"></a>

**REQ-0394.** `KEYS` and `DETAILS` are compact ASCII JSON:

- no insignificant whitespace;
- object names ordered by [Text values](../values/text.md);
- `null`, `true`, and `false` in lower case;
- numbers in [Types and conversion](../values/types.md)'s `str` form;
- strings escaped to ASCII by JSON's short escapes where one exists and
  lower-case `\\u` hexadecimal escapes otherwise.

A scalar above `U+FFFF` is its JSON surrogate-pair escape. `KEYS` is an array
holding one object per offending row, its output keys by name, or per
offending group, its grouping columns by name; an ungrouped `row_count` has the
one empty group `[{}]`. Empty remaining context is `{}`.

### Report verification

<a id="req-0395"></a>

**REQ-0395.** The report is verified before any host sees it or any byte of
it is written. These properties of the report must hold:

- its columns, types, and order;
- one row per run, input dataset, row template, handler path, output column,
  and evaluated check, each kind in order;
- a non-missing `SPEC_PATH` unique within the report;
- each kind's outcome, severity, and condition vocabularies;
- `KEYS` on exactly the violated checks;
- `COUNT` no greater than `TOTAL` where both exist.

A malformed report is an execution defect, not a finding it can record.

<a id="req-1178"></a>

**REQ-1178.** The rows correspond one to one with what the executor
evaluated: the declared inputs, the row templates, the handler paths it
counted, the declared output columns, and the checks it evaluated. A run fails
with a named condition only under [REQ-0373](verification.md#req-0373) and the
other rules. A report that fails its own verification refuses the build, and
no partial report is returned or written.

### Interface behavior

<a id="req-1152"></a>

**REQ-1152.** The `dataset_verifications.row_count` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `Result` | Bounds how many rows a group holds, or the whole output. group_by partitions the artifact's rows and applies each bound to every group; a group's count is how many of its rows filter admits, and a row is admitted only when the predicate is TRUE. Fraction bounds divide this count by all rows in the group. The id is optional for either form. |
| `dataset_verifications.row_count.when` | A predicate over one constructed output row. A group is bound when at least one of its rows evaluates the predicate to TRUE; the bounds then apply to that group. A group no row of which evaluates it to TRUE is exempt. |

## Error conditions

<a id="req-0397"></a>

**REQ-0397.** An unknown verification keyword or field: schema failure. A
fraction bound outside 0 through 1 also fails.

<a id="req-0398"></a>

**REQ-0398.** A duplicate declared `id` within one dataset verification
list, one column's verification list, or one intermediate's verification
list: fail. The same ID may be used in a different list.

<a id="req-0399"></a>

**REQ-0399.** `range` or `row_count` with no bound, or with `min > max`:
  fail. A `row_count` with `min_fraction > max_fraction` also fails.

<a id="req-0400"></a>

**REQ-0400.** `max_length` whose `max` is less than one: fail.

<a id="req-0401"></a>

**REQ-0401.** `all_or_none` with fewer than two distinct columns: fail.

<a id="req-0402"></a>

**REQ-0402.** An unnamed grouped `row_count` is valid. Its failure report
contains the specification path, grouping columns, and observed counts;
the bounds are in the declaration at that path.

<a id="req-0403"></a>

**REQ-0403.** An empty or duplicated `row_count.group_by`: fail.

<a id="req-0404"></a>

**REQ-0404.** A verification applied to an incompatible column type: fail.

<a id="req-0405"></a>

**REQ-0405.** An unknown column in `unique`, `all_or_none`, `assert`, or
  `row_count.group_by`: fail.

<a id="req-0406"></a>

**REQ-0406.** Any `error` verification failure: fail and report it. A
  `warning` violation follows [REQ-0389](verification.md#req-0389) and
  [REQ-0390](verification.md#req-0390) instead, and the run report records it
  under [REQ-0393](verification.md#req-0393).

<a id="req-1179"></a>

**REQ-1179.** An `output.report` whose path collides with `output.path`, or
  whose extension names no profile: fail validation under
  [REQ-0756](../storage/publication.md#req-0756) and
  [REQ-1180](../storage/publication.md#req-1180).
