# Dates and times

These operations compute with the `date` and `datetime` values of
[VALUE-13](values.md#value-13): differences, study days and times of day,
reading of declared date forms, assembly and completion of collected dates and
times, collected precision and imputation flags, and civil arithmetic with
fixed-length durations.

## Operations

<a id="time-1"></a>

### TIME-1. Each temporal operation has fixed input types, and a type-dispatched one declares its type

The temporal operations take the input types below, under the input contracts
of these rules, and return the result types
[EXPR-14](expressions.md#expr-14) gives them; [Expressions](expressions.md)
registers, orders and nests them. `diff`, `shift`, `impute` and `precision`
declare a `type`, `date` or `datetime`, that fixes their input types and so
their results; it is never inferred from data. A field the declared type
requires must be present, and one it does not take must be absent. An input of
another type fails rather than converting, and a temporal value never enters
arithmetic.

Fails: `incompatible_input_type`, `missing_required_field`, `unknown_field`.
Evidence: `negative-study-day-datetime`, `negative-date-diff-endpoints`, `negative-epoch-day-non-date`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::temporal_declarations_report_their_refusals_before_the_imputations`.

| Operation | Inputs |
|---|---|
| `diff`, `type: date` | `start` and `end` are `date`; a calendar `unit` |
| `diff`, `type: datetime` | `start` and `end` are `datetime`; an elapsed `unit` |
| `study_day` | `source` and `reference` are `date` |
| `impute`, `type: date` | `source` is collected date text or a `date`; `not_before`, `not_after` and `fallback` are `date` |
| `impute`, `type: datetime` | `source` is collected date text or a `datetime`; `not_before`, `not_after` and `fallback` are `datetime` |
| `precision` | `source` is collected date text or a value of the declared type |
| `imputation_flag` | `source` is a `date` or a `datetime`, and a `datetime` with `part: time` |
| `to_date` | `source` is a `datetime` or collected date text |
| `date_text` | `source` is `str`; `forms` lists date forms |
| `to_epoch_day` | `source` is `date` |
| `shift`, `type: date` | `source` is `date`; `days` is `int` |
| `shift`, `type: datetime` | `source` is `datetime`; a numeric `amount` and an elapsed `unit` |
| `duration_value` | `source` is duration text; an elapsed `unit` |
| `duration_text` | `source` is `int` or `float`; an elapsed `unit` |
| `dtc` | `year`, `month`, `day`, `hour`, `minute` and `second` are `str` or `int` |
| `time_of_day` | `source` is `datetime` |

Collected date text is defined by [TIME-7](#time-7), and
[VALUE-13](values.md#value-13) to [VALUE-21](values.md#value-21) define the
values themselves. `shift` with `type: date` takes `days` and neither `amount`
nor `unit`, and with `type: datetime` takes `amount` and `unit` and not
`days`. `impute` takes `time` exactly when its type is `datetime`. Where the
specification is read, a field the type requires and the operation omits fails
with `missing_required_field`, and a field the type does not take fails with
`unknown_field`, naming the field and the declared type. A temporal value used
as an operand of arithmetic in expression-field text fails under
[Formulas](formulas.md), which admits only numeric operands there.

<a id="time-2"></a>

### TIME-2. to_date returns the calendar date of a datetime or of text carrying a complete date, and partial text gives missing

`to_date` returns a `date`. From a `datetime` it copies the calendar fields and
keeps the collected precision, capped at `day`; the truncation is never a
timezone conversion. From collected date text ([TIME-7](#time-7)) whose prefix
carries a complete date it returns that date, collected precision `day`,
whatever follows. A shorter prefix is partial: the result is missing and
counted as `partial`, and nothing fails. A missing source gives missing, an
interval or duration fails as [TIME-13](#time-13) states, and other text fails
as invalid date text.

Fails: `incompatible_input_type`, `invalid_date_text`, `interval_or_duration_text`.
Evidence: `negative-date-conversion`, `schema-to-date-partial`, `adam-adae-post-dose-window`, `schema-birth-date`, `rust/crates/yamaa-core/tests/temporal_date_text.rs`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/temporal_imputation.json::to_date_refuses_an_interval_with_its_source_and_row`.

`2025-03-04T08` gives `2025-03-04`, and `2025-03` gives missing. A known date
before an unknown time gives that date, so `2025-03-04T-:30` gives
`2025-03-04`, while `2025---04` and `--03-04` are partial. A `datetime`
completed from `2025-03` keeps collected precision `month`, while one parsed
from complete text gives `day`. A `date` source is an incompatible input. The
partial count is reported at the operation's `partial` path, zero included,
like a handler count under [COLUMN-14](columns.md#column-14). An interval or
duration fails with `interval_or_duration_text` and other text with
`invalid_date_text`; `to_date` declares no handler for either.

A `datetime` is produced by converting datetime text, by `impute` with
`type: datetime` ([TIME-8](#time-8)) and by `shift` ([TIME-14](#time-14)). It
is consumed by `diff`, `shift`, comparisons, `precision`, `imputation_flag` and
`to_date`. The `datetime` value space is zone-free
([VALUE-14](values.md#value-14)).

<a id="time-3"></a>

### TIME-3. to_epoch_day converts a date to its signed day count from 1970-01-01

`to_epoch_day` converts a `date` to the integer count of days since 1970-01-01
on the proleptic Gregorian calendar: 1970-01-01 is `0` and earlier dates are
negative. The result is ordinary arithmetic input. A missing source returns
missing. A `datetime` source is an incompatible input, as with every other date
operation, and text is not parsed: a text source first becomes a `date`
through `to_date` or `impute`.

Fails: `incompatible_input_type`.
Evidence: `schema-derive-date-epoch`, `negative-epoch-day-non-date`.

A per-record derivation converts dates once, and aggregates reduce the
resulting integers; date logic never enters reduction.

<a id="time-20"></a>

### TIME-20. time_of_day returns a datetime's time of day as whole seconds since midnight

`time_of_day` returns the time of day of a `datetime` as an `int` count of
seconds since midnight, from 0 to 86399: the hour times 3600, plus the minute
times 60, plus the second. A missing source gives missing. Collected precision
takes no part, so a time that `impute` supplied counts as the time it names.
A source of another type is an incompatible input.

Fails: `incompatible_input_type`.
Evidence: `adam-adae-analysis-time`, `negative-time-of-day-text`, `rust/crates/yamaa-core/tests/temporal_date_text.rs`.

`2025-03-04T08:30:15` gives `30615` and `2025-03-04T00:00:00` gives `0`. The
result is the ADaM `*TM` analysis time, which a SAS time value holds as the
same count of seconds. An `int` column holding it may declare a `TIMEw.`
display format such as `TIME8.`, which a transport file writes and a document
describes like any other numeric display format
([SUB-9](profiles/submission.md#sub-9), [XPT-9](profiles/xpt.md#xpt-9)). The
time carries no collected precision: the `*TMF` flag of a supplied time is
read from the `datetime` it came from ([TIME-18](#time-18)). A `str` or `date`
source fails as an incompatible input where the specification is read, as
[TIME-1](#time-1) states.

## Differences

<a id="time-4"></a>

### TIME-4. diff returns end minus start in whole calendar units for dates, or in an elapsed unit for datetimes

With `type: date`, `diff` counts whole calendar units from `start` to `end`:
`day` is the calendar-date difference, `week` the whole seven-day blocks in
it, and `month` or `year` the monthly or yearly anniversaries of `start` on or
before `end`, each anniversary's day clamped to its month's length. With
`type: datetime`, the exact signed difference in whole seconds is an `int` in
`second`, and in `minute`, `hour` or `day` a `float`, divided by 60, 3600 or
86400. An earlier `end` gives a negative result in every unit.

Fails: `value_not_permitted`, `incompatible_input_type`.
Evidence: `adam-adsl-analysis-age`, `adam-adsl-duration`, `sdtm-dm-age`, `adam-adpc-relative-times`, `adam-adex-infusion-duration`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::a_datetime_difference_refuses_a_date_operand`.

A `unit` the declared type does not count in fails with `value_not_permitted`
where the specification is read: `type: date` counts in `day`, `week`, `month`
and `year`, and `type: datetime` in `second`, `minute`, `hour` and `day`. A
`start` or `end` not of the declared type is an incompatible input under
[TIME-1](#time-1).

Three boundary cases pin the anniversary rule: `2025-01-31` to `2025-02-28` is
one month, `2024-02-29` to `2025-02-28` is twelve months and one year, and
`2025-01-31` to `2025-03-01` is one month, the March anniversary of January 31
being March 31. A February 29 anniversary in a common year falls on February
28.

When the `end` date precedes `start`, the result is the negation of the count
with the operands exchanged, so no calendar unit rounds toward negative
infinity. A missing `start` or `end` yields a missing result, so no guarding
predicate is needed. Which endpoints a day count of dates includes is declared
by `closed` ([TIME-5](#time-5)).

Datetimes are zone-free civil values ([VALUE-14](values.md#value-14)): there
is no timezone or daylight-saving adjustment, and a day is 86400 seconds. The
division follows [Formulas](formulas.md), without rounding. A sample taken
before the dose therefore has a negative relative time.

<a id="time-5"></a>

### TIME-5. `closed` sets which endpoints a day count between two dates includes, and only that count takes it

`closed` counts the endpoints of a day range of dates. `right` counts from
`start` to `end` excluding `start`; `both` counts both endpoints and is one
greater; `neither` counts neither and is one smaller. With `type: datetime`,
or with `unit: week`, `unit: month` or `unit: year`, `closed` must be absent
or `right`; any other value fails validation with `value_not_permitted`, where
the specification is read, before any data is seen.

Fails: `value_not_permitted`.
Evidence: `negative-date-diff-units`, `adam-adsl-crossover-periods`, `adam-adtte-os`.

`right` is the default, and `diff` takes no `left`. The failure names the
offending `closed` value and the permitted value `right`. Like every
validation failure, no handler answers it and no artifact is accepted.

<a id="time-6"></a>

### TIME-6. study_day counts from the reference date as day 1, with no day zero

`study_day` returns the CDISC study day of `source` against `reference`, the
reference start date. The reference date is day 1 and there is no day zero: a
date on or after the reference counts forward from 1, and an earlier date
counts back from -1. A missing date or reference yields a missing result.

Evidence: `sdtm-vs-study-day`, `schema-birth-date`, `adam-adsl-randomization`.

## Collected dates and times

<a id="time-7"></a>

### TIME-7. Collected date text is ISO 8601 read up to its first unknown field, and precision reports how much of the declared type a source carries

Collected date text is a year, `YYYY`, optionally followed in order by `-MM`,
`-DD`, `Thh`, `:mm` and `:ss`, each field with the width and range of
[VALUE-15](values.md#value-15), or `-` when unknown, and at least one field
known; it stays `str` until read, and a reader takes its prefix before the
first unknown field. `precision` returns a word of the ladder of
[VALUE-16](values.md#value-16): for text, the finest field that prefix
carries, at most `day` with `type: date`; for a value of the declared type,
its collected precision.

Fails: `incompatible_input_type`.
Evidence: `sdtm-ae-partial-dates`, `sdtm-dm-age`, `schema-birth-date`, `adam-adpc-relative-times`, `rust/crates/yamaa-adapters/tests/fixtures/temporal_imputation_reference.json`, `rust/crates/yamaa-core/tests/fixtures/collected_date_text.tsv`.

```text
collected := Y [ "-" M [ "-" D [ "T" h [ ":" m [ ":" s ] ] ] ] ]
Y := YYYY | "-"    M := MM | "-"    D := DD | "-"
h := hh | "-"      m := mm | "-"    s := ss | "-"
```

`2025` is `year`, `2025-03` is `month`, `2025-03-04` is `day`,
`2025-03-04T08` is `hour`, `2025-03-04T08:30` is `minute` and
`2025-03-04T08:30:15` is `second`; with `type: date` the last three are `day`.
Any other text is not collected date text, including a zone, an offset, a
fractional second, hour 24, a leap second, a field of one digit, a space before
the time and text whose every field is unknown.

An unknown field is the SDTM placeholder for a component missing inside the
text. The prefix a reader takes ends before it, and every field after it is
not read, so `2025---15`, a year and a day in an unknown month, reads as
`2025`, collected precision `year`, and `2025-03-15T-:30` reads as
`2025-03-15`, `day`. A known field after an unknown one still has its width
and range; a day whose month is unknown lies from `01` to `31`, and to `29` in
a February of an unknown year. Text whose year is unknown, such as `--03-15`,
leaves a prefix carrying no field: `precision` gives `none`, `impute` gives
missing as below every minimum precision ([TIME-8](#time-8)), and `to_date`
counts it as partial ([TIME-2](#time-2)). An ISO 8601 interval or duration,
such as `2025-01-01/2025-01-10` or `P3D`, names no single date and is not
collected date text; [TIME-13](#time-13) gives it its own condition.

Reading collected text is not converting it. Text converted to a temporal
column, or ingested as one, follows [VALUE-15](values.md#value-15): there
`2025-03-04T08:30` names second zero with collected precision `second`, and
`2025-03` is not a `date`. A specification that must keep how much of a value
was collected keeps the text as `str` and reads it with `impute` or
`precision`. A `datetime` source to `precision` with `type: date`, and a `date`
source with `type: datetime`, are incompatible inputs ([TIME-1](#time-1)). A
missing source and invalid text follow [TIME-13](#time-13).

<a id="time-22"></a>

### TIME-22. date_text reads text in one of its declared date forms into the collected date text it denotes

`date_text` reads its `str` source, whole and as written, in the first
declared form whose shape it has, and returns the collected date text of
[TIME-7](#time-7) it denotes: `iso` returns collected date text unchanged,
`monyyyy` a year and month, and `ddmonyyyy`, `mdy_slash`, `dmy_slash` and
`yyyymmdd` a complete date, each field with the width and range of
[VALUE-15](values.md#value-15). A missing source gives missing, and text that
no declared form reads fails with `invalid_date_text` unless `invalid`
declares a literal substitute.

Fails: `incompatible_input_type`, `invalid_field_type`, `invalid_date_text`.
Evidence: `adam-adcm-free-text-start-date`, `negative-date-text-no-form`, `negative-date-text-both-slash-forms`, `rust/crates/yamaa-adapters/tests/fixtures/date_text_forms.json`, `rust/crates/yamaa-core/src/date_forms.rs::each_form_reads_its_own_shape_into_collected_date_text`, `rust/crates/yamaa-core/src/date_forms.rs::only_the_two_slash_orders_share_a_shape`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_date_text_lists_at_least_one_form`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_date_text_lists_each_form_once`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_date_text_lists_one_slash_order`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::date_text_fails_on_text_in_no_declared_form`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::date_text_refuses_a_source_that_is_not_text`.

| Form | Shape | Example | Result |
|---|---|---|---|
| `iso` | collected date text ([TIME-7](#time-7)) | `2001-03-04T08:30` | `2001-03-04T08:30` |
| `ddmonyyyy` | `DD`, a month abbreviation, `YYYY` | `04MAR2001` | `2001-03-04` |
| `mdy_slash` | `MM/DD/YYYY` | `03/04/2001` | `2001-03-04` |
| `dmy_slash` | `DD/MM/YYYY` | `03/04/2001` | `2001-04-03` |
| `yyyymmdd` | `YYYYMMDD` | `20010304` | `2001-03-04` |
| `monyyyy` | a month abbreviation, `YYYY` | `MAR2001` | `2001-03` |

`forms` is a list naming at least one form, each once. `mdy_slash` and
`dmy_slash` read the one shape `NN/NN/YYYY` with the month or the day first,
so a list names at most one of them, and the order of day and month is the
form it names. No other two forms share a shape, so the order of a list never
changes what it reads. An empty list, a form named again and both slash forms
fail with `invalid_field_type` where the specification is read, before any
input is read: an empty list and both slash forms at `forms`, and a form named
again at its entry. Each refusal reports the operation, a form named again as
`value`, and what the list must be as `expected`: `at least one date form`,
`each date form once`, or `mdy_slash or dmy_slash, not both`. A name outside
the six fails with `value_not_permitted`
([SCHEMA-12](reference/schema-language.md#schema-12)).

The source is read whole: no blank is trimmed and no other separator is read,
so `04MAR2001` after a blank, and `4 March 2001`, are in no form. A day and a
month are two digits and a year four, so `4MAR2001` and `04MAR01` are in no
form and no two-digit year is read; `yyyymmdd` reads eight digits and nothing
shorter. A month abbreviation is the first three letters of the month's
English name, `JAN` to `DEC`, in any letter case, so `04mar2001` reads as
`04MAR2001` does. A field outside its range in the form whose shape the text
has, such as `29FEB2001`, or `13/04/2001` with `mdy_slash`, makes the text
invalid.

`iso` returns its text unchanged, an unknown field or a time included, so
`impute`, `precision` and `to_date` read the result as they read collected
date text; an ISO 8601 interval or duration is not collected date text and is
in no form. `monyyyy` returns a month, so `MAR2001` gives `2001-03`, which
`impute` completes with its `day` rule ([TIME-8](#time-8)); every other form
returns a complete date.

Text in no declared form fails with `invalid_date_text` in the `impute` phase,
reporting the operation, the source and the value, unless `invalid` declares a
literal substitute; a declared handler is counted only when reached. A missing
source gives missing without invoking `invalid`, and nothing is counted. A
source of another type than `str` is an incompatible input under
[TIME-1](#time-1).

<a id="time-19"></a>

### TIME-19. dtc writes collected date parts as collected date text that stops before the first missing part

`dtc` writes the collected date text of [TIME-7](#time-7) from separate
`year`, `month`, `day`, `hour`, `minute` and `second` sources, of which only
`year` is required and each declared part needs every coarser one. A `str`
part holds its field's digits at the field's width, and an `int` part is
written zero-padded, within the range of [VALUE-15](values.md#value-15). The
text stops before the first missing part, so a missing day gives `YYYY-MM`
and a missing year gives missing. Nothing is counted.

Fails: `incompatible_input_type`, `missing_required_field`, `invalid_date_text`.
Evidence: `sdtm-ae-partial-dates`, `sdtm-mh-ongoing-conditions`, `negative-dtc-month-digits`, `rust/crates/yamaa-core/tests/temporal_date_text.rs`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/temporal.json::a_day_after_a_missing_month_is_never_looked_up`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/temporal.json::an_absence_answer_after_a_missing_month_is_not_counted`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/temporal.json::no_absence_is_reached_when_only_the_row_without_a_month_lacks_a_day_record`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/temporal.json::a_reached_day_with_two_lookup_records_fails_the_run`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/temporal.json::a_reached_day_that_is_not_two_digits_is_refused_with_its_part_and_row`.

The parts are read in order from `year`, and reading stops at the first
missing part: the parts after it are not read, and the result is the
right-truncated text, never a placeholder form. A part that is not read makes
no lookup, so it can neither fail the run nor count an absence. A year `2025`,
a missing month and a day `15` give `2025`; a year, month and day give
`2025-03-15`, and with an hour and minute `2025-03-15T08:30`. A truncated
result is the collected precision rather than a substitution, so no handler
fires and no count is reported for it.

An `int` part is written as its decimal digits padded with leading zeros to
the field's width: a month `3` writes `03` and a year `2025` writes `2025`. A
part the text reads that is not its field's digits at the field's width, such
as a month `3` or `MAR` held as text, or that lies outside its range, such as a
day `30` in February of the year and month read, fails with
`invalid_date_text` in the `impute` phase, reporting the operation, the part,
its source and its value, unless `invalid` declares a literal substitute; a
declared handler is counted only when reached. A part of another type is an
incompatible input under [TIME-1](#time-1).

A part declared without every coarser part, such as `day` without `month`,
could never be written, and fails with `missing_required_field` at the absent
part where the specification is read. The text is an assembly of collected
values, which [SUB-19](profiles/submission.md#sub-19) admits as `Collected`.

<a id="time-8"></a>

### TIME-8. impute completes collected text into a value of its declared type from declared month, day and time rules

`impute` returns a value of its declared `type`. A source value of that type is
returned unchanged. Text carrying every field of the type stands as collected,
and with `type: date` text carrying a complete date gives that date, whatever
time follows. Otherwise `month` and `day` supply the missing date fields and,
for a datetime, `time: first` or `last` the missing time fields. A source
missing a field that no declared rule supplies gives missing. The result keeps
the source's collected precision.

Fails: `incompatible_input_type`.
Evidence: `adam-adae-partial-dates`, `adam-adae-partial-times`, `adam-adsl-alive-date`, `schema-birth-date`, `schema-impute-text-with-time`, `schema-imputation-flags`.

`time: first` supplies hour `00`, minute `00` and second `00`, and `time: last`
supplies `23`, `59` and `59`: the earliest or latest civil moment the collected
fields admit. A `day` that names a position, `first` or `last`, is resolved
against the month the completed date lands in, after `month` is fixed, so
`last` is 28 or 29 in a February according to the year and 30 or 31
elsewhere, and the completed value is a real calendar date by construction.

Which sources `impute` completes follows from the rules it declares: with
`month` and `day` a year alone receives both; with `day` alone a year and
month receives the day and a year alone gives missing; with neither only the
time of a datetime is supplied, and any source coarser than a day gives
missing. A source a rule does not reach is neither a missing source nor
invalid text, so it invokes neither answer of [TIME-13](#time-13). Which rules
may be declared together is fixed by [TIME-10](#time-10).

With `type: date`, `month: 6` and `day: first`, `1975` completes to
`1975-06-01` with collected precision `year`, and `2025-03-04T08:30` gives
`2025-03-04` with `day`. With `type: datetime`, `month: 1`, `day: first` and
`time: first`, `2025` completes to `2025-01-01T00:00:00` with `year`,
`2025-03` to `2025-03-01T00:00:00` with `month`, and `2025-03-04T08` to
`2025-03-04T08:00:00` with `hour`; under `time: last` the last completes to
`2025-03-04T08:59:59`. Text carrying seconds, and a value of the declared type,
are returned unchanged.

The text completed is the prefix [TIME-7](#time-7) reads, so an unknown field
and every field after it are completed as if not collected: `2025---15`
completes as `2025` does, and with `type: datetime` and `time: first`,
`2025-03-15T-:30` completes to `2025-03-15T00:00:00` with `day`. Text whose
year is unknown gives missing, as a source coarser than the minimum does.

A source whose type is neither `str` nor the declared type is an incompatible
input under [TIME-1](#time-1).

The parameters apply in this order: a missing source, then invalid text, then
the minimum precision, then completion from `month`, `day` and `time`, then
the bounds of [TIME-9](#time-9). [VALUE-16](values.md#value-16) governs the
resulting value.

<a id="time-9"></a>

### TIME-9. Imputation bounds move only supplied fields, a bound outside the collected interval is ignored, and crossed bounds give not_after

`not_before` and `not_after`, values of the declared type, bound a completed
value and move only the fields imputation supplied, within the interval the
collected fields admit: `2025` admits its year, `2025-01` its month and
`2025-01-15T08` its hour. A missing bound, or one outside that interval, is
ignored. Otherwise the result is `min(max(completed, not_before), not_after)`,
so crossed bounds give `not_after`, and each crossing is counted. A source that
needed no completion is never moved, and bounds never make a result missing.

Evidence: `adam-adae-partial-dates`, `adam-adcm-end-date-bound`, `schema-impute-crossed-bounds`, `rust/crates/yamaa-core/tests/scalar_declarative_temporal.rs::datetime_bounds_move_only_supplied_time_fields`.

For example, `2025-03` completed to `2025-03-31` under a `not_after` of
`2025-03-12` gives `2025-03-12`. Under a `not_after` of `2025-02-10`, a day
the collected month does not admit, the bound is ignored and the result is
`2025-03-31`, never a day in February. With `not_before` `2025-03-20` and
`not_after` `2025-03-10`, both inside March, the bounds cross and the result is
`2025-03-10`.

A crossing is counted at the operation's `not_after` path, zero included, as
`crossed_bounds`, like a handler count under
[COLUMN-14](columns.md#column-14). A moved result keeps the source's collected
precision, so [TIME-18](#time-18) still reports what imputation supplied. A
fallback value ([TIME-13](#time-13)) is whole and the bounds do not move it.
Bounds are read only when a source needs completion. A specification that
constrains collected values states a verification under
[Verification](verification.md).

<a id="time-10"></a>

### TIME-10. impute's month and day rules must fit each other and the calendar

A declared `month` requires a declared `day`. A declared `month` must lie from
1 to 12 and a numeric `day` from 1 to 31, and the completed value must be a
real calendar date. A `day` that names a position in its month is not a
literal to range-check. Any other `day` token is neither a number nor a
position and is rejected where the specification is read, before any data is
seen.

Fails: `missing_required_field`, `month_out_of_range`, `value_not_permitted`, `invalid_calendar_date`.
Evidence: `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/one_spelling.json::an_impute_with_a_month_rule_and_a_day_rule_is_admitted`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/one_spelling.json::an_impute_with_a_day_rule_alone_is_admitted`, `rust/crates/yamaa-adapters/tests/fixtures/diagnostics/one_spelling.json::an_impute_with_neither_rule_is_admitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::an_impute_month_rule_needs_a_day_rule`, `negative-impute-bad-month`, `negative-impute-bad-day`, `negative-impute-day-rule`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::an_impute_month_above_twelve_is_out_of_range`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::an_impute_day_number_above_thirty_one_is_not_permitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::an_impute_day_position_other_than_first_or_last_is_not_permitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::an_impute_that_completes_a_day_past_the_month_end_fails`.

A `month` without `day` fails as `missing_required_field` at the `impute`,
naming `day`: a source missing its month also misses its day, so the `month`
could never complete a date. A `month` outside the calendar range fails with
`month_out_of_range` and a numeric `day` outside it with
`value_not_permitted`. A completed value that is not a real calendar date, such
as a numeric `day` of 31 in April, fails with `invalid_calendar_date` in the
`impute` phase. A `day` token that is neither a number nor a declared position
fails with `value_not_permitted`. The `time` rule follows the declared type
([TIME-1](#time-1)).

<a id="time-13"></a>

### TIME-13. A missing source gives missing unless impute declares a fallback, and invalid text fails unless invalid answers it

`impute` and `precision` give a missing result for a missing source, counted as
a default substitution at the operation's `missing` path. `impute`'s optional
`fallback`, a column of the declared type, answers a missing source instead:
the result is its value with collected precision `none`, counted at
`fallback`, and missing when the fallback is missing. Text that is not
collected date text ([TIME-7](#time-7)) fails with `interval_or_duration_text`
when it is an ISO 8601 interval or duration and with `invalid_date_text`
otherwise, unless `invalid` declares a literal substitute.

Fails: `invalid_date_text`, `interval_or_duration_text`.
Evidence: `schema-impute-fallback`, `negative-impute-bad-source`, `negative-precision-bad-source`, `negative-impute-datetime-text`, `negative-precision-datetime-text`, `negative-impute-interval`, `rust/crates/yamaa-core/tests/fixtures/collected_date_text.tsv`.

Both counts are reported like handler counts under
[COLUMN-14](columns.md#column-14), zero included. The fallback is read only
when the source is missing, never for a source below the minimum precision or
invalid text. A source that is a temporal value is never invalid. The phase of
the invalid-text failure is `impute` ([COLUMN-16](columns.md#column-16)),
and that one phase serves both operations; `invalid` fires only when the
source is present ([COLUMN-11](columns.md#column-11)). A fallback value is
whole: the bounds of [TIME-9](#time-9) do not move it, and
[TIME-18](#time-18) reports it as wholly supplied.

An ISO 8601 duration is `P`, optionally after `-`, followed by components
that are each a number and a designator: `nW` alone, or `nY`, `nM` and `nD`
and, after `T`, `nH`, `nM` and `nS`, each at most once and in that order, at
least one, the last alone with a decimal fraction. An interval is two parts
joined by one `/`, each a right-truncated date and time, which may carry a
fraction of a second and a zone, or a duration, not both durations. `P3D`,
`-PT15M` and `2025-01-01/2025-01-10` name a length of time or a period, not one
date, so no reader completes or measures them, and the failure names that
cause; `2025/03/04` is neither and fails as invalid date text. One `invalid`
handler answers both conditions, and the failure reports the operation, the
source and the value.

## Civil arithmetic

<a id="time-14"></a>

### TIME-14. shift moves a date by whole days, or a datetime by a signed elapsed amount rounded to a whole second

With `type: date`, `shift` adds `days`, a signed whole-day count, to a complete
date on the proleptic Gregorian calendar, crossing month, year and leap-day
boundaries without clamping. With `type: datetime`, it adds a signed `amount`
in an elapsed `unit` to a civil datetime: `second` requires an integer, and a
float amount is scaled to seconds as binary64 and rounded to whole seconds
with the one rounding of [FORMULA-16](formulas.md#formula-16) before it is
added exactly. The result keeps the source's collected precision.

Fails: `incompatible_input_type`, `conversion_failed`.
Evidence: `adam-adsl-period-dates`, `adam-adae-post-dose-window`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::date_shift_refuses_a_source_that_is_not_a_date`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::date_shift_fails_past_year_9999`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::a_datetime_shift_in_seconds_refuses_a_float_amount`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::a_datetime_shift_fails_past_year_9999`, `rust/tests/host-fixtures/cases.json`.

`days` is a signed integer literal or integer variable; a negative count moves
backward. `minute`, `hour` and `day` admit integer or float amounts; integer
amounts are scaled exactly and float amounts are multiplied by 60, 3600 or
86400 as binary64. The rounding applies to the amount, so the source second
does not affect it: `0.025` minute is exactly `1.5` seconds and adds `2`
seconds. A day is 86400 seconds, and no timezone or daylight-saving
adjustment occurs.

A missing source, day count or amount returns missing. A source or count of
another type, or a non-integer day count, fails validation as
`incompatible_input_type`. A result outside years 0001 through 9999 fails at
build as `conversion_failed` in the `convert` phase; no local handler answers
the arithmetic overflow.

<a id="time-17"></a>

### TIME-17. duration_value reads ISO 8601 fixed-length duration text as a number in a declared unit

`duration_value` reads ISO 8601 fixed-length duration text `[-]PnW` or
`[-]P[nD][T[nH][nM][nS]]` into a `float` in the declared elapsed unit. At least
one component is required, components appear once in the stated order, `T`
requires a time component, and weeks cannot combine with other components.
Only the last written component may carry a decimal fraction, with a dot or a
comma. Whitespace, a leading plus sign, component signs, exponent notation,
years and calendar months are invalid.

Fails: `incompatible_input_type`, `invalid_duration_text`.
Evidence: `adam-adex-infusion-duration`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::duration_value_refuses_a_source_that_is_not_text`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::duration_value_fails_on_a_calendar_month`, `rust/crates/yamaa-core/tests/scalar_sqrt.rs::duration_value_reads_signed_fractional_and_week_forms_exactly`, `rust/tests/host-fixtures/cases.json`.

`PT1M` is one minute; `P1M` is a calendar month and is invalid. A week is seven
days and a day is 86400 seconds. A missing source returns missing without
invoking `invalid`. A non-text source fails validation as
`incompatible_input_type`.

Decimal components are combined exactly before conversion. In every unit,
whole-second durations included, the exact seconds are converted once to
binary64 and divided by 1, 60, 3600 or 86400 under [Formulas](formulas.md), so
the result is a `float` ([EXPR-14](expressions.md#expr-14)). Whole seconds
beyond `2^53` take the nearest binary64 value: `PT9007199254740993S` in
`second` is `9007199254740992.0`, and an `int` column converts it to
`9007199254740992`. `PT9223372036854775807S` rounds to `2^63`, beyond the
signed integer range, so an `int` column fails to convert it as
`conversion_failed` ([VALUE-7](values.md#value-7)). Text whose result is not
finite is invalid.

Invalid text returns the value the `invalid` handler declares, and
`invalid: null` gives missing; without the handler, the condition
`invalid_duration_text` is fatal in the `impute` phase. A declared handler is
counted only when reached.

<a id="time-21"></a>

### TIME-21. duration_text writes a number in a declared unit as canonical ISO 8601 fixed-length duration text

`duration_text` writes an `int` or `float` amount of the declared elapsed unit
as one canonical text: `P`, then days, then `T` and hours, minutes and seconds,
each zero component omitted, with a fraction only on the seconds. Zero is
`PT0S`, a negative amount takes one leading `-`, and weeks are never written.
An `int` is scaled to seconds exactly; a `float` is scaled once as binary64,
and that value's canonical text ([VALUE-8](values.md#value-8)) is split
exactly.

Fails: `incompatible_input_type`, `conversion_failed`.
Evidence: `sdtm-ex-infusion-duration`, `negative-duration-text-non-numeric-source`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::duration_text_refuses_a_source_that_is_not_numeric`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/temporal.json::duration_text_fails_when_its_seconds_leave_the_float_range`, `rust/crates/yamaa-core/tests/scalar_sqrt.rs::duration_text_writes_one_canonical_text_for_each_amount`, `rust/crates/yamaa-core/tests/scalar_sqrt.rs::duration_text_read_back_returns_every_whole_second_amount`, `rust/tests/host-fixtures/cases.json`.

`120` minutes is `PT2H`, `90` minutes `PT1H30M`, `26` hours `P1DT2H`, `-0.25`
hours `-PT15M`, `90.5` seconds `PT1M30.5S` and `0.5` seconds `PT0.5S`. A day
is 86400 seconds and the largest unit written, so 26 hours is never `PT26H`,
and 14 days is `P14D`, never the `P2W` that `duration_value` also reads, so
that one amount has one text.

An `int` amount times 1, 60, 3600 or 86400 is exact, so `9223372036854775807`
days is `P9223372036854775807D`. A `float` amount is multiplied by 60, 3600 or
86400 as binary64, as `shift` scales its amount ([TIME-14](#time-14)), and the
shortest digits of the product are the exact decimal the components split:
`0.1` minute is `PT6S`, and the `float` nearest one third of an hour,
`0.3333333333333333`, is `PT20M`. Negative zero is zero. A `float` whose
product leaves the binary64 range, such as `1e304` days, has no text: it fails
at build as `conversion_failed` in the `convert` phase, with `to` naming `str`,
and no local handler answers it.

Reading the text back with `duration_value` ([TIME-17](#time-17)) in the same
unit returns the amount whenever it is a whole number of seconds no larger
than `2^53` in magnitude: those seconds are exact in binary64, and dividing
them by the unit's seconds gives the amount back. A missing amount gives
missing. An amount of another type fails validation as
`incompatible_input_type`, and `duration_text` declares no handler.

## Imputation flags

<a id="time-18"></a>

### TIME-18. imputation_flag returns the ADaM date or time imputation flag from an imputed value's collected precision

`imputation_flag` reads a `date` or `datetime` and returns the first part
imputation supplied, from its collected precision. With `part: date`, `none`
gives `Y`, `year` gives `M`, `month` gives `D`, and `day` or finer gives
missing. With `part: time`, which reads a `datetime`, `none` through `day` give
`H`, `hour` gives `M`, `minute` gives `S` unless `seconds_collected` is false,
and `second` gives missing. A missing source gives missing.

Fails: `incompatible_input_type`.
Evidence: `schema-imputation-flags`, `schema-impute-fallback`, `adam-adae-partial-times`, `adam-adcm-end-date-bound`, `negative-imputation-flag-text`.

The codes are the ADaM `*DTF` and `*TMF` values. `seconds_collected`, default
true, states whether the study collects seconds; when it is false, a time
collected to the minute had nothing imputed and gives missing. A bound that
moved an imputed day ([TIME-9](#time-9)) keeps the flag `D`, and a fallback
value ([TIME-13](#time-13)) gives `Y` and `H`. Collected text is not a flag
source: a `str` source fails as an incompatible input where the specification
is read, and a specification flags the value `impute` completed. Collected
precision does not survive an artifact ([VALUE-17](values.md#value-17)), so
the flag reads a value derived in the same specification.
