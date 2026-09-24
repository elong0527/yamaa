---
id: values/temporal
title: Temporal values
status: normative
---

# Temporal values

## Purpose

Define dates, local civil datetimes, precision, canonical text, and order.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Local handlers](../execution/handlers.md).
- [Verification](../execution/verification.md).
- [Aggregation](../operations/aggregation.md).
- [Numeric computation](../operations/computation.md).
- [Expression evaluation](../operations/expressions.md).
- [Project functions](../operations/functions.md).
- [Execution lifecycle](../execution/lifecycle.md).
- [Source ingestion](../storage/ingestion.md).
- [Types and conversion](types.md).

## Requirements

### The two values

<a id="req-0539"></a>

**REQ-0539.** A `date` is a complete proleptic Gregorian calendar date naming
one day.

<a id="req-0540"></a>

**REQ-0540.** A `datetime` is a complete local civil datetime: a date of the same
kind together with a time of day resolved to a whole second. It names a reading
on a wall clock. It is not an instant on a timeline. It carries no zone
and no offset.

<a id="req-0541"></a>

**REQ-0541.** Their fields and ranges are exactly:

| Field | Range | In `date` | In `datetime` |
|---|---|---|---|
| year | `0001` to `9999` | yes | yes |
| month | `01` to `12` | yes | yes |
| day | `01` to the length of that month in that year | yes | yes |
| hour | `00` to `23` | -- | yes |
| minute | `00` to `59` | -- | yes |
| second | `00` to `59` | -- | yes |

<a id="req-0542"></a>

**REQ-0542.** Like every column type, both additionally admit the missing value.

<a id="req-0543"></a>

**REQ-0543.** Every combination of fields in range names one day or one civil
moment, and every day or civil moment in range has one such combination.
Both value spaces are total and gapless.

<a id="req-0544"></a>

**REQ-0544.** Every value is complete and records how much was collected. A value
is complete or is not a value of the type. A truncated collected value stays
text until `date_impute` or `datetime_impute` completes it. Each value has a
**collected precision**: the finest field its collected source supplied.

<a id="req-0545"></a>

**REQ-0545.** For a `date` that property is `year`, `month`, or `day`. For a
`datetime` it is `day` or `second`. A parsed datetime is collected through the
second: an omitted `ss` names second zero rather than claiming a coarser value.
`datetime_impute` may instead complete a collected date with a declared time
of day, producing a complete datetime whose collected precision remains `day`.

<a id="req-0546"></a>

**REQ-0546.** Where a value gets its collected precision is fixed by where the
value came from. Only five origins exist:

| Origin | Collected precision |
|---|---|
| parsed text | `day`; `datetime`: `second` |
| `date_impute` | the precision its source carried |
| `datetime_impute` | `day` for a date source; `second` for a datetime source |
| `to_date` | `day` |
| selecting an existing value | unchanged; property follows selected value |

<a id="req-0547"></a>

**REQ-0547.** The parsed-text row admits no other answer: the grammar below
admits only complete text, so nothing a parse produces was collected in part.
[Expression evaluation](../operations/expressions.md) fixes the fourth, for the extreme, conditional, coalescing, and offset
expressions that return an operand rather than computing one.

<a id="req-0548"></a>

**REQ-0548.** Provenance is read off collected precision rather than recorded
beside it. A component finer than the collected precision was supplied by an
imputation operation. A date whose precision is `day` was collected in full;
a datetime whose precision is `day` had its time supplied, while `second`
means the source carried a time. A second property would be a second place to
keep correct, and the two could disagree.

### Lexical form

<a id="req-0549"></a>

**REQ-0549.** Text becomes a temporal value in exactly one shape each:

    date     := YYYY "-" MM "-" DD
    datetime := date "T" time
    time     := hh ":" mm [ ":" ss ]

<a id="req-0550"></a>

**REQ-0550.** `YYYY` is four ASCII digits and `MM`, `DD`, `hh`, `mm`, and `ss`
are two each, zero-padded to that width. One production defines the calendar
half of both types, so a date parses identically wherever it appears.

<a id="req-0551"></a>

