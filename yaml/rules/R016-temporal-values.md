---
id: R016
title: Temporal Values
status: normative
applies_to:
  - column_type
  - column.type
  - derivation
  - date_diff
  - study_day
  - date_impute
  - date_precision
depends_on: [R005, R006, R007, R008, R010, R011, R014]
---

# Temporal values

## Intent

Define the two temporal values this design admits, `date` and `datetime`: what
each denotes, the text it is read from and written back to, how two of them
order, which operations read them, and what fails. R and Python must produce
the same value, the same order, the same output text, and the same failures
from the same input.

## Boundaries

**This rule owns both temporal types completely.** No other rule states what a
date or a datetime is, which text becomes one, or what may be done with one.

R011 owns the `column_type` vocabulary that admits both and the shape of the
conversion table; its temporal cells name this rule for the grammar and the
canonical text they apply, exactly as its numeric cells name R010 for the
`number` production. R007 owns registration, nesting, and evaluation order for
every expression, and defers the input and result types of the temporal
operations to this rule, as it defers `aggregate` to R013. R014 owns which
stored fields are missing and reaches this grammar through R011's `str` row.
R008 owns the handler lifecycle whose `missing` and `invalid` fields
`date_impute` and `date_precision` declare. R005 owns when conversion happens
and R010 owns arithmetic, which no temporal value enters.

Each operation's own parameters are documented beside its registration in
`schema_expression_date.yaml`, which is authoritative for operation-local
behavior. This rule states what is shared: the types those parameters carry
and the value they produce.

R004 owns the predicate grammar and is draft. A predicate comparing two
temporal values orders them as this rule defines; what else a predicate may
write is R004's to close.

## The two values

A **`date`** is a complete proleptic Gregorian calendar date. It names a day.

A **`datetime`** is a complete local civil datetime: a date of the same kind
together with a time of day resolved to a whole second. It names a reading on
a wall clock. It is not an instant on a timeline, because it carries no zone
and no offset.

Their fields and ranges are exactly:

| Field | Range | In `date` | In `datetime` |
|---|---|---|---|
| year | `0001` to `9999` | yes | yes |
| month | `01` to `12` | yes | yes |
| day | `01` to the length of that month in that year | yes | yes |
| hour | `00` to `23` | -- | yes |
| minute | `00` to `59` | -- | yes |
| second | `00` to `59` | -- | yes |

Like every column type, both additionally admit the missing value.

Every combination of fields in range names one day or one civil moment, and
every day or civil moment in range has one combination of fields. Both value
spaces are total and gapless.

**Neither carries precision.** A `date` has no month or year precision and a
`datetime` has no day or minute precision. A value is complete or it is not a
value of the type, so a truncated collected value stays text until something
completes it; *Partial collected dates* below defines the one completion this
design offers.

## Lexical form

Text becomes a temporal value in exactly one shape each:

    date     := YYYY "-" MM "-" DD
    datetime := date "T" time
    time     := hh ":" mm [ ":" ss ]

`YYYY` is four ASCII digits and `MM`, `DD`, `hh`, `mm`, and `ss` are two each,
zero-padded to that width. One production defines the calendar half of both
types, so a date parses identically wherever it appears.

An omitted `ss` names second `00`, and it is the only omission either form
permits. Nothing else is defaulted, no sign or surrounding whitespace is
accepted, and no other separator or field order is recognised.

Rejecting everything else is what makes two implementations agree. Each
runtime's own parser accepts a wider and a different set: a space separator, a
lowercase `t`, a bare date read as a moment, and a trailing `Z` are each read
by one of them and not the other, so a rule admitting whatever a runtime
happened to accept would not be portable. These are the cases that decision
costs:

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
| `2025-01-12t14:00:00` | `datetime` | a lowercase `t`; the separator is uppercase |
| `2025-01-12T14:00:00Z` | `datetime` | a zone designator |
| `2025-01-12T14:00:00+02:00` | `datetime` | an offset |
| `2025-01-12T14:00:00.5` | `datetime` | a fractional second |
| `2025-01-12T24:00` | `datetime` | hour 24; midnight is `00:00` of the following day |
| `2025-01-12T23:59:60` | `datetime` | a leap second |

`24:00` and `23:59:60` are rejected for the same reason as the rest and not
only because they are unusual. `2025-01-12T24:00` names the moment
`2025-01-13T00:00` already names, and the two spellings disagree about the
day, so admitting the first would leave the date a value carries depending on
which spelling arrived. A leap second is not a value either runtime holds:
neither R's nor Python's representation has a sixty-first second, so nothing
could be stored.

## No zone, no offset

A `datetime` carries no timezone and no offset, and text carrying either is
rejected rather than normalized.

