---
id: operations/temporal
title: Temporal operations
status: normative
---

# Temporal operations

## Purpose

Compute calendar differences, study days, date completion, and precision.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Local handlers](../execution/handlers.md).
- [Verification](../execution/verification.md).
- [Numeric computation](computation.md).
- [Expression evaluation](expressions.md).
- [Temporal values](../values/temporal.md).


## Requirements

### Type behavior

<a id="req-0309"></a>

**REQ-0309.** `date_diff`, `study_day`, `date_impute`, `date_precision`,
`datetime_impute`, `datetime_precision`, and `to_date` use the input contracts
below; [Temporal values](../values/temporal.md) defines the values themselves.

### Partial collected dates

<a id="req-0578"></a>

**REQ-0578.** A study collects dates truncated to a year or year and month.
Neither is a `date`. Such a value is a `str` until completion.

<a id="req-0579"></a>

**REQ-0579.** `date_impute` completes the value by a declared rule, not string
surgery. The `date` result carries the source text's collected precision. The
value therefore records supplied components rather than leaving that fact to
the specification.

<a id="req-0580"></a>

**REQ-0580.** One precision ladder orders `year`, `month`, then `day`.
A policy names a ladder level: `minimum_source_precision` takes `year`
or `month`. `date_precision` returns the ladder code: `Y`, `M`, or `D`.
The levels correspond in order. The spellings name the same ladder.

<a id="req-0581"></a>

**REQ-0581.** `date_precision` reads a precision from either kind of source.
Given the collected text it reports how much of a date that text carries. Given
a temporal value it reports that value's collected precision. This lets a
specification derive an imputation flag from the analysis date itself.
Reading the date binds the flag to its value. Reading the text leaves the
flag and value in step only by convention, and drift is undetected.

<a id="req-0582"></a>

**REQ-0582.** A known day in an unknown month has no representation, and this
rule does not invent one. The collected text admitted above is prefix
truncation only: a year, or a year and a month. A day known without its month
cannot be collected in the first place, so there is no value for a precision to
describe, and the ladder is a prefix ladder for exactly that reason. A study
that records such a value keeps the collected text as `str`, as it does for
every other text this contract does not admit.

<a id="req-0583"></a>

**REQ-0583.** `minimum_source_precision` bounds how much `date_impute` may
invent. Its default is `year`, allowing year-only and year-month sources.
With `month`, a year-month source may receive the declared day. A valid
year-only source produces missing. Supplying month and day would
exceed the declared policy. A complete source date is always returned
unchanged. Falling below the minimum is neither a missing source nor invalid
text, so it does not invoke either [Local handlers](../execution/handlers.md) handler.

<a id="req-0584"></a>

**REQ-0584.** A `day` may name a position in its month instead of a number.
`first` and `last` are resolved after `month` is fixed, against the month the
completed date lands in, so `last` is 28 or 29 in a February according to the
year and 30 or 31 elsewhere. A study placing a partial date at the end of its
month therefore declares one rule rather than one rule per month, and the
completed value is a real calendar date by construction.

<a id="req-0585"></a>

**REQ-0585.** `not_before` bounds the completed date from below and moves only
what imputation supplied. The collected components of a truncated source
admit a day interval: `2025` admits its year and `2025-01` admits its month.
The bound may move the result only within that interval. A completed
date on or after the bound stands. Otherwise the result is the earliest
day the interval admits that satisfies the bound, so the bound invents no more

than it must. When no day in the interval satisfies the bound, the result is
missing. Like falling below minimum precision, this is neither missing nor
invalid text and invokes no handler. A missing bound is no bound.

<a id="req-0586"></a>

**REQ-0586.** A complete source date is returned unchanged whatever the bound
says. The date supplied nothing for the bound to move. This is what
makes the bound a rule rather than a comparison a specification could write
itself: the bound constrains an invented component and never a collected one.
A specification constraining collected dates states a verification under [Verification](../execution/verification.md),
which is where a claim about data a study recorded belongs.

<a id="req-0587"></a>

**REQ-0587.** The parameters apply in this order: a missing or invalid source,
then a source below the minimum precision, then completion from `month` and
`day`, then the bound.

<a id="req-0588"></a>

**REQ-0588.** Where both operations read the same source text they answer the
same two conditions about it, so one handler stage in [Local handlers](../execution/handlers.md) serves both: a
missing source, and a non-missing source that is neither a complete date nor a
date prefix. Text that is not a date is a different defect from an uncollected
value, and a specification may answer them differently. A `date_precision`
reading a value has only the first of the two to answer. A value that
exists is already a value of its type.

<a id="req-0589"></a>