**REQ-0551.** An omitted `ss` names second `00`: the only omission either
form permits. Nothing else is defaulted, no sign or surrounding
whitespace is accepted, and no other separator or field order is recognised.

<a id="req-0552"></a>

**REQ-0552.** Rejecting everything else lets two implementations agree.
Each runtime's own parser accepts a wider and a different set: a space
separator, lowercase `t`, bare date read as a moment, and trailing `Z` are
each read by one of them and not the other, so a rule admitting whatever a
runtime happened to accept would not be portable. These cases are the cost
of that rejection:

| Rejected | Offered as | Why |
|---|---|---|
| `2025-01` | `date` | truncated; a partial date is not a date |
| `20250112` | `date` | basic format; the extended form only |
| `2025-1-2` | `date` | fields not padded to their width |
| `2025-02-30` | `date` | not a date in the calendar |
| `2025-01-12T14:00:00` | `date` | a time of day; a moment is not a day |
| `2025-01-12` | `datetime` | no time of day; a day is not a moment |
| `14:00:00` | `datetime` | no date; a time of day alone is not a value |
| `2025-01-12 14:00:00` | `datetime` | a space; the separator is `T` |
| `2025-01-12t14:00:00` | `datetime` | lowercase `t`; separator uppercase |
| `2025-01-12T14:00:00Z` | `datetime` | a zone designator |
| `2025-01-12T14:00:00+02:00` | `datetime` | an offset |
| `2025-01-12T14:00:00.5` | `datetime` | a fractional second |
| `2025-01-12T24:00` | `datetime` | hour 24; midnight is next day's `00:00` |
| `2025-01-12T23:59:60` | `datetime` | a leap second |

<a id="req-0553"></a>

**REQ-0553.** `24:00` and `23:59:60` are rejected for the same reason as the rows above, and not only
for being unusual. `2025-01-12T24:00` names the moment
`2025-01-13T00:00` already names, and the two spellings disagree about the day,
so admitting the first would leave the carried date dependent on the
spelling that arrived. A leap second is not a value either runtime holds.
Neither R's nor Python's representation has a sixty-first second, so none
can be stored.

<a id="req-0554"></a>

**REQ-0554.** `14:00:00` is rejected for a different reason from the rest of the
table. Every other row spells a value one of these two types holds; a clock
reading carrying no date is not one. Both types name a position on the
calendar. A study that collects one, as the `--TM` family does, keeps the
collected text as `str`. A time-only value would be a third temporal type, not
a wider `datetime` -- a new [Types and conversion](types.md) vocabulary entry.
The type enters when an example needs a time of day with no date.

### No zone, no offset

<a id="req-0555"></a>

**REQ-0555.** A `datetime` carries no timezone and no offset, and text carrying
either is rejected rather than normalized.

<a id="req-0556"></a>

**REQ-0556.** Admitting both a local and an offset-aware value would put two
kinds of datetime in one column type, and the target runtimes disagree about
that pair. Python refuses to order a naive datetime against an aware one and
raises instead. R has no naive datetime at all: a `POSIXct` always carries a
`tzone`, and an empty one resolves against the machine's timezone, so the same
specification would order the same column differently on two machines. Neither
behavior is this design's to choose. Each is a property of that
runtime's type.

<a id="req-0557"></a>

**REQ-0557.** Prohibiting the zone removes the disagreement rather than
arbitrating, and costs studies nothing they collect: a CDISC `--DTC` value is
local site time and carries no offset. A study that records an offset keeps
the offset in its own column, where a specification can read the offset as
data, and an instant-typed value can be added later without invalidating any
specification written under this contract.

<a id="req-0559"></a>

**REQ-0559.** No civil time is nonexistent or ambiguous. A daylight-saving gap
  or repetition arises only when a wall-clock reading is mapped to a timeline.
  A zone supplies that mapping. Without a zone, every field combination
  in the ranges above is a value of the type and denotes the reading it spells,
  so there is nothing to reject as unrepresentable and nothing to disambiguate.

<a id="req-0560"></a>

**REQ-0560.** A datetime is never shifted. Nothing normalizes it into another
zone, so the held value carries the fields of the parsed text.

### Whole seconds

<a id="req-0561"></a>

