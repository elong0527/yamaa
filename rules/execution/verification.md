---
id: execution/verification
title: Verification
status: normative
---

# Verification

## Purpose

Apply assertions, severity, and grouped counts to completed values.
Record what ran in the warning and verification logs.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Execution lifecycle](lifecycle.md).
- [Aggregation](../operations/aggregation.md).
- [Expression evaluation](../operations/expressions.md).
- [Predicates](../operations/predicates.md).
- [Text operations](../operations/text.md).
- [Schema language](../reference/schema-language.md).
- [Source ingestion](../storage/ingestion.md).
- [Artifact publication](../storage/publication.md).
- [Text values](../values/text.md).
- [Types and conversion](../values/types.md).


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
  under [Aggregation](../operations/aggregation.md) reduces a source relation, and its `between` narrowing keeps only
  the records at or before a current-row value, so a cumulative property of
  collected values reaches the row it must be asserted about.
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
A `lookup`'s `strict: true` rejects a value matching no record, and a
`lookup` result carried by a `not_missing` column does the same.
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
change pass or fail. A `warning` violation records every offending key under
[REQ-0392](verification.md#req-0392) rather than applying the reporting limit.

<a id="req-0374"></a>

**REQ-0374.** `all_or_none`, `implies`, `assert`, and a `row_count`
declaring `group_by` require an `id`. These IDs must be unique across the
dataset verifications that declare them and should describe the asserted
business rule. Implementations must include the ID in failure reports in
addition to the stable specification path.

### Column verifications

<a id="req-0375"></a>

**REQ-0375.** `not_missing` passes only when every value is non-missing.

<a id="req-0376"></a>

**REQ-0376.** `allowed_values` requires every non-missing value to equal one
  listed value under its type's equality, including [Text values](../values/text.md) for strings. Missing
  values pass; combine with `not_missing` when absence is invalid.

<a id="req-0377"></a>

**REQ-0377.** `range` requires every non-missing numeric value to be greater
  than or equal to `min` and less than or equal to `max` for the supplied
  bounds. At least one bound is required.

<a id="req-0378"></a>

**REQ-0378.** `max_length` requires every non-missing string to contain at
  most `max` [Text values](../values/text.md) scalar values. Missing values pass; combine with
  `not_missing` when absence is invalid. It is declared only on a `str`
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
  column `not_missing` when they are prohibited.

<a id="req-0382"></a>

**REQ-0382.** `all_or_none` requires at least two distinct columns. For every
  output row, either every listed value must be missing or every listed value
  must be non-missing.

<a id="req-0383"></a>

**REQ-0383.** `implies` evaluates `when` and `then` for every output row.
  When `when` is `TRUE`, `then` must be `TRUE`; a `FALSE` or `UNKNOWN` result
  from `then` fails. When `when` is `FALSE` or `UNKNOWN`, the row passes.
  The rule does not apply.

<a id="req-0384"></a>

**REQ-0384.** `assert` evaluates `expr` for every output row. Every
  result must be `TRUE`; `FALSE` and `UNKNOWN` fail. This verification covers
  row-wise rules that lack a more specific verification type.

<a id="req-0385"></a>

**REQ-0385.** `row_count` requires the output count to meet inclusive `min`
  and `max` bounds, and its proportion to meet inclusive `min_fraction` and
  `max_fraction` bounds. At least one bound is required. Fraction bounds must
  be between 0 and 1. The denominator is the number of unfiltered rows in
  the group, or the whole artifact without `group_by`. An empty artifact has
  proportion zero. `filter` and `group_by` determine the numerator and groups
  as the next section defines. `when` selects groups to check but does not
  change the denominator.

For example, this warns when more than 5% of completed output rows have a
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

**REQ-0386.** `row_count` counts completed output rows. `group_by` names
declared columns and partitions **the artifact's rows** by each value type's
equality, including [Text values](../values/text.md) for strings. Missing values group with
other missing values as [Row construction](rows.md) partitions an input dataset. Both bounds then
apply to every group.

<a id="req-0387"></a>

**REQ-0387.** `filter` is an [Predicates](../operations/predicates.md) predicate over one completed output row, and
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
predicate over one completed output row. A group is bound when at least one of
its rows evaluates `when` to `TRUE`; the bounds then apply to that group as
[REQ-0386](verification.md#req-0386) and [REQ-0387](verification.md#req-0387) define. A group
no row of which evaluates `when` to `TRUE` is exempt: the bounds do not apply
to it. Without `group_by`, the whole output is one group, bound when any row
satisfies `when`. `FALSE` and `UNKNOWN` do not bind, as `filter` admits only
`TRUE` rows everywhere else. A conditional existence -- at least one baseline
record for each subject and parameter that carries a baseline value -- is a
`min` of one over the rows whose baseline flag is set, grouped by subject and
parameter, with `when` selecting the rows that carry a baseline value; a group
whose rows never carry one is not required to have one.

### Functional dependency

<a id="req-1153"></a>

**REQ-1153.** `determines` requires its two listed columns to stand in a
functional dependency: within each group, every distinct value of the first
column is paired with exactly one distinct value of the second. `group_by`
partitions the artifact's rows exactly as [REQ-0386](verification.md#req-0386)
partitions them for `row_count`; without `group_by` the whole artifact is one
group. Missing values participate as values, as in [REQ-0381](verification.md#req-0381):
a missing determinant paired with two different dependents fails, while a
missing determinant paired only with a missing dependent passes. A one-to-one
mapping in both directions -- the code/decode bijection the ADaM conformance
rules require of pairs like `TRTP` and `TRTPN` -- is two `determines`
declarations with the columns reversed. `determines` requires an `id`. A
`determines` that does not list exactly two columns, declares no `id`, or
names an unknown column is rejected.

### Reference membership

<a id="req-1155"></a>

**REQ-1155.** `subset_of` requires every non-missing value of the declared
`column` to equal some value of `reference_column` in the named `dataset`.
The named dataset must be declared in the study document; it need not be a
derivation source of this specification, which is what distinguishes this
check from the producer-side link assertions [REQ-0369](verification.md#req-0369)
requires. Missing values pass; combine with `not_missing` when absence is
invalid. `subset_of` requires an `id`. The ADaM conformance rule that every
`USUBJID` appear in SDTM `DM` is a `subset_of` naming the study's `DM` dataset
and its `USUBJID` column. A `subset_of` that declares no `id`, names a
dataset the study document does not declare, or names an unknown column or
reference column is rejected.

### Severity and the warning log

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

<a id="req-0391"></a>

**REQ-0391.** A specification declaring any warning must declare
`output.warning_log`. Its path must differ from `output.path` and selects an
[Artifact publication](../storage/publication.md) profile by the same closed extension mapping. A successful run produces
this sidecar even when no warning is violated; the empty case is a header-only
dataset, so publication replaces a stale non-empty log from an earlier run.

<a id="req-0392"></a>

**REQ-0392.** The warning log is version 1.0 and has exactly these columns,
in this order and with these [Types and conversion](../values/types.md) types:

| Column | Type | Value |
|---|---|---|
| `LOG_VERSION` | `str` | `1.0` |
| `ARTIFACT` | `str` | the specification's `output.path` |
| `SEVERITY` | `str` | `warning` |
| `CONDITION` | `str` | the stable failed condition |
| `REQUIREMENT` | `str` | the numbered requirement defining the check |
| `SPEC_PATH` | `str` | the violated check's stable specification path |
| `VERIFICATION_ID` | `str` | its declared ID, or missing when it has none |
| `FAILURE_COUNT` | `int` | the total number of offending rows or groups |
| `OFFENDING_KEYS` | `str` | every offending key or group as canonical JSON |
| `DETAILS` | `str` | remaining condition context as canonical JSON |

<a id="req-0393"></a>

**REQ-0393.** There is one row per violated warning declaration, keyed by
`SPEC_PATH`. Rows keep execution order: column declaration order first, then
dataset-verification order. `FAILURE_COUNT` is positive. `OFFENDING_KEYS` is
the complete ordered sequence, not the bounded sample an error report may
show. For grouped `row_count`, `DETAILS.counts` is the complete sequence of
observed counts aligned with those groups.

<a id="req-0394"></a>

**REQ-0394.** The two JSON fields are compact ASCII JSON: no insignificant
whitespace; object names ordered by [Text values](../values/text.md); `null`, `true`, and `false` in lower
case; numbers in [Types and conversion](../values/types.md)'s `str` form; and strings escaped to ASCII by JSON's
short escapes where one exists and lower-case `\\u` hexadecimal escapes
otherwise. A scalar above `U+FFFF` is its JSON surrogate-pair escape. The
empty key sequence is `[]` and empty remaining context is `{}`.

<a id="req-0395"></a>

**REQ-0395.** The log is verified before it becomes an artifact: its columns,
types, order, non-missing and unique `SPEC_PATH`, fixed version and severity,
positive count, complete key sequence, and one-to-one correspondence with the
executor's warning findings must hold. A malformed log is an execution defect,
not a warning that can be logged inside itself.

<a id="req-0396"></a>

**REQ-0396.** The warning log is built only for a successful execution, after
all verifications and before publication. A warning-log build or serialization
failure fails the run and prevents primary-artifact publication.
[Artifact publication](../storage/publication.md) defines how a runner publishes the completed pair.

### The verification log

<a id="req-1173"></a>

**REQ-1173.** `output.verification_log` names a governed sidecar recording
the outcome of every verification the specification declares, held or
violated. Declaring it is independent of severity: a specification whose
checks are all `error` may declare it, and one declaring warnings may omit
it. [Artifact publication](../storage/publication.md) selects its profile from its path by the same closed
extension mapping and requires that path to differ from `output.path` and
`output.warning_log`. A run that declares the field always produces the
log, violations or not, so publication replaces a stale log from an
earlier run. This is what the warning log cannot state: a header-only log
is the same bytes whether every declared check held or the specification
declared no warning at all.

<a id="req-1174"></a>

**REQ-1174.** The verification log is version 1.0 and has exactly these
columns, in this order and with these [Types and conversion](../values/types.md) types:

| Column | Type | Value |
|---|---|---|
| `REPORT_VERSION` | `str` | `1.0` |
| `ARTIFACT` | `str` | the specification's `output.path` |
| `SPEC_PATH` | `str` | the check's stable specification path |
| `VERIFICATION_ID` | `str` | its declared ID, or missing when it has none |
| `CHECK` | `str` | the registered verification name |
| `TARGET` | `str` | the verified column, or missing for a dataset verification |
| `REQUIREMENT` | `str` | the numbered requirement defining the check |
| `SEVERITY` | `str` | `error` or `warning` |
| `OUTCOME` | `str` | `held` or `violated` |
| `CONDITION` | `str` | the stable failed condition, or missing when `OUTCOME` is `held` |
| `EVALUATED_COUNT` | `int` | rows, or groups for a grouped check, the check examined |
| `FAILURE_COUNT` | `int` | offending rows or groups; `0` when `OUTCOME` is `held` |
| `DETAILS` | `str` | remaining condition context as canonical JSON |

<a id="req-1175"></a>

**REQ-1175.** There is one row for every declared check rather than one row
per violation. A check that ran and held is therefore a row, which is what
distinguishes it from a check the specification never declared. Rows keep
execution order: column declaration order first, then dataset-verification
order, exactly as [REQ-0393](verification.md#req-0393) orders the log.
`SPEC_PATH` is the key. It is non-missing, unique within the log, and the
join to the warning log, whose row for the same path carries the complete
`OFFENDING_KEYS` evidence [REQ-0393](verification.md#req-0393) requires.
Offending keys stay out of the log so the same unbounded sequence is not
maintained in two places. `CHECK` is the verification's registered name under
[REQ-0371](verification.md#req-0371), and `TARGET` is the column a column
verification infers under [REQ-0372](verification.md#req-0372), missing for a
dataset verification.

<a id="req-1176"></a>

**REQ-1176.** `EVALUATED_COUNT` and `FAILURE_COUNT` count the unit the check
itself counts: output rows for a row-wise check, distinct combinations for
`unique`, and groups for a check that partitions the artifact, where an
ungrouped `row_count` is the one group [REQ-0385](verification.md#req-0385)
bounds. `FAILURE_COUNT` is therefore never greater than `EVALUATED_COUNT`,
and is `0` exactly when `OUTCOME` is `held`. A held check leaves `CONDITION`
missing and `DETAILS` `{}`. A violated one carries the stable condition its
failure reports and that failure's remaining context, encoded by
[REQ-0394](verification.md#req-0394) unchanged rather than by a second
encoding.

<a id="req-1177"></a>

**REQ-1177.** The log is written for a failed run as well as a successful
one. It is diagnostic output rather than one of [Artifact publication](../storage/publication.md)'s artifacts, so a
failed `error` verification still produces no accepted artifact and
[REQ-0390](verification.md#req-0390) is unchanged: the log records the
failure, it does not make the run publishable. Its rows are the checks the
run evaluated, in execution order. The stage that failed contributes the
checks it evaluated, at least one of them `violated` at `error` severity,
and a check a stopped run never reached has no row. The log
states what was checked and nothing more.

<a id="req-1178"></a>

**REQ-1178.** The log is verified before it is written: its columns,
types, and order; its fixed version; non-missing and unique `SPEC_PATH`;
`OUTCOME` exactly `held` or `violated`; the count relationship
[REQ-1176](verification.md#req-1176) fixes; and one-to-one correspondence
with the checks the executor evaluated, including agreement with the
warning log about every warning that log carries. A malformed log is an
execution defect, not a finding that can be recorded inside itself.

### Interface behavior

<a id="req-1152"></a>

**REQ-1152.** The `dataset_verifications.row_count` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `Result` | Bounds how many rows a group holds, or the whole output. group_by partitions the artifact's rows and applies each bound to every group; a group's count is how many of its rows filter admits, and a row is admitted only when the predicate is TRUE. Fraction bounds divide this count by all rows in the group. A grouped count requires id, which an ungrouped count does not. |
| `dataset_verifications.row_count.when` | A predicate over one completed output row. A group is bound when at least one of its rows evaluates the predicate to TRUE; the bounds then apply to that group. A group no row of which evaluates it to TRUE is exempt. |

## Error conditions

<a id="req-0397"></a>

**REQ-0397.** An unknown verification keyword or field: schema failure. A
fraction bound outside 0 through 1 also fails.

<a id="req-0398"></a>

**REQ-0398.** A duplicate dataset-verification `id`: fail.

<a id="req-0399"></a>

**REQ-0399.** `range` or `row_count` with no bound, or with `min > max`:
  fail. A `row_count` with `min_fraction > max_fraction` also fails.

<a id="req-0400"></a>

**REQ-0400.** `max_length` whose `max` is less than one: fail.

<a id="req-0401"></a>

**REQ-0401.** `all_or_none` with fewer than two distinct columns: fail.

<a id="req-0402"></a>

**REQ-0402.** A `row_count` declaring `group_by` without an `id`: fail.

<a id="req-0403"></a>

**REQ-0403.** An empty or duplicated `row_count.group_by`: fail.

<a id="req-0404"></a>

**REQ-0404.** A verification applied to an incompatible column type: fail.

<a id="req-0405"></a>

**REQ-0405.** An unknown column in `unique`, `all_or_none`, `implies`,
  `assert`, or `row_count.group_by`: fail.

<a id="req-0406"></a>

**REQ-0406.** Any `error` verification failure: fail and report it. A
  `warning` violation follows [REQ-0389](verification.md#req-0389) through [REQ-0396](verification.md#req-0396) instead.

<a id="req-1179"></a>

**REQ-1179.** An `output.verification_log` whose path collides with
  `output.path` or `output.warning_log`, or whose extension names no
  profile: fail validation under [REQ-1180](../storage/publication.md#req-1180).

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-adeg-rrr](../../benchmarks/negative-adeg-rrr/README.md).
- [negative-strata-mismatch](../../benchmarks/negative-strata-mismatch/README.md).
- [negative-dose-expansion](../../benchmarks/negative-dose-expansion/README.md).
- [negative-adlb-two-baselines](../../benchmarks/negative-adlb-two-baselines/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Apply assertions, severity, and grouped counts to completed values.
Record what ran in the warning and verification logs.
One contract avoids a second policy.
