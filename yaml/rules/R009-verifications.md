---
id: R009
title: Verifications
status: normative
applies_to: [root.verifications, column.verifications, output.violation_log,
  column_verifications, dataset_verifications]

---

# Verifications

## Intent

Define closed, portable assertions over completed output values. Distinguish
fatal errors from reviewable warnings. Preserve every warning violation in a
governed sidecar dataset.

## Boundaries

This rule owns what each verification asserts, when it runs, and how a failure
is reported. R005 owns key uniqueness, which is checked by the output contract
rather than declared as a verification. R005 also owns the artifact's
row order, which no verification here observes. R004 owns the predicates that
`implies`, `predicate`, and a grouped `row_count` evaluate. R019 owns string
equality and scalar counting. R005 owns whether the primary artifact is
complete and publication-eligible. R020 owns the containers and publication of
the artifact and the violation log.

Verifications reach across rows only in deliberately fixed ways. `unique` and
`row_count` ask one question about the full output. A grouped `row_count` asks
one question per group. `all_or_none`, `implies`, and `predicate` see one
completed output row. No verification compares rows by order.

## An ordered frame is not a shape this rule has

**R009-1.** State an ordered-series assertion as a derivation followed by
a row-wise assertion in this rule. Use exactly one of three forms: the
adjacent row, a partition or its history up to the current row, or a derived
property at coarser keys or from an upstream specification.

- **The adjacent row.** `row_value` under R007 places another row's value on
  the row and `predicate` compares the two.
  `negative-adrs-partial-response-after-complete-response` uses
  the immediately preceding assessment, so it rejects a partial response next
  to a complete one and passes the same fault with an assessment in between.
- **A partition, or its history up to the current row.** A qualified aggregate
  under R013 reduces a source relation, and its `between` narrowing keeps only
  the records at or before a current-row value, so a cumulative property of
  collected values reaches the row it must be asserted about.
- **A derived property at coarser keys or from an upstream specification.** The specification that
  derives it publishes it, and R014's producing-specification link makes it an
  ordinary source field of the specification that asserts over it, which is
  the same split every other change of keys already uses.

**R009-2.** A frame assertion enters this vocabulary only when an example
needs that shape and the shape cannot be written as a producer and a consumer.

## Referential integrity is proven where the value is produced

**R009-3.** A supplemental qualifier record pointing at its parent domain
record, like every other cross-dataset link, is asserted by the derivation
that produces the link rather than by a verification over the finished
artifact. a lookup's `strict: true` rejects a value matching no record, and a
`lookup` result carried by a column declaring `not_missing` does the
same; `sdtm-suppmh-parent-linkage` links `IDVARVAL` to its medical-history
record that way.

**R009-4.** The claim that every parent record has a supplemental record is
not an artifact property, so no verification here can state the claim. A
specification that must assert the claim derives at the parent's keys.

## Registration and timing

**R009-5.** `schema_verification.yaml` registers column checks in
`column_verifications` and completed-dataset checks in
`dataset_verifications`.

**R009-6.** Column verifications infer the column on which they are declared.
They run after that column's derivation and conversion.
Dataset verifications run after all column work, output-key validation, and
column verifications finish.

**R009-7.** A failed `error` verification fails execution. Implementations
must report its stable specification path, failure count, and representative
offending keys. Reporting limits may be implementation options but must not
change pass or fail. A `warning` violation records every offending key under
R009-36 rather than applying the reporting limit.

**R009-8.** `all_or_none`, `implies`, `predicate`, and a `row_count`
declaring `group_by` require an `id`. These IDs must be unique across the
dataset verifications that declare them and should describe the asserted
business rule. Implementations must include the ID in failure reports in
addition to the stable specification path.

## Column verifications

- **R009-9.** `not_missing` passes only when every value is non-missing.
- **R009-10.** `allowed_values` requires every non-missing value to equal one
  listed value under its type's equality, including R019 for strings. Missing
  values pass; combine with `not_missing` when absence is invalid.