**REQ-0561.** A `datetime` resolves to a whole second, and text carrying a
fractional second is rejected.

<a id="req-0562"></a>

**REQ-0562.** The runtimes cannot agree on a fraction. Python's
`datetime.datetime` records whole microseconds as integers. R's `POSIXct` is a
binary64 count of seconds, which represents most fractions only approximately
and prints them under its own rounding. Admitting one would mean two
implementations that store, compare, and render the same collected value
differently, and the design requires them to agree.

<a id="req-0563"></a>

**REQ-0563.** Whole seconds are exact in both, which fixes the
representation:

| Runtime | `date` | `datetime` |
|---|---|---|
| Python | `datetime.date` | `datetime`; tzinfo unset; microsecond zero |
| R | `Date` | `POSIXct` with `tzone` set to `"UTC"` |

<a id="req-0564"></a>

**REQ-0564.** Integral seconds below 2^53 are exact in R's binary64
representation, which covers all years above. R's `tzone` is a carrier, not a
value claim. `UTC` has no offset or daylight-saving rule. It cannot shift a
value or make one ambiguous, and it keeps the machine's timezone out of the
result. An implementation must set it rather than leave it empty.

<a id="req-0565"></a>

**REQ-0565.** A study that collects sub-second times keeps the collected text in
a `str` column until a rule fixes a representation both runtimes share.

### Canonical text

<a id="req-0566"></a>

**REQ-0566.** A temporal value is written back in exactly one form:

| Type | Canonical text |
|---|---|
| `date` | `YYYY-MM-DD` |
| `datetime` | `YYYY-MM-DDThh:mm:ss` |

<a id="req-0567"></a>

**REQ-0567.** Every field is zero-padded to its width, and a `datetime`
always shows seconds. This is the text a temporal value converts to under
[Types and conversion](types.md)'s `str` row and the text the artifact records for a temporal column, so a
`str` column derived from a temporal value and the artifact's rendering of
that value never disagree. [Types and conversion](types.md) fixes the same relationship for `float`.

<a id="req-0568"></a>

**REQ-0568.** Canonical text is not the collected text. A value parsed from
`2025-01-12T14:00` renders as `2025-01-12T14:00:00`. The value names
second zero. Like `float`, a declared type stores a value rather than the
received characters. For example, `1.50` renders as `1.5`. A column
that must keep collected characters unchanged is `str`. A `str` column
uses scalar order under [Text values](text.md). Canonical fixed-width text
for values of one temporal type has the same order as the represented fields.

<a id="req-0569"></a>

**REQ-0569.** Unlike `float`, neither form takes a project setting. This contract
fixes rendered precision at one day and one second.

<a id="req-0570"></a>

**REQ-0570.** Canonical text carries fields alone, so its collected precision is
not observable outside the derivation. A temporal value converted to `str`
under [Types and conversion](types.md)'s row, the artifact's record of a temporal column, and the typed
value [Project functions](../operations/functions.md) encodes for a function argument all carry the day or the moment and
nothing about how much of it was collected. This is deliberate: the property
answers a question about a study's collection, and a reader holding only the
text has no way to check an answer to it. A specification that must carry
precision past any of those three boundaries derives a column from
`date_precision` or `datetime_precision`, which is data the artifact records
like any other.

### Comparison and ordering

<a id="req-0571"></a>

**REQ-0571.** Two values of the same temporal type compare field by field, most
significant first: year, then month, then day; a `datetime` then uses hour,
minute, and second. Every pair of non-missing values of one type is ordered,
and that order is chronological. No value carries a zone, so a comparison
cannot be between a civil time and an instant. No datetime comparison fails.

<a id="req-0572"></a>

**REQ-0572.** Both are comparable types wherever a rule requires mutually
comparable values. `greatest` and `least` reduce them across a row, an
`order_by` term orders by one, and [Aggregation](../operations/aggregation.md)'s `MIN` and `MAX` reduce one; [Ordering](../execution/ordering.md)
places missing values by the term's `nulls`, as it does for every other type.

<a id="req-0573"></a>