Admitting both a local and an offset-aware value would put two kinds of
datetime in one column type, and the two target runtimes disagree about that
pair. Python refuses to order a naive datetime against an aware one and raises
instead. R has no naive datetime at all: a `POSIXct` always carries a `tzone`,
and an empty one resolves against the machine's timezone, so the same
specification would order the same column differently on two machines. Neither
behavior is this design's to choose, because each is a property of that
runtime's type.

Prohibiting the zone removes the disagreement rather than arbitrating it, and
it costs a study nothing it collects: a CDISC `--DTC` value is local site time
and carries no offset. A study that does record one keeps it in its own
column, where it is data a specification can read, and an instant-typed value
can be added later without invalidating any specification written under this
rule.

Two consequences are worth stating, because both are failure modes this type
does not have:

- **No civil time is nonexistent or ambiguous.** A daylight-saving gap or
  repetition arises only in mapping a wall-clock reading onto a timeline, and
  that mapping is what a zone supplies. Without one, every combination of
  fields in the ranges above is a value of the type and denotes the reading it
  spells, so there is nothing to reject as unrepresentable and nothing to
  disambiguate.
- **A datetime is never shifted.** Nothing normalizes it into another zone, so
  the value an implementation holds carries the fields the text it parsed
  carried.

## Whole seconds

A `datetime` resolves to a whole second, and text carrying a fractional second
is rejected.

The runtimes cannot agree on a fraction. Python's `datetime.datetime` records
whole microseconds as integers. R's `POSIXct` is a binary64 count of seconds,
which represents most fractions only approximately and prints them under its
own rounding. Admitting one would mean two implementations that store,
compare, and render the same collected value differently, and the design
requires them to agree.

Whole seconds are exact in both, which is what fixes the representation:

| Runtime | `date` | `datetime` |
|---|---|---|
| Python | `datetime.date` | `datetime.datetime`, `tzinfo` unset, `microsecond` zero |
| R | `Date` | `POSIXct` with `tzone` set to `"UTC"` |

Integral seconds below 2^53 are exact in R's binary64 representation, which
covers the whole of the year range above. R's `tzone` is a carrier and not a
claim about the value: `"UTC"` is chosen because it is the one zone with no
offset and no daylight-saving rule, so it cannot shift a value or make one
ambiguous, and it keeps the machine's timezone out of the result. An
implementation must set it rather than leave it empty.

A study that collects sub-second times keeps the collected text in a `str`
column until a rule fixes a representation both runtimes share.

## Canonical text

A temporal value is written back in exactly one form:

| Type | Canonical text |
|---|---|
| `date` | `YYYY-MM-DD` |
| `datetime` | `YYYY-MM-DDThh:mm:ss` |

with every field zero-padded to its width and, for a `datetime`, the seconds
always present. This is the text a temporal value converts to under R011's
`str` row and the text the artifact records for a temporal column, so a `str`
column derived from one and the artifact's own rendering of that same value
never disagree. R011 fixes the same relationship for `float`.

**Canonical text is not the collected text.** A value parsed from
`2025-01-12T14:00` renders as `2025-01-12T14:00:00`, because what is rendered
is the value and the value names second zero. This is the rule R011 already
states for `float`, where `1.50` renders as `1.5`: a declared type stores a
value rather than the characters it arrived as. A variable that must carry the
collected characters unchanged is `str`, which keeps them and still orders
chronologically under R007.

Unlike `float`, neither form takes a project setting. This rule fixes the
precision at one day and one second, so a project has nothing left to declare.

## Comparison and ordering

Two values of the same temporal type compare field by field, most significant
first: year, then month, day, and for a `datetime` hour, minute, and second.
Every pair of non-missing values of one type is therefore ordered, and that
order is chronological. Because no value carries a zone, no comparison can be
between a civil time and an instant, so no comparison of two datetimes fails.

Both are comparable types wherever a rule requires mutually comparable values.
`greatest` and `least` reduce them across a row, an `order_by` term orders by
one, and R013's `MIN` and `MAX` reduce one; R007 places missing values by the
term's `nulls`, as it does for every other type.

**A temporal value is comparable only with its own type.** R007 admits no
implicit conversion between operation inputs, so a source list or an ordering
term mixing a `datetime` with a `date`, a number, or a string is an error
rather than a comparison over a coerced value. Ordering a moment against a day
would first need a rule saying which moment a day stands for, and this rule
declines to invent one for the same reason it declines to convert between
them.

## Conversion

R011's conversion table routes every temporal cell here. What those cells
apply is:

| Conversion | Result |
|---|---|
| `str` to `date`, `str` to `datetime` | parse the lexical form above; anything else fails |
| `date` to `str`, `datetime` to `str` | the canonical text above |
| `date` to `date`, `datetime` to `datetime` | identity |
| `date` to `datetime`, `datetime` to `date` | fail |
| `int`, `float`, or `bool` to either | fail |
| either to `int` or `float` | fail |
| missing to either | missing |

