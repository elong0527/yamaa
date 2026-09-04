---
id: R009
title: Verifications
status: normative
applies_to: [root.verifications, column.verifications, column_verifications, dataset_verifications]
depends_on: [R004, R005, R006, R011]
---

# Verifications

## Intent

Define closed, portable assertions over completed output values without using a
generic function or argument bag.

## Boundaries

This rule owns what each verification asserts, when it runs, and how a failure
is reported. R005 owns key uniqueness, which is checked by the output contract
rather than declared as a verification, and it owns the artifact's row order,
which no verification here observes. R004 owns the predicates that `implies`,
`predicate`, and a grouped `row_count` evaluate.

Verifications reach across rows only in fixed ways, deliberately. `unique` and
`row_count` ask one question about the output as a whole, and `row_count` asks
it once per group when it declares one; `all_or_none`, `implies`, and
`predicate` see one completed output row at a time. None of them compares rows
in an order.

## An ordered frame is not a shape this rule has

An assertion over an ordered series -- that no partial response ever follows a
complete response for a subject -- is a decision against, not an omission.
Three constructs already state every such case the suite has, each of them a
derivation followed by a row-wise assertion here:

- **The adjacent row.** `row_value` under R007 places another row's value on
  the row and `predicate` compares the two.
  `negative-adrs-partial-response-after-complete-response` does that against
  the immediately preceding assessment, so it rejects a partial response next
  to a complete one and passes the same fault with an assessment in between.
- **A partition, or its history up to the current row.** A qualified aggregate
  under R013 reduces a source relation, and its `between` narrowing keeps only
  the records at or before a current-row value, so a cumulative property of
  collected values reaches the row it must be asserted about.
- **A derived property at a coarser or earlier grain.** The specification that
  derives it publishes it, and R014's producing-specification link makes it an
  ordinary source field of the specification that asserts over it, which is
  the same split every other change of grain already uses.

What none of them reaches is a cumulative property of a *derived* value inside
one specification, because R013 admits no `between` on an unqualified aggregate
and no `CASE` in a reducer. A frame assertion enters this vocabulary when an
example needs exactly that and cannot be written as a producer and a consumer.
Until then it would carry its own partition, ordering, frame-bound, and
missing-value contract for a case no example has, and nothing weaker than all
four would be portable.

## Referential integrity is proven where the value is produced

A supplemental qualifier record pointing at its parent domain record, like
every other cross-dataset link, is asserted by the derivation that produces the
link rather than by a verification over the finished artifact. R015's
`unmatched: fail` rejects a value matching no record, and a `mapping_from`
result carried by a column declaring `not_missing` does the same;
`sdtm-suppmh-parent-linkage` links `IDVARVAL` to its medical-history record
that way. Restating the match here would duplicate R015's matching, filtering,
and multiple-match semantics inside an assertion, and that assertion would run
long after the value it doubts was consumed.

The opposite direction -- every parent record has a supplemental record -- is
not a property of the artifact, so no verification here can state it. A
specification that must assert it derives at the parent's grain.

## Registration and timing

`schema_verification.yaml` registers column checks in `column_verifications`
and completed-dataset checks in `dataset_verifications`.

Column verifications infer the column on which they are declared. They run
after that column's derivation, conversion, and final override. Dataset
verifications run after every column, output-key validation, and column
verification is complete.

A failed verification fails execution. Implementations must report its stable
specification path, failure count, and representative offending keys. Reporting
limits may be implementation options but must not change pass or fail.

`all_or_none`, `implies`, `predicate`, and a `row_count` declaring `group_by`
require an `id`. These IDs must be unique across the dataset verifications that
declare them and should describe the asserted business rule. Implementations
must include the ID in failure reports in addition to the stable specification
path.

## Column verifications

- `not_missing` passes only when every value is non-missing.
- `allowed_values` requires every non-missing value to equal one listed value.
  Missing values pass; combine with `not_missing` when absence is invalid.