**REQ-0573.** Collected precision takes no part in a comparison. Two values
compare by the fields above; precision is not one of them. A value completed
from a year and a month therefore orders against a fully collected one on the
day it names, wins a `greatest` it is the latest operand of, and satisfies a
predicate the day satisfies. Every pair of non-missing values of one type stays
ordered, which is what keeps an `order_by` term total and [Types and conversion](types.md)'s comparability
argument intact.

<a id="req-0574"></a>

**REQ-0574.** This is a decision and not an omission, and the imputed value
itself forces that decision. A completed date names a day: that is what
completing it did. An imputed operand that lost a comparison would have to
denote something else: the interval its collected components still admit, or a
day carrying a rank against collected dates -- and either is a different value
space with its own ordering, its own canonical text, and its own conversions.
That is a type this design does not have, not a property of the two it does.

<a id="req-0575"></a>

**REQ-0575.** The cost is worth stating plainly: it is the case the
property was added for: an imputed start still decides whether an event is
treatment emergent, and precision does not stop it. What precision changes is
that the specification classifying the event can now see that the day was
supplied. The specification can record that fact in the artifact. A
specification that needs a supplied day not to reach a classification
bounds the imputation with `not_before`, or states a verification under
[Verification](../execution/verification.md). Neither is a comparison, so neither belongs in this section.

### Conversion

<a id="req-0576"></a>

**REQ-0576.** A `date` and a `datetime` do not convert in either direction.

<a id="req-0577"></a>

**REQ-0577.** A temporal value never enters arithmetic. [Numeric computation](../operations/computation.md)'s grammar is
numeric, and a difference between two values is `date_diff` or `study_day`
below.

### Ingestion

<a id="req-0599"></a>

**REQ-0599.** Every place text becomes a temporal value uses this text form.
[Source ingestion](../storage/ingestion.md) applies [Types and conversion](types.md)'s `str` row, which reaches the grammar above. A field
declared `date` or `datetime` in a `types` declaration parses
exactly as a column conversion parses.

<a id="req-0600"></a>

**REQ-0600.** The two paths differ only in what answers a bad value, which [Source ingestion](../storage/ingestion.md)
fixes rather than this contract: an ingested value that does not parse is rejected
before any derivation, so no handler applies. A `str` field converted
at the column declaring a temporal type fails there, where
`missing` can answer. A specification that wants to see a malformed
value therefore leaves the field `str`, which is what `negative-datetime-zone-
offset` does.

## Error conditions

<a id="req-0601"></a>

**REQ-0601.** Text that is not the lexical form above: not a temporal value. For
a `date` this includes a truncated date, a date carrying a time of day, and the
basic format; for a `datetime` it additionally includes a zone designator, an
offset, a fractional second, hour 24, and a leap second. Reaching a temporal
column, it is the conversion failure [Types and conversion](types.md) defines, handled by
`missing` under [Local handlers](../execution/handlers.md) and otherwise fatal under [Execution lifecycle](../execution/lifecycle.md).

<a id="req-0602"></a>

**REQ-0602.** A
date part that is not a date in the calendar, such as `2025-02-30`: the same
failure.

<a id="req-0603"></a>

**REQ-0603.** A year outside `0001` to `9999`: the same failure. The
four-digit field admits no other year, and that range is also the one Python's
`datetime` holds.

<a id="req-0604"></a>

**REQ-0604.** A conversion the table above marks `fail`,
including `date` to `datetime` and `datetime` to `date`: fail; choose none.

<a id="req-0605"></a>

**REQ-0605.** Comparing or ordering a temporal value against another type:
fail under [REQ-0005](types.md#req-0005), reported as the [REQ-0323](types.md#req-0323) incompatible-input error.

<a id="req-0612"></a>

**REQ-0612.** Storing a value no implementation can hold exactly,
such as a fractional or leap second, is never reached. Text is rejected
first. An implementation must not round to reach such a value.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [adam-adae-partial-dates](../../benchmarks/adam-adae-partial-dates/README.md).
- [adam-adsl-treatment](../../benchmarks/adam-adsl-treatment/README.md).
- [negative-date-incomplete](../../benchmarks/negative-date-incomplete/README.md).
- [negative-datetime-zones](../../benchmarks/negative-datetime-zones/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Define dates, local civil datetimes, precision, canonical text, and order. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