**REQ-0589.** The date operations do not answer about a `datetime`.
`datetime_impute` supplies only the time of a complete collected date, under an
explicit first- or last-second rule; it does not supply a missing day.
`datetime_precision` reports whether a complete date or datetime source
carried a time. A source truncated before the day stays `str` and is invalid to
both datetime operations rather than silently combining date and time policies.

### Operations

<a id="req-0590"></a>

**REQ-0590.** [Expression evaluation](expressions.md) registers, orders, and
nests these operations. Their input and result types are:

| Operation | Inputs | Result |
|---|---|---|
| `date_diff` | `start` and `end` are `date` | `int` |
| `study_day` | `date` and `reference` are `date` | `int`, never zero |
| `date_impute` | inputs in [REQ-0578](temporal.md#req-0578)--53 | `date` |
| `date_precision` | `source` is `str` or `date` | `str` |
| `datetime_impute` | `source` is complete date or datetime text; `time` is `first` or `last` | `datetime` |
| `datetime_precision` | `source` is complete date or datetime text, or `datetime` | `str` |
| `to_date` | `source` is `datetime` | `date` with collected precision `day` |
| `to_epoch_day` | `source` is `date` | `int` days since 1970-01-01 |

<a id="req-1187"></a>

**REQ-1187.** `to_epoch_day` converts a `date` to the integer count of days
since 1970-01-01 on the proleptic Gregorian calendar: 1970-01-01 is `0` and
earlier dates are negative. The integer is ordinary arithmetic input, so a
per-record derivation converts dates once and aggregates reduce the resulting
integers; date logic never enters reduction. A `datetime` source is the
incompatible-input error [Types and conversion](../values/types.md) defines,
as with every other date operation, and text is not parsed: a text source
first becomes a `date` through `to_date` or `date_impute`. A missing source
returns missing.

<a id="req-0591"></a>

**REQ-0591.** `date_diff`, `study_day`, `date_impute`, and `date_precision` are
date operations. A `datetime` operand to one of those operations is an error
rather than a widened one. `date_diff` counts whole calendar units and its `bounds` field
counts endpoints of a day range, and neither has a meaning between two moments:
`unit: day` between `2025-01-01T23:00:00` and `2025-01-02T01:00:00` could be
`1` or `0`. Widening either operation would make that choice silently, so both
stay on `date`. A difference between two moments enters the vocabulary when an
example needs it.

<a id="req-0592"></a>

**REQ-0592.** `date_impute` requires its `month` and a numeric `day` to lie
within its registered calendar ranges, and the completed value
must be a real calendar date. `month` is required when
`minimum_source_precision` is `year` and must be absent when it is `month`:
a specification carries no value the precision policy leaves unreachable.
A `day` naming a position in its month is not a literal to range-check,
and the calendar-date requirement cannot fail for one: it names whichever day
the target month begins or ends with rather than a number that month might not
have. Any other `day` token is neither a number nor a position, and is rejected
where the specification is read.

<a id="req-0593"></a>

**REQ-0593.** A `datetime` is produced by converting datetime text or by
`datetime_impute`, which completes a date source under its declared time rule.
It is consumed by comparisons, `datetime_precision`, or `to_date`, which copies
calendar fields and drops time fields. A missing `to_date` source returns a
missing date. Any other source type is the incompatible-input error
[Types and conversion](../values/types.md) defines; in particular, a `date`
value is not accepted as an identity spelling.

### Whole calendar units

<a id="req-0594"></a>

**REQ-0594.** `date_diff` with `unit: day` is the calendar-date difference
`end` minus `start` in days. With `unit: week`, the result is the number of
whole seven-day blocks in that difference: the day count divided by seven,
with any remainder discarded.

<a id="req-0595"></a>

**REQ-0595.** With `unit: month`, `date_diff` counts how many monthly
anniversaries of `start` fall on or before `end`. The k-th anniversary
carries the year and month k months after `start`, with the day clamped
to the length of that month. With `unit: year` it counts yearly
anniversaries the same way. Three boundary cases pin the rule:
`2025-01-31` to `2025-02-28` is one month, `2024-02-29` to `2025-02-28`
is twelve months and one year, and `2025-01-31` to `2025-03-01` is one
month. The March anniversary of January 31 is March 31.

<a id="req-0596"></a>

**REQ-0596.** A February 29 anniversary in a common year falls on February
28. This names the clamping the previous requirement already states. It is
the case an age computation meets every leap year.

<a id="req-0597"></a>

**REQ-0597.** When `end` precedes `start`, the result is the negation of
the count with the operands exchanged. An earlier date therefore
produces a negative result in every unit, and no unit rounds toward
negative infinity.

<a id="req-0598"></a>

**REQ-0598.** `bounds` counts endpoints of a day range and is defined
only with `unit: day`. `exclusive` counts from `start` to `end`
excluding `start`; `inclusive` counts both endpoints and is one greater;
`between` counts neither and is one smaller. With `unit: week`,
`unit: month`, or `unit: year`, `bounds` must be absent or `exclusive`;
any other value has no meaning -- an age of 35 does not become 36 --
and is rejected where the specification is read, before any data is seen.

### Interface behavior

<a id="req-1104"></a>

**REQ-1104.** The `expressions.date_diff` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.date_diff.start` | Starting date. |
| `expressions.date_diff.end` | Ending date; an earlier date produces a negative result. |
| `expressions.date_diff.unit` | Calendar unit counted between the dates. |
| `expressions.date_diff.bounds` | Endpoints counted. exclusive counts from start to end excluding start; inclusive counts both endpoints and is one greater; between counts neither and is one smaller. The three values are defined only with unit day; with unit week, month, or year bounds must be absent or exclusive, and any other value fails validation under this operation contract. |
| `Result` | Counts whole calendar units between two dates. Which endpoints the count includes is declared by bounds. A missing start or end yields a missing result, so no guarding predicate is needed. |

<a id="req-1105"></a>

**REQ-1105.** The `expressions.date_impute` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.date_impute.source` | ISO 8601 date text, complete or truncated to year or month. |
| `expressions.date_impute.month` | Month used when the source carries only a year, 1 to 12; required when `minimum_source_precision` is `year`, absent when it is `month`. |
| `expressions.date_impute.day` | Day used when the source carries no day: an integer from 1 to 31, or first or last resolved against the month the date lands in. |
| `expressions.date_impute.minimum_source_precision` | Least precision the collected source must carry before imputation; month leaves a year-only source missing instead of supplying both month and day. |
| `expressions.date_impute.not_before` | Date the completed value must not precede; it moves only the components imputation supplied, and never a collected date. |
| `expressions.date_impute.missing` | Result when the source value is missing. |
| `expressions.date_impute.invalid` | Result when the source is not an ISO 8601 date or date prefix. |
| `Result` | Completes a truncated ISO 8601 date and returns a date. A complete source date is returned unchanged; a source carrying only a year, or a year and month, is completed from month and day unless minimum_source_precision forbids supplying that much information. A day token is resolved after month is fixed, so it names a day in the month the completed date lands in. not_before is applied last, to the completed date alone: a date already on or after the bound stands, and otherwise the result is the earliest day the collected text still admits that satisfies the bound. When the collected text admits no such day the result is missing, invoking no handler. A source that is not an ISO 8601 date or an ISO 8601 date prefix is an invalid value, distinct from a missing one; omitting the `missing` or `invalid` handler makes its condition fatal per [REQ-0344](../execution/handlers.md#req-0344). [Temporal values](../values/temporal.md) governs the resulting date. |

<a id="req-1106"></a>

**REQ-1106.** The `expressions.date_precision` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.date_precision.source` | ISO 8601 date text, complete or truncated to year or month, or a date whose collected precision is reported. |
| `expressions.date_precision.missing` | Result when the source value is missing. |
| `expressions.date_precision.invalid` | Result when the source is not an ISO 8601 date or date prefix. |
| `Result` | Returns how much of a date its source carries. D for a complete date, M for a year and month, and Y for a year alone. Collected text reports what the text carries and a date reports the collected precision that value carries, so a specification records what date_impute supplied by reading either the same source or the completed date itself. Invalid text and a missing source behave as they do there; a date source is never invalid. A datetime is an error here as in every date operation. [Temporal values](../values/temporal.md) governs both source kinds. |

<a id="req-1107"></a>

**REQ-1107.** The `expressions.to_date` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.to_date.source` | Datetime whose calendar date is returned, or ISO 8601 date text to parse. |
| `Result` | Extracts the calendar date from a datetime, or parses ISO 8601 date text directly. A missing source yields a missing date. Other source types are an incompatible input error under this operation contract; text that is not a complete ISO date is invalid date text. |

<a id="req-1108"></a>

**REQ-1108.** The `expressions.study_day` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.study_day.date` | Date whose study day is returned. |
| `expressions.study_day.reference` | Reference start date, which is study day 1. |
| `Result` | Returns the CDISC study day of a date against a reference date. The reference date is day 1 and there is no day zero: a date on or after the reference counts forward from 1, and an earlier date counts back from -1. A missing date or reference yields a missing result. |

<a id="req-1188"></a>

**REQ-1188.** The `expressions.to_epoch_day` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.to_epoch_day.source` | Date converted to days since 1970-01-01. |
| `Result` | Returns the integer count of days since 1970-01-01 for a date. A missing source yields a missing result. A `datetime` or text source is an incompatible input error under this operation contract. |

<a id="req-1109"></a>

**REQ-1109.** The `day_rule` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `day_rule` | Day resolved against the month the completed date lands in. |

<a id="req-1182"></a>

**REQ-1182.** The `expressions.datetime_impute` interface has the following
meanings. Shape, defaults, and structural constraints come from its schema
declaration.

| Field | Meaning |
| --- | --- |
| `expressions.datetime_impute.source` | ISO 8601 datetime text or complete date text. |
| `expressions.datetime_impute.time` | Time supplied when the source has no time: `first` is `00:00:00` and `last` is `23:59:59`. |
| `expressions.datetime_impute.missing` | Result when the source value is missing. |
| `expressions.datetime_impute.invalid` | Result when the source is neither a datetime nor a complete date. |
| `Result` | Returns a datetime. A complete datetime is returned unchanged with collected precision `second`; a complete date receives the declared edge of its day and carries collected precision `day`. A source truncated before its day is invalid rather than receiving a second imputation policy. Missing and invalid sources yield no datetime unless their local handlers are declared. |

<a id="req-1183"></a>

**REQ-1183.** The `expressions.datetime_precision` interface has the following
meanings. Shape, defaults, and structural constraints come from its schema
declaration.

| Field | Meaning |
| --- | --- |
| `expressions.datetime_precision.source` | ISO 8601 datetime or complete date text, or a datetime whose collected precision is reported. |
| `expressions.datetime_precision.missing` | Result when the source value is missing. |
| `expressions.datetime_precision.invalid` | Result when the source is neither a datetime nor a complete date. |
| `Result` | Returns `S` when the source carried a time and `D` when `datetime_impute` supplied it. Reading a datetime value binds the answer to the completed value; reading its source text reports the same distinction before completion. Invalid text and a missing source behave as they do for `datetime_impute`; a datetime value is never invalid. |

<a id="req-1184"></a>

**REQ-1184.** The `time_rule` interface has the following meanings. Shape,
defaults, and structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `time_rule` | Whole second at the beginning or end of the completed day. |

## Error conditions

<a id="req-0337"></a>

**REQ-0337.** A violation of this operation contract or the [Temporal values](../values/temporal.md)
contract must fail.

<a id="req-0606"></a>

**REQ-0606.** A
date operation other than `to_date` given a `datetime`: fail rather than widen
the operation. This includes a `datetime` reaching `date_precision` as a value
source.

<a id="req-0607"></a>

**REQ-0607.** `to_date` given anything other than a `datetime` or ISO 8601 date
text: fail as an incompatible input. Text that is not a complete ISO date fails
as invalid date text. A missing source yields a missing date instead.

<a id="req-0608"></a>

**REQ-0608.** `date_impute` whose `month` or numeric `day` is outside the
calendar range, or whose completed value is not a real calendar date: fail.
Neither can arise from a `day` naming a position in its month.

<a id="req-0609"></a>

**REQ-0609.** `date_impute` whose `day` is a token that is neither a number nor a declared
position: rejected where the specification is read, before any data is seen.

`datetime_impute` whose `time` is neither `first` nor `last` is rejected the
same way under [REQ-1184](temporal.md#req-1184).

<a id="req-0610"></a>

**REQ-0610.** `date_impute` whose completed value cannot satisfy `not_before`
within the interval its collected components admit: missing, no failure. Like
a source below minimum precision, it is neither missing nor invalid
text, so no [Local handlers](../execution/handlers.md) handler answers it.

<a id="req-0611"></a>

**REQ-0611.** A temporal value used as an
operand in a `compute` expression: fail under [Numeric computation](computation.md), which admits only numeric
identifiers.

<a id="req-0613"></a>

**REQ-0613.** A `date_diff` with a non-`exclusive` `bounds` beside a
non-`day` `unit` fails validation with condition `value_not_permitted`,
naming the offending `bounds` value and the permitted value
`exclusive`. Like every validation failure, no handler answers it and
no artifact is accepted.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [adam-adae-partial-dates](../../benchmarks/adam-adae-partial-dates/README.md).
- [adam-adsl-treatment](../../benchmarks/adam-adsl-treatment/README.md).
- [negative-date-diff-units](../../benchmarks/negative-date-diff-units/README.md).
- [negative-date-diff-endpoints](../../benchmarks/negative-date-diff-endpoints/README.md).
- [negative-impute-bad-source](../../benchmarks/negative-impute-bad-source/README.md).
- [negative-impute-bad-month](../../benchmarks/negative-impute-bad-month/README.md).
- [negative-datetime-impute-bad-source](../../benchmarks/negative-datetime-impute-bad-source/README.md).
- [negative-datetime-precision-bad-source](../../benchmarks/negative-datetime-precision-bad-source/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Compute calendar differences, study days, date completion, and precision. This
topic lets other owners refer to one policy.