- `range` requires every non-missing numeric value to be greater than or equal
  to `min` and less than or equal to `max`, for whichever bounds are supplied.
  At least one bound is required.
- `max_length` requires every non-missing string to be at most `max` Unicode
  code points long. Missing values pass; combine with `not_missing` when
  absence is invalid. It is declared only on a `str` column: a length counts
  the characters of a stored string, and the text a number or a temporal value
  renders as is a property of R011's rendering rather than of the value.
- `matches` requires every non-missing string to match its ECMAScript regular
  expression. Matching searches unless the pattern is anchored.

`max_length` counts code points rather than bytes or UTF-16 units, which is
the unit R006 already fixes for its own `min_length`, so one character is one
count in both R and Python whatever plane it comes from. A length is therefore
a check of its own rather than an anchored `matches` pattern: `.` in an
ECMAScript regular expression counts UTF-16 code units and excludes line
terminators, so the same value could pass in one runtime's spelling of the
bound and fail in another's.

## Dataset verifications

- `unique` requires the listed columns to exist and their combined values to be
  unique. Missing values participate as values; use column `not_missing` when
  they are prohibited.
- `all_or_none` requires at least two distinct columns. For every output row,
  either every listed value must be missing or every listed value must be
  non-missing.
- `implies` evaluates `when` and `then` for every output row. When `when` is
  `TRUE`, `then` must be `TRUE`; a `FALSE` or `UNKNOWN` result from `then`
  fails. When `when` is `FALSE` or `UNKNOWN`, the row passes because the rule
  does not apply.
- `predicate` evaluates `assert` for every output row. Every result must be
  `TRUE`; `FALSE` and `UNKNOWN` fail. It is the escape hatch for row-wise rules
  that do not match a more specific verification type.
- `row_count` requires the output count to meet inclusive `min` and `max`
  bounds. At least one bound is required. `filter` and `group_by` narrow what
  it counts, as the next section defines.

## Counting a group

`row_count` counts completed output rows. Two optional fields change how many
counts it makes and which rows each one counts:

- `group_by` names declared columns and partitions **the artifact's rows** by
  equality on their values, missing values grouping with other missing values
  as R001 partitions a driver relation. Both bounds then apply to every group.
- `filter` is an R004 predicate over one completed output row, and a group's
  count is how many of its rows the predicate admits. A row counts only when
  the predicate is `TRUE`, so `FALSE` and `UNKNOWN` do not count, which is what
  `filter` means everywhere else in the language.

Grouping the artifact rather than the counted rows is what makes an exact
cardinality statable. Exactly one baseline record for each subject and
parameter is a `min` and a `max` of one over the rows whose baseline flag is
`Y`, grouped by subject and parameter: the group exists because the subject has
records, so a group holding no flagged record fails the `min` instead of
disappearing. `unique` cannot state this because it admits no filter, and
`predicate` cannot because one row cannot see how many others exist. A failure
reports the offending groups and their counts.

**Bounds still apply only to the groups the artifact contains.** A subject,
visit, or parameter absent from the artifact entirely forms no group, so no
`min` here can discover it. That assertion belongs to the derivation, where the
relation defining the expected groups is readable: a record lookup declaring
`unmatched: fail` under R015 rejects an expected group the data cannot supply,
and a planning relation at the required grain gives every expected group a
driver record under R001.

## Errors

- An unknown verification keyword or field: schema failure.
- A duplicate dataset-verification `id`: fail.
- `range` or `row_count` with no bound, or with `min > max`: fail.
- `max_length` whose `max` is less than one: fail. A column that admits no
  value at all is a column the specification should not declare.
- `all_or_none` with fewer than two distinct columns: fail.
- A `row_count` declaring `group_by` without an `id`: fail.
- An empty or duplicated `row_count.group_by`: fail.
- A verification applied to an incompatible column type: fail.
- An unknown column in `unique`, `all_or_none`, `implies`, `predicate`, or
  `row_count.group_by`: fail.
- Any verification failure: fail and report it.