- **R009-11.** `range` requires every non-missing numeric value to be greater
  than or equal to `min` and less than or equal to `max` for the supplied
  bounds. At least one bound is required.
- **R009-12.** `max_length` requires every non-missing string to contain at
  most `max` R019 scalar values. Missing values pass; combine with
  `not_missing` when absence is invalid. It is declared only on a `str`
  column: the text a number or a temporal value renders as is a property of
  R011's rendering rather than of the value.
- **R009-13.** `matches` requires every non-missing string to match its
  regular expression. R022 owns the pattern's syntax, the engine that reads
  it, and whether a match must span the whole value.

**R009-14.** R019 scalar count rather than bytes or UTF-16 units is also the
unit R006 uses for `min_length`, so a supplementary-plane scalar counts once
in both R and Python. A length is therefore a separate check rather than
an anchored `matches` pattern. `max_length` counts scalar values directly,
not by regular-expression matching.

## Dataset verifications

- **R009-15.** `unique` requires the listed columns to exist and their
  combined values to be unique. Missing values participate as values; use
  column `not_missing` when they are prohibited.
- **R009-16.** `all_or_none` requires at least two distinct columns. For every
  output row, either every listed value must be missing or every listed value
  must be non-missing.
- **R009-17.** `implies` evaluates `when` and `then` for every output row.
  When `when` is `TRUE`, `then` must be `TRUE`; a `FALSE` or `UNKNOWN` result
  from `then` fails. When `when` is `FALSE` or `UNKNOWN`, the row passes
  because the rule does not apply.
- **R009-18.** `predicate` evaluates `assert` for every output row. Every
  result must be `TRUE`; `FALSE` and `UNKNOWN` fail. This verification covers
  row-wise rules that lack a more specific verification type.
- **R009-19.** `row_count` requires the output count to meet inclusive `min`
  and `max` bounds. At least one bound is required. `filter` and `group_by`
  narrow what it counts, as the next section defines.

## Counting a group

**R009-20.** `row_count` counts completed output rows. `group_by` names
declared columns and partitions **the artifact's rows** by each value type's
equality, including R019 for strings. Missing values group with
other missing values as R001 partitions an input dataset. Both bounds then
apply to every group.

**R009-21.** `filter` is an R004 predicate over one completed output row, and
a group's count is how many of its rows the predicate admits. A row counts
only when the predicate is `TRUE`, so `FALSE` and `UNKNOWN` do not
count, as `filter` means everywhere else.

Grouping the artifact rather than the counted rows is what makes an exact
cardinality statable. Exactly one baseline record for each subject and
parameter is a `min` and a `max` of one over the rows whose baseline flag is
`Y`, grouped by subject and parameter. The group exists because the subject
has records, so a group holding no flagged record fails the `min` instead of
disappearing. `unique` cannot state this because it admits no filter, and
`predicate` cannot because one row cannot see how many others exist. A
failure reports the offending groups and their counts.

**R009-22.** Bounds still apply only to the groups the artifact contains. A
subject, visit, or parameter absent from the artifact entirely forms no
group, so no `min` here can discover it. The absent-group assertion belongs to
the derivation, where the relation defining the expected groups is readable: a
record lookup declaring `unmatched: fail` under R015 rejects an expected
group the data cannot supply. A planning relation at the required keys
gives every expected group an input record under R001.

## Severity and the violation log

**R009-33.** Every column and dataset verification may declare `severity`
inside its operation payload. Its value is exactly `error` or `warning`; it
defaults to `error`. Omitting it therefore preserves the behavior of every
version 1.0 specification written before severity existed.

**R009-34.** A violated `warning` does not fail execution, remove or change a
row, or make the primary artifact ineligible for publication. The executor
continues through later column checks, key validation, and dataset checks and
collects warning violations in that order. An `error` still stops at the same
R005 stage as before. Warning findings collected before a later error do not
turn the failed run into a successful one. No accepted artifact is produced.

