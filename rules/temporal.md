# Dates and times

These operations compute with the `date` and `datetime` values of
[VALUE-13](values.md#value-13): calendar differences and study days,
completion of partial dates and times, collected precision, and civil
arithmetic with fixed-length durations.

## Operations

<a id="time-1"></a>

### TIME-1. Each temporal operation has fixed input and result types, and a date operation rejects a datetime

The temporal operations take and return the types below, under the input
contracts of these rules; [Expressions](expressions.md)
registers, orders and nests them. A temporal value never enters `compute`
arithmetic. A `datetime` given to a date operation other than `to_date`,
including as a `date_precision` value source, fails rather than widening the
operation, and so does any other violation of these input contracts or of the
temporal values.

Fails: `incompatible_input_type`.
Evidence: `negative-study-day-datetime`, `negative-date-diff-endpoints`, `negative-epoch-day-non-date`.
Replaces: REQ-0309, REQ-0337, REQ-0590, REQ-0591, REQ-0606, REQ-0611.

| Operation | Inputs | Result |
|---|---|---|
| `date_diff` | `start` and `end` are `date` | `int` |
| `study_day` | `date` and `reference` are `date` | `int`, never zero |
| `date_impute` | inputs in [TIME-7](#time-7) to [TIME-10](#time-10) | `date` |
| `date_precision` | `source` is `str` or `date` | `str` |
| `datetime_impute` | `source` is date or datetime text in a [TIME-11](#time-11) form, or `datetime`; `time` is `first` or `last` | `datetime` |
| `datetime_precision` | `source` is date or datetime text in a [TIME-11](#time-11) form, or `datetime` | `str` |
| `to_date` | `source` is `datetime`, ISO 8601 date text, or ISO 8601 datetime text | `date` with collected precision `day` |
| `to_epoch_day` | `source` is `date` | `int` days since 1970-01-01 |
| `date_shift` | `source` is `date`; `days` is `int` | `date` |
| `datetime_diff` | `start` and `end` are `datetime`; declared elapsed unit | `int` seconds or `float` minutes, hours or days |
| `datetime_shift` | `source` is `datetime`; numeric `amount`; declared elapsed unit | `datetime` |
| `duration_value` | `source` is duration text; declared elapsed unit | `int` whole seconds or `float` |

[VALUE-13](values.md#value-13) to [VALUE-21](values.md#value-21) define the
values themselves. `date_diff`, `study_day`, `date_impute` and
`date_precision` are date operations; a `datetime` operand to one of them is an
error rather than a widened operand. A temporal value used as an operand in a
`compute` expression fails under
[Formulas](formulas.md), which admits only numeric
identifiers.

<a id="time-2"></a>

### TIME-2. to_date takes a datetime's calendar fields, or parses complete ISO date or datetime text

`to_date` returns a `date` with collected precision `day`. From a `datetime` it
copies the calendar fields and drops the time fields; the truncation is never a
timezone conversion. ISO 8601 date text parses directly, and ISO 8601 datetime
text parses as a `datetime` first and is truncated by the same field copy. A
missing source yields missing. Any other input fails as an incompatible input,
and text that is neither a complete ISO date nor a complete ISO datetime fails
as invalid date text.

Fails: `incompatible_input_type`, `invalid_date_text`.
Evidence: `negative-date-conversion`, `adam-adae-post-dose-window`, `schema-birth-date`.
Replaces: REQ-0593, REQ-0607, REQ-1107.

A `datetime` is produced by converting datetime text, by `datetime_impute`,
which completes a date or a datetime collected to the hour or minute under its
declared time rule ([TIME-12](#time-12)), and by `datetime_shift`
([TIME-16](#time-16)). It is consumed by `datetime_diff`, `datetime_shift`,
comparisons, `datetime_precision` and `to_date`. The `datetime` value space is
zone-free ([VALUE-14](values.md#value-14)).

<a id="time-3"></a>

### TIME-3. to_epoch_day converts a date to its signed day count from 1970-01-01

`to_epoch_day` converts a `date` to the integer count of days since 1970-01-01
on the proleptic Gregorian calendar: 1970-01-01 is `0` and earlier dates are
negative. The result is ordinary arithmetic input. A missing source returns
missing. A `datetime` source is an incompatible input, as with every other date
operation, and text is not parsed: a text source first becomes a `date`
through `to_date` or `date_impute`.

Fails: `incompatible_input_type`.
Evidence: `schema-derive-date-epoch`, `negative-epoch-day-non-date`.
Replaces: REQ-1187, REQ-1188.

A per-record derivation converts dates once, and aggregates reduce the
resulting integers; date logic never enters reduction.

## Calendar differences

<a id="time-4"></a>

### TIME-4. date_diff counts whole calendar units from start to end, negative when end is earlier

`date_diff` counts whole calendar units between two dates. With `unit: day` it
is the calendar-date difference `end` minus `start` in days; with `unit: week`,
the number of whole seven-day blocks in that difference, any remainder
discarded. With `unit: month` it counts the monthly anniversaries of `start`
that fall on or before `end`, the k-th carrying the year and month k months
after `start` with its day clamped to that month's length; `unit: year` counts
yearly anniversaries the same way.

Evidence: `adam-adsl-analysis-age`, `adam-adsl-duration`, `sdtm-dm-age`.
Replaces: REQ-0594, REQ-0595, REQ-0596, REQ-0597, REQ-1104.

Three boundary cases pin the anniversary rule: `2025-01-31` to `2025-02-28` is
one month, `2024-02-29` to `2025-02-28` is twelve months and one year, and
`2025-01-31` to `2025-03-01` is one month, the March anniversary of January 31
being March 31. A February 29 anniversary in a common year falls on February
28.

When `end` precedes `start`, the result is the negation of the count with the
operands exchanged. An earlier `end` therefore produces a negative result in
every unit, and no unit rounds toward negative infinity. A missing `start` or
`end` yields a missing result, so no guarding predicate is needed. Which
endpoints a day count includes is declared by `bounds` ([TIME-5](#time-5)).

<a id="time-5"></a>

### TIME-5. date_diff bounds counts the endpoints of a day range and applies only with unit day

`bounds` counts the endpoints of a day range and is defined only with
`unit: day`. `exclusive` counts from `start` to `end` excluding `start`;
`inclusive` counts both endpoints and is one greater; `between` counts neither
and is one smaller. With `unit: week`, `unit: month` or `unit: year`, `bounds`
must be absent or `exclusive`; any other value fails validation with
`value_not_permitted`, where the specification is read, before any data is
seen.

Fails: `value_not_permitted`.
Evidence: `negative-date-diff-units`, `adam-adsl-crossover-periods`, `adam-adtte-os`.
Replaces: REQ-0598, REQ-0613.

`exclusive` is the default. The failure names the offending `bounds` value and
the permitted value `exclusive`. Like every validation failure, no handler
answers it and no artifact is accepted.

<a id="time-6"></a>

### TIME-6. study_day counts from the reference date as day 1, with no day zero

`study_day` returns the CDISC study day of `date` against `reference`, the
reference start date. The reference date is day 1 and there is no day zero: a
date on or after the reference counts forward from 1, and an earlier date
counts back from -1. A missing date or reference yields a missing result.

Evidence: `sdtm-vs-study-day`, `schema-birth-date`, `adam-adsl-randomization`.
Replaces: REQ-1108.

## Partial dates

<a id="time-7"></a>

### TIME-7. A partial date stays text, and date_precision reports how much of a date its source carries

A study collects dates truncated to a year, or to a year and month; neither is
a `date`, and such a value is a `str` until `date_impute` completes it. A known
day in an unknown month has no representation. One precision ladder orders
`year`, `month` and `day`, and `date_precision` returns its code, `Y`, `M` or
`D`: given collected text, how much of a date the text carries; given a `date`,
that value's collected precision.

Evidence: `sdtm-ae-partial-dates`, `sdtm-dm-age`, `schema-birth-date`.
Replaces: REQ-0578, REQ-0580, REQ-0581, REQ-0582, REQ-1106.

A policy names a ladder level: `minimum_source_precision` takes `year` or
`month`. The levels and codes correspond in order and name the same ladder.
`D` is a complete date, `M` a year and month, and `Y` a year alone. A study
that records a known day in an unknown month keeps the collected text as `str`.

A specification records what `date_impute` supplied by reading either the same
source text or the completed date itself. Invalid text and a missing source
behave as they do for `date_impute` ([TIME-13](#time-13)); a `date` source is
never invalid, and a `datetime` source is an error as in every date operation
([TIME-1](#time-1)). [VALUE-16](values.md#value-16) governs both source kinds.

<a id="time-8"></a>

### TIME-8. date_impute completes a truncated date from its declared month and day, within a minimum precision

`date_impute` returns a complete source date unchanged and completes a source
carrying only a year, or a year and month, from its declared `month` and
`day`, by a declared rule rather than string surgery; the `date` result carries
the source text's collected precision. `minimum_source_precision`, default
`year`, bounds what it may invent: with `month`, a year-month source receives
the declared day and a valid year-only source produces missing. A `day` of
`first` or `last` is resolved after `month` is fixed.

Evidence: `adam-adae-partial-dates`, `adam-adsl-alive-date`, `schema-birth-date`.
Replaces: REQ-0579, REQ-0583, REQ-0584, REQ-0587, REQ-1105, REQ-1109.

A `day` may name a position in its month instead of a number. `first` and
`last` are resolved against the month the completed date lands in, so `last`
is 28 or 29 in a February according to the year and 30 or 31 elsewhere, and
the completed value is a real calendar date by construction.

The default `minimum_source_precision` allows year-only and year-month
sources. Falling below the minimum is neither a missing source nor invalid
text, so it invokes neither handler of [TIME-13](#time-13). A complete source
date is always returned unchanged.

The parameters apply in this order: a missing or invalid source, then a source
below the minimum precision, then completion from `month` and `day`, then the
bounds of [TIME-9](#time-9). A source that is not an ISO 8601 date or an ISO
8601 date prefix is an invalid value, distinct from a missing one
([TIME-13](#time-13)). [VALUE-16](values.md#value-16) governs the resulting
date.

<a id="time-9"></a>

### TIME-9. Imputation bounds move only the supplied components, and an unsatisfiable bound gives missing

`not_before` and `not_after` bound the completed date from below and above and
move only what imputation supplied, within the day interval the collected
components admit: `2025` admits its year and `2025-01` its month. A completed
date that satisfies its bounds stands. Otherwise the result is the day in the
interval and within the bounds nearest the completed date: the earliest when it
precedes `not_before`, the latest when it follows `not_after`. When no such day
exists, the result is missing, with no failure and no handler.

Evidence: `adam-adae-partial-dates`, `adam-adcm-end-date-bound`.
Replaces: REQ-0585, REQ-0586, REQ-0610, REQ-1323, REQ-1324, REQ-1325.

A missing bound is no bound. The two bounds may be declared together, and then
every candidate day must satisfy both; a missing value of either leaves only
the other. When no day in the interval lies between the bounds, including a
`not_after` earlier than `not_before`, the result is missing. For example,
`2025-03` completed to `2025-03-31` under a `not_after` of `2025-03-12` gives
`2025-03-12`; under a `not_after` of `2025-02-10` it gives missing, never a day
in February.

An unsatisfiable bound, like a source below the minimum precision, is neither
a missing source nor invalid text, so no [Column derivation](columns.md)
handler answers it. A complete source date is returned unchanged whatever its
bounds say; the bounds never move a collected date. A specification that
constrains collected dates states a verification under
[Verification](verification.md).

<a id="time-10"></a>

### TIME-10. date_impute's month and day must fit the calendar and its precision policy

`month` is required when `minimum_source_precision` is `year` and must be
absent when it is `month`. A declared `month` must lie from 1 to 12 and a
numeric `day` from 1 to 31, and the completed value must be a real calendar
date. A `day` that names a position in its month is not a literal to
range-check, and neither check can fail for it. Any other `day` token is
neither a number nor a position and is rejected where the specification is
read, before any data is seen.

Fails: `month_required`, `month_not_permitted`, `month_out_of_range`, `value_not_permitted`, `invalid_calendar_date`.
Evidence: `negative-impute-missing-month`, `negative-impute-bad-month`, `negative-impute-bad-day`.
Replaces: REQ-0592, REQ-0608, REQ-0609.

A specification carries no value the precision policy leaves unreachable: an
absent required `month` fails with `month_required`, and a `month` beside
`minimum_source_precision: month` fails with `month_not_permitted`. A `month`
outside the calendar range fails with `month_out_of_range` and a numeric `day`
outside it with `value_not_permitted`. A completed value that is not a real
calendar date fails with `invalid_calendar_date`. A `day` token that is neither
a number nor a declared position fails with `value_not_permitted`.

## Partial times

<a id="time-11"></a>

### TIME-11. The datetime operations read a complete date whose time may stop after its hour or minute

A study collects times truncated to the hour or to the minute, as ISO 8601
reduced precision allows. `datetime_impute` and `datetime_precision` read
source text in exactly the forms below, whose fields have the widths and ranges
of [VALUE-15](values.md#value-15). One precision ladder orders `day`, `hour`,
`minute` and `second`, each form carries the level of its last field, and
`datetime_precision` returns its code, `D`, `H`, `M` or `S`, for such text or
for a `datetime`'s collected precision.

Evidence: `adam-adpc-relative-times`, `adam-adae-post-dose-window`, `rust/crates/yamaa-adapters/tests/fixtures/temporal_imputation_reference.json`.
Replaces: REQ-1183, REQ-1326, REQ-1327, REQ-1329.

```text
source := date [ "T" hh [ ":" mm [ ":" ss ] ] ]
```

Any other text is invalid to both operations, including a zone, an offset, a
fractional second, hour 24, a leap second and a field of one digit.
`2025-03-04` is `day`, `2025-03-04T08` is `hour`, `2025-03-04T08:30` is
`minute`, and `2025-03-04T08:30:15` is `second`. `D` is a date alone, whose
whole time `datetime_impute` supplies.

Reading a `datetime` value binds the answer to the completed value; reading its
source text reports the same answer before completion. Invalid text and a
missing source behave as they do for `datetime_impute`
([TIME-13](#time-13)); a `datetime` value is never invalid.

Reading source text is not converting it. Text converted to a `datetime`
column, or ingested as one, follows [VALUE-15](values.md#value-15):
`2025-03-04T08:30` names second zero there, with collected precision `second`,
and `2025-03-04T08` is not a `datetime`. A specification that must record a
minute or hour precision keeps the collected text as `str` and completes it
with `datetime_impute`.

<a id="time-12"></a>

### TIME-12. datetime_impute keeps every collected field and supplies each missing time field from its rule

`datetime_impute` keeps every field its source text carries and supplies each
missing time field from its `time` rule: `first` supplies hour `00`, minute
`00` and second `00`, and `last` supplies hour `23`, minute `59` and second
`59`. The result carries the source form's collected precision. Text carrying
seconds, and a `datetime` value, are returned unchanged. It supplies only time
fields, never a missing day: a source truncated before its day stays `str` and
is invalid to both datetime operations.

Fails: `value_not_permitted`.
Evidence: `adam-adae-partial-times`, `adam-adsl-treatment`, `sdtm-vs-epoch-from-subject-elements`.
Replaces: REQ-0589, REQ-1182, REQ-1184, REQ-1328.

`2025-03-04T08` completes to `2025-03-04T08:00:00` under `first` and to
`2025-03-04T08:59:59` under `last`. Text collected to the hour or minute keeps
its collected fields and carries collected precision `hour` or `minute`; a
complete date receives the declared edge of its day and carries collected
precision `day`; text carrying seconds keeps collected precision `second`.

The `time` rule names the earliest or latest civil moment the collected source
admits: each missing time field takes its first or last value. A `time` that is
neither `first` nor `last` is rejected where the specification is read, as an
unknown `day` token is under [TIME-10](#time-10), and fails with
`value_not_permitted`.

The date operations do not answer about a `datetime` ([TIME-1](#time-1)).
`datetime_precision` reports how much of a time a complete date or datetime
source carried ([TIME-11](#time-11)). Missing and invalid sources yield no
datetime unless their local handlers are declared ([TIME-13](#time-13)).

<a id="time-13"></a>

### TIME-13. The imputation and precision operations answer a missing source and invalid text by their handlers

`date_impute`, `date_precision`, `datetime_impute` and `datetime_precision`
answer the same two conditions about their source: a missing source, by
`missing`, and a non-missing source that is not text the operation reads, by
`invalid`. For the date operations that text is a complete date or a date
prefix; for the datetime operations, a complete date or a datetime collected to
the hour, minute or second. One handler stage serves all four. A source that is
a temporal value is never invalid.

Fails: `missing_input`, `invalid_date_text`, `invalid_datetime_text`.
Evidence: `negative-impute-bad-source`, `negative-datetime-impute-bad-source`, `negative-datetime-precision-bad-source`.
Replaces: REQ-0588.

The stage is `impute` under [Column derivation](columns.md), where the
operations reading one source domain answer the same missing and invalid
conditions. A `date_precision` or `datetime_precision` reading a value has only
the missing source to answer. `invalid` fires only when the source is present
([COLUMN-11](columns.md#column-11)). Omitting a handler makes its
condition fatal ([COLUMN-10](columns.md#column-10)): a missing source
fails with `missing_input`, and invalid text with `invalid_date_text` for the
date operations or `invalid_datetime_text` for the datetime operations.

## Civil arithmetic

<a id="time-14"></a>

### TIME-14. date_shift moves a complete date by a signed whole-day count, without clamping

`date_shift` adds a signed whole-day count to a complete date on the proleptic
Gregorian calendar and returns a date, crossing month, year and leap-day
boundaries without clamping. The result preserves the source's collected
precision. A missing source or day count returns missing. A datetime or text
source, or a non-integer day count, fails validation as
`incompatible_input_type`. A result outside years 0001 through 9999 fails at
build as `conversion_failed` on the `convert` stage.

Fails: `incompatible_input_type`, `conversion_failed`.
Evidence: `adam-adsl-period-dates`, `rust/crates/yamaa-core/tests/support/diagnostic/temporal_arithmetic.rs`, `rust/tests/host-fixtures/cases.json`.
Replaces: REQ-1298.

`days` is a signed integer literal or integer variable; a negative count moves
backward. No local handler answers the arithmetic overflow.

<a id="time-15"></a>

### TIME-15. datetime_diff returns end minus start in a declared elapsed unit, with no zone adjustment

`datetime_diff` returns `end` minus `start` in the declared elapsed unit. Both
inputs must be datetimes; other input types, including dates, fail validation
as `incompatible_input_type`, and missing either input returns missing. In
`second` the exact signed whole-second difference is an `int`. In `minute`,
`hour` or `day` it is a `float`, that difference divided by 60, 3600 or 86400
under [Formulas](formulas.md), without rounding. There
is no timezone or daylight-saving adjustment.

Fails: `incompatible_input_type`.
Evidence: `adam-adpc-relative-times`, `adam-adex-infusion-duration`, `rust/crates/yamaa-core/tests/support/diagnostic/temporal_arithmetic.rs`.
Replaces: REQ-1299.

The datetimes are zone-free civil values ([VALUE-14](values.md#value-14)), and
a day is 86400 seconds. A sample taken before the dose therefore has a negative
relative time.

<a id="time-16"></a>

### TIME-16. datetime_shift moves a datetime by a signed elapsed amount, rounded to a whole second

`datetime_shift` adds a signed elapsed amount to a complete civil datetime and
returns a datetime. `second` requires an integer amount; `minute`, `hour` and
`day` admit integer or float amounts. Integer amounts are scaled exactly. A
float amount is multiplied by 60, 3600 or 86400 as binary64, rounded to whole
seconds with the one rounding of [FORMULA-16](formulas.md#formula-16), and
then added exactly. No timezone or daylight-saving adjustment occurs, and the
source's collected precision is preserved.

Fails: `incompatible_input_type`, `conversion_failed`.
Evidence: `adam-adae-post-dose-window`, `rust/crates/yamaa-core/tests/support/diagnostic/temporal_arithmetic.rs`, `rust/tests/host-fixtures/cases.json`.
Replaces: REQ-1300.

Other input types fail validation as `incompatible_input_type`, and a missing
source or amount returns missing. The rounding applies to the amount, so the
source second does not affect it: `0.025` minute is exactly `1.5` seconds and
adds `2` seconds. A day is 86400 seconds. A rounded result outside years 0001 through 9999 fails at build as
`conversion_failed` on the `convert` stage; no local handler answers the
arithmetic overflow.

<a id="time-17"></a>

### TIME-17. duration_value reads ISO 8601 fixed-length duration text as a number in a declared unit

`duration_value` reads ISO 8601 fixed-length duration text `[-]PnW` or
`[-]P[nD][T[nH][nM][nS]]` into a number in the declared elapsed unit. At least
one component is required, components appear once in the stated order, `T`
requires a time component, and weeks cannot combine with other components.
Only the last written component may carry a decimal fraction, with a dot or a
comma. Whitespace, a leading plus sign, component signs, exponent notation,
years and calendar months are invalid.

Fails: `incompatible_input_type`, `invalid_duration_text`.
Evidence: `adam-adex-infusion-duration`, `rust/crates/yamaa-core/tests/support/diagnostic/temporal_arithmetic.rs`, `rust/tests/host-fixtures/cases.json`.
Replaces: REQ-1301.

`PT1M` is one minute; `P1M` is a calendar month and is invalid. A week is seven
days and a day is 86400 seconds. A missing source returns missing without
invoking `invalid`. A non-text source fails validation as
`incompatible_input_type`.

Decimal components are combined exactly before conversion. In `second`, a
whole-second duration returns an `int` and a fractional-second duration a
`float`. In other units, the seconds are converted to binary64 and divided by
60, 3600 or 86400 under [Formulas](formulas.md). Text
outside the resulting numeric range is invalid: whole seconds must fit the
signed integer range, and float results must be finite.

Invalid text returns the literal `invalid` handler, and a null `invalid` gives
missing; without the handler, the condition `invalid_duration_text` is fatal on
the `impute` stage. A declared handler is counted only when reached.
