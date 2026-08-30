---
id: R016
title: Datetime Values
status: normative
applies_to: [column_type, column.type, derivation]
depends_on: [R006, R007, R011]
---

# Datetime values

## Intent

Define the portable `datetime` value: the text it is written as, the moment it
denotes, how two of them order, and the one text an implementation writes back.
R and Python must produce the same value, the same order, the same output text,
and the same failures from the same input.

## Boundaries

This rule owns the datetime value itself. R011 owns the `column_type`
vocabulary that admits `datetime` and the conversion table that reaches it, and
names this rule for the grammar and the canonical text those cells use, exactly
as it names R010 for the `number` production its numeric cells use. R014 owns
what a stored field carries before any expression reads it, and reaches this
grammar through R011's `str` row. R007 owns which expression accepts which
input type, R008 owns `conversion_failure`, and R005 owns when conversion
happens.

R004 owns the predicate grammar and is draft. A predicate comparing two
datetimes orders them as this rule defines; what else a predicate may write is
R004's to close.

## The value

A `datetime` is a **complete local civil datetime**: a proleptic Gregorian
calendar date together with a time of day resolved to a whole second. It names
a reading on a wall clock. It is not an instant on a timeline, because it
carries no zone and no offset.

Its fields and their ranges are exactly:

| Field | Range |
|---|---|
| year | `0001` to `9999` |
| month | `01` to `12` |
| day | `01` to the length of that month in that year |
| hour | `00` to `23` |
| minute | `00` to `59` |
| second | `00` to `59` |

Like every column type, `datetime` additionally admits the missing value.

Every combination of fields in range names one civil moment, and every civil
moment in range has one combination of fields. The value space is total and
gapless.

## Lexical form

Text becomes a `datetime` in exactly one shape:

    datetime := date "T" time
    date     := YYYY "-" MM "-" DD
    time     := hh ":" mm [ ":" ss ]

`YYYY` is four ASCII digits and `MM`, `DD`, `hh`, `mm`, and `ss` are two each,
zero-padded to that width. The date part is exactly the form R011 accepts for a
`date`, so one production defines the calendar half of both types.

An omitted `ss` names second `00`, and it is the only omission the form
permits. Nothing else is defaulted, no sign or surrounding whitespace is
accepted, and no other separator or field order is recognised.

Rejecting everything else is what makes two implementations agree. Each
runtime's own parser accepts a wider and a different set: a space separator, a
lowercase `t`, a bare date, and a trailing `Z` are each read by one of them and
not the other, so a rule admitting whatever a runtime happened to accept would
not be portable. These are the cases that decision costs:

| Rejected | Why |
|---|---|
| `2025-01-12` | no time of day; a `date` does not become a `datetime` |
| `14:00:00` | no date; a time of day alone is not a value of this type |
| `2025-01-12 14:00:00` | a space; the separator is `T` |
| `2025-01-12t14:00:00` | a lowercase `t`; the separator is uppercase |
| `20250112T140000` | basic format; the extended form only, as for `date` |
| `2025-1-2T4:00` | fields not padded to their width |
| `2025-01-12T14:00:00Z` | a zone designator |
| `2025-01-12T14:00:00+02:00` | an offset |
| `2025-01-12T14:00:00.5` | a fractional second |
| `2025-01-12T24:00` | hour 24; midnight is `00:00` of the following day |
| `2025-01-12T23:59:60` | a leap second |
| `2025-02-30T00:00:00` | not a date in the calendar |

`24:00` and `23:59:60` are rejected for the same reason as the rest and not
only because they are unusual. `2025-01-12T24:00` names the moment
`2025-01-13T00:00` already names, and the two spellings disagree about the day,
so admitting the first would leave the date a value carries depending on which
spelling arrived. A leap second is not a value either runtime holds: neither
R's nor Python's representation has a sixty-first second, so nothing could be
stored.

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
and carries no offset. A study that does record one keeps it in its own column,
where it is data a specification can read, and an instant-typed value can be
added later without invalidating any specification written under this rule.

Two consequences are worth stating, because both are failure modes this type
does not have:

- **No civil time is nonexistent or ambiguous.** A daylight-saving gap or
  repetition is a property of a zone. Without one, every value in the field
  ranges above names exactly one moment, so there is nothing to reject as
  unrepresentable and nothing to disambiguate.
- **A datetime is never shifted.** Nothing normalizes it into another zone, so
  the value an implementation holds carries the fields the text it parsed
  carried.

## Whole seconds

A `datetime` resolves to a whole second, and text carrying a fractional second
is rejected.

The runtimes cannot agree on a fraction. Python's `datetime.datetime` records
whole microseconds as integers. R's `POSIXct` is a binary64 count of seconds,
which represents most fractions only approximately and prints them under its
own rounding. Admitting one would mean two implementations that store, compare,
and render the same collected value differently, and the design requires them
to agree.

Whole seconds are exact in both, which is what fixes the representation:

| Runtime | Value | Exactness |
|---|---|---|
| Python | `datetime.datetime` with `tzinfo` unset and `microsecond` zero | exact |
| R | `POSIXct` with `tzone` set to `"UTC"` | integral seconds below 2^53 are exact in binary64 |

R's `tzone` is a carrier and not a claim about the value: `"UTC"` is chosen
because it is the one zone with no offset and no daylight-saving rule, so it
cannot shift a value or make one ambiguous, and it keeps the machine's timezone
out of the result. An implementation must set it rather than leave it empty.

A study that collects sub-second times keeps the collected text in a `str`
column until a rule fixes a representation both runtimes share.

## Canonical text

A `datetime` is written back in exactly one form:

    YYYY-MM-DDThh:mm:ss

with every field zero-padded to its width and the seconds always present. This
is the text a `datetime` converts to under R011's `str` row and the text the
artifact records for a `datetime` column, so a `str` column derived from a
datetime and the artifact's own rendering of that datetime never disagree.
R011 fixes the same relationship for `float`.

**Canonical text is not the collected text.** A value parsed from
`2025-01-12T14:00` renders as `2025-01-12T14:00:00`, because what is rendered
is the value and the value names second zero. This is the rule R011 already
states for `float`, where `1.50` renders as `1.5`: a declared type stores a
value rather than the characters it arrived as. A variable that must carry the
collected characters unchanged is `str`, which keeps them and still orders
chronologically under R007.

Unlike `float`, the form takes no project setting. This rule fixes the
precision at one second, so a project has nothing left to declare.

## Comparison and ordering

Two datetimes compare field by field, most significant first: year, then month,
day, hour, minute, and second. Every pair of non-missing datetimes is therefore
ordered, and that order is chronological on the shared wall clock. Because no
value carries a zone, no comparison can be between a civil time and an instant,
so no comparison of two datetimes fails.

`datetime` is a comparable type wherever a rule requires mutually comparable
values. `greatest` and `least` reduce datetimes across a row, an `order_by`
term orders by one, and R013's `MIN` and `MAX` reduce one; R007 places missing
values by the term's `nulls`, as it does for every other type.

**A datetime is comparable only with a datetime.** R007 admits no implicit
conversion between operation inputs, so a source list or an ordering term
mixing a `datetime` with a `date`, a number, or a string is an error rather
than a comparison over a coerced value. Ordering a moment against a day would
first need a rule saying which moment a day stands for, and this rule declines
to invent one for the reason R011 declines to convert between them: either
choice would silently decide a classification the specification never stated.

## What this rule does not add

No expression is registered here. A `datetime` is produced by converting text
under R011 and is consumed by the comparisons above, which is what the
motivating examples need and no more.

Further operations are foreseeable and none is registered:

- extracting the date from a datetime, and composing a datetime from a date
  and a time, which R011 leaves as failing conversions until an operation
  states the intent explicitly;
- a difference between two datetimes. `date_diff` counts whole calendar units
  between dates and its `bounds` field counts endpoints of a day range, and
  neither has a meaning between two moments: `unit: day` between
  `2025-01-01T23:00:00` and `2025-01-02T01:00:00` could defend `1` or `0`.
  Widening it would make that choice silently, so R007 keeps it and
  `study_day` on `date`.

Each enters the vocabulary when an example needs it, as `examples/agents.md`
requires. `examples/plan.md` records them as open.

## Ingestion

This rule defines the text form once, so every place text becomes a datetime
uses it, and R014 needs no clause of its own. R014 applies R011's `str` row to
a field's declared type, and that row reaches the grammar above, so a field
named `datetime` in a `types` declaration parses exactly as a column conversion
parses. `sdtm-ae-effective-transaction` declares its audit field that way and
orders the transaction log by the value rather than by the text.

The two paths differ only in what answers a bad value, which R014 fixes rather
than this rule: an ingested value that does not parse is rejected before any
derivation runs and no handler applies, while a `str` field converted at the
column that declares `datetime` fails there, where `conversion_failure` can
answer. A specification that wants to see a malformed moment therefore leaves
the field `str`, which is what `negative-datetime-zone-offset` does.

## Errors

- Text that is not the lexical form above, including a zone designator, an
  offset, a fractional second, hour 24, a leap second, and a field out of
  range: not a datetime. Reaching a `datetime` column, it is the conversion
  failure R011 defines, handled by `conversion_failure` under R008 and
  otherwise fatal under R005.
- A date part that is not a date in the calendar, such as `2025-02-30`: the
  same failure.
- A year outside `0001` to `9999`: the same failure. The four-digit field
  admits no other year, and that range is also the one Python's `datetime`
  holds.
- Comparing or ordering a `datetime` against a value of another type: fail
  under R007, which owns comparability.
- Storing a value an implementation cannot hold exactly, such as a fractional
  or leap second: never reached, because the text is rejected first. An
  implementation must not round to reach one.