**R009-35.** A specification declaring any warning must declare
`output.violation_log`. Its path must differ from `output.path` and selects an
R020 profile by the same closed extension mapping. A successful run produces
this sidecar even when no warning is violated; the empty case is a header-only
dataset, so publication replaces a stale non-empty log from an earlier run.

**R009-36.** The violation log is version 1.0 and has exactly these columns,
in this order and with these R011 types:

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

**R009-37.** There is one row per violated warning declaration, keyed by
`SPEC_PATH`. Rows keep execution order: column declaration order first, then
dataset-verification order. `FAILURE_COUNT` is positive. `OFFENDING_KEYS` is
the complete ordered sequence, not the bounded sample an error report may
show. For grouped `row_count`, `DETAILS.counts` is the complete sequence of
observed counts aligned with those groups.

**R009-38.** The two JSON fields are compact ASCII JSON: no insignificant
whitespace; object names ordered by R019; `null`, `true`, and `false` in lower
case; numbers in R011's `str` form; and strings escaped to ASCII by JSON's
short escapes where one exists and lower-case `\\u` hexadecimal escapes
otherwise. A scalar above `U+FFFF` is its JSON surrogate-pair escape. The
empty key sequence is `[]` and empty remaining context is `{}`.

**R009-39.** The log is verified before it becomes an artifact: its columns,
types, order, non-missing and unique `SPEC_PATH`, fixed version and severity,
positive count, complete key sequence, and one-to-one correspondence with the
executor's warning findings must hold. A malformed log is an execution defect,
not a warning that can be logged inside itself.

**R009-40.** The log is built only for a successful execution, after all
verifications and before publication. A failure while building or serializing
it fails the run and leaves the primary artifact ineligible for publication.
R020 defines how a runner publishes the completed pair.

## Worked warning example

This check admits the completed ADSL artifact while recording the implausible
age for review:

```yaml
output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGE]
  violation_log: adsl-violations.csv

columns:
  - name: AGE
    type: int
    verifications:
      - range:
          min: 18
          max: 100
          severity: warning
```

For subject `P7-732` with age `214`, the primary row survives and the log has
one row. `CONDITION` is `range_failed`, `REQUIREMENT` is `R009-11`,
`SPEC_PATH` is `columns.AGE.verifications[0].range`, `FAILURE_COUNT` is `1`,
`OFFENDING_KEYS` is
`[{"STUDYID":"PILOT7","USUBJID":"P7-732"}]`, and `DETAILS` is
`{"column":"AGE"}`.

## Errors

- **R009-23.** An unknown verification keyword or field: schema failure.
- **R009-24.** A duplicate dataset-verification `id`: fail.
- **R009-25.** `range` or `row_count` with no bound, or with `min > max`:
  fail.
- **R009-26.** `max_length` whose `max` is less than one: fail. A column that
  admits no value at all is a column the specification should not declare.
- **R009-27.** `all_or_none` with fewer than two distinct columns: fail.
- **R009-28.** A `row_count` declaring `group_by` without an `id`: fail.
- **R009-29.** An empty or duplicated `row_count.group_by`: fail.
- **R009-30.** A verification applied to an incompatible column type: fail.
- **R009-31.** An unknown column in `unique`, `all_or_none`, `implies`,
  `predicate`, or `row_count.group_by`: fail.
- **R009-32.** Any `error` verification failure: fail and report it. A
  `warning` violation follows R009-33 through R009-40 instead.

## Rationale

An ordered-series assertion would carry its own partition, ordering,
frame-bound, and missing-value contract for a case no example has, and
nothing weaker than all four would be portable. The rule admits no frame
shape until an example needs one that a producer and a consumer cannot
express. Restating a cross-dataset match as a verification would duplicate
R015's matching, filtering, and multiple-match semantics inside an assertion
that runs long after the value the assertion doubts was consumed, so the
link is asserted where it is produced instead. A `max_length` on a rendered
number or temporal value would assert a property of R011's rendering rather
than of the value, so it is declared only on `str` columns. A column that
admits no value at all is a column the specification should not declare, so
`max_length` requires a `max` of at least one.