**A `date` and a `datetime` do not convert into each other, in either
direction.** `date` to `datetime` would invent a time of day, and `datetime`
to `date` would discard a collected one; each would decide silently what a
specification never stated, which is the same reason a non-integral `float`
does not become an `int`. An operation that extracts a day or composes a
moment states that intent explicitly, and none is registered.

A temporal value never enters arithmetic. R010's grammar is numeric, and a
difference between two values is `date_diff` or `study_day` below.

## Partial collected dates

A study collects dates that are truncated to a year or to a year and month,
and neither is a `date`. Such a value is carried as `str` and completed before
it becomes one.

`date_impute` performs that completion as a declared rule rather than as
string surgery. Its result is a `date` like any other, and nothing
distinguishes it from a fully collected one. `date_precision` reads how much
of a date the collected text carried, so a specification can record beside the
date what it supplied; the date value itself still carries no precision.

`minimum_source_precision` bounds how much `date_impute` may invent. Its
default is `year`, preserving completion of both year-only and year-month
sources. With `month`, a year-month source may receive the declared day, while
a valid year-only source produces missing because supplying both month and day
would exceed the declared policy. A complete source date is always returned
unchanged. Falling below the minimum is neither a missing source nor invalid
text, so it does not invoke either R008 handler.

Both read the same source text and answer the same two conditions about it, so
one handler stage in R008 serves both: a missing source, and a non-missing
source that is neither a complete date nor a date prefix. Text that is not a
date is a different defect from an uncollected value, and a specification may
answer them differently.

Neither operation answers about a `datetime`. A truncated moment has no
agreed completion -- an unknown time of day is not the same claim as an unknown
day -- so the collected text stays `str`.

## Operations

R007 registers these operations, orders them, and nests them like any other.
Their input and result types are:

| Operation | Inputs | Result |
|---|---|---|
| `date_diff` | `start` and `end` are `date` | `int` |
| `study_day` | `date` and `reference` are `date` | `int`, never zero |
| `date_impute` | `source` is `str`; `month` and `day` are `int`; `minimum_source_precision` is `year` or `month` | `date` |
| `date_precision` | `source` is `str` | `str` |

**Every temporal operation is a date operation.** A `datetime` operand is an
error rather than a widened one. `date_diff` counts whole calendar units and
its `bounds` field counts endpoints of a day range, and neither has a meaning
between two moments: `unit: day` between `2025-01-01T23:00:00` and
`2025-01-02T01:00:00` could defend `1` or `0`. Widening either operation would
make that choice silently, so both stay on `date`. A difference between two
moments enters the vocabulary when an example needs it.

`date_impute` requires its `month` and `day` to lie within the calendar
ranges its registration states, and the date it completes to must be a real
calendar date. The range checks still apply when a component is not used, so a
specification cannot hide an invalid literal behind a precision policy.

A `datetime` is therefore produced only by converting text and consumed only
by the comparisons above, which is what the examples need and no more.

## Ingestion

This rule defines the text form once, so every place text becomes a temporal
value uses it. R014 applies R011's `str` row to a field's declared type, and
that row reaches the grammar above, so a field declared `date` or `datetime`
in a `types` declaration parses exactly as a column conversion parses.

The two paths differ only in what answers a bad value, which R014 fixes rather
than this rule: an ingested value that does not parse is rejected before any
derivation runs and no handler applies, while a `str` field converted at the
column that declares a temporal type fails there, where `conversion_failure`
can answer. A specification that wants to see a malformed value therefore
leaves the field `str`, which is what `negative-datetime-zone-offset` does.

## Errors

- Text that is not the lexical form above: not a temporal value. For a `date`
  this includes a truncated date, a date carrying a time of day, and the basic
  format; for a `datetime` it additionally includes a zone designator, an
  offset, a fractional second, hour 24, and a leap second. Reaching a temporal
  column, it is the conversion failure R011 defines, handled by
  `conversion_failure` under R008 and otherwise fatal under R005.
- A date part that is not a date in the calendar, such as `2025-02-30`: the
  same failure.
- A year outside `0001` to `9999`: the same failure. The four-digit field
  admits no other year, and that range is also the one Python's `datetime`
  holds.
- A conversion the table above marks `fail`, including `date` to `datetime`
  and `datetime` to `date`: fail rather than choose a representation.
- Comparing or ordering a temporal value against a value of another type:
  fail under R007, which owns comparability.
- A date operation given a `datetime`: fail rather than widen the operation.
- `date_impute` whose `month` or `day` is outside the calendar range, or whose
  completed value is not a real calendar date: fail.
- A temporal value used as an operand in a `compute` expression: fail under
  R010, which admits only numeric identifiers.
- Storing a value an implementation cannot hold exactly, such as a fractional
  or leap second: never reached, because the text is rejected first. An
  implementation must not round to reach one.
