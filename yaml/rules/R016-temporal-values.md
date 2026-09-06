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
  - to_date
  - to_datetime
  - datetime_diff
depends_on: [R005, R006, R007, R008, R010, R011, R014]
---

# Temporal values

## Intent

Define the three temporal values this design admits, `date`, `time`, and
`datetime`: what each denotes, how much of it a study collected, the text it
is read from and written back to, how values of each type order, which
operations read them, and what fails. R and Python must produce the same value,
the same collected precision, the same order, the same output text, and the
same failures from the same input.

## Boundaries

**This rule owns all temporal types completely.** No other rule states what a
date, time, or datetime is, which text becomes one, or what may be done with
one.

R011 owns the `column_type` vocabulary that admits all three and the shape of the
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

R004 owns the predicate grammar. A predicate comparing two temporal values
orders them as this rule defines; no other predicate behavior is owned here.

## The three values

A **`date`** is a complete proleptic Gregorian calendar date. It names a day.

A **`time`** is a complete local time of day resolved to a whole second. It
names a reading on a wall clock without naming a calendar date.

A **`datetime`** is a complete local civil datetime: a date of the same kind
together with a time of day resolved to a whole second. It names a reading on
a wall clock. It is not an instant on a timeline, because it carries no zone
and no offset.

Their fields and ranges are exactly:

| Field | Range | In `date` | In `time` | In `datetime` |
|---|---|---|---|---|
| year | `0001` to `9999` | yes | -- | yes |
| month | `01` to `12` | yes | -- | yes |
| day | `01` to the length of that month in that year | yes | -- | yes |
| hour | `00` to `23` | -- | yes | yes |
| minute | `00` to `59` | -- | yes | yes |
| second | `00` to `59` | -- | yes | yes |

Like every column type, all three additionally admit the missing value.

Every combination of fields in range names one day, local time, or civil
moment, and every such value has one combination of fields. All three value
spaces are total and gapless.

**Every value is complete, and every value records how much of it was
collected.** A value is complete or it is not a value of the type, so a
truncated collected value stays text until something completes it; *Partial
collected dates* below defines the one completion this design offers. Beside
its fields, a value carries one further property: its **collected precision**,
the finest field the collected source supplied.

For a `date` that property is `year`, `month`, or `day`. For a `time` or
`datetime` it is always `second`, because this rule admits no truncated clock
reading: an omitted `ss` names second zero rather than claiming a coarser
value.

Where a value gets its collected precision is fixed by where the value came
from, and only five origins exist:

| Origin | Collected precision |
|---|---|
| parsed from text -- the lexical form below, R011's `str` row, an R014 field | `day` for a `date`; `second` for a `time` or `datetime` |
| `date_impute` | the precision its source carried |
| `to_date` | `day` |
| `to_datetime` | `second` |
| any expression that selects an existing value | unchanged; the property travels with the value |

The first row has no other reachable answer: the grammar below admits only
complete text, so nothing a parse produces was collected in part. R007 fixes
the fifth, for the extreme, conditional, coalescing, and offset expressions
that return an operand rather than computing one.

**Provenance is read off that one property rather than recorded beside it.** A
component finer than the collected precision was supplied by `date_impute`, and
a value whose collected precision is `day` was collected in full. A second flag
would be a second place to keep correct, and the two could disagree.

## Lexical form

Text becomes a temporal value in exactly one shape each:

    date     := YYYY "-" MM "-" DD
    time     := hh ":" mm [ ":" ss ]
    datetime := date "T" time

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
| `14:00:00` | `datetime` | no date; a time of day is not a moment |
| `2025-01-12` | `time` | a date; a day is not a time of day |
| `24:00` | `time` | hour 24; midnight is `00:00` |
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

`14:00:00` is rejected for a different reason from the rest of the table, and
the difference is worth naming. Every other row is a spelling of a value one of
these two types holds; a clock reading carrying no date is not, because both
types name a position on the calendar. A study that collects one, as the `--TM`
family does, keeps the collected text as `str`. Admitting it would be a third
temporal type rather than a widening of `datetime` -- a new entry in R011's
closed vocabulary -- and it enters when an example needs a time of day that no
date accompanies.

## No zone, no offset

A `time` or `datetime` carries no timezone and no offset. Text offering a
datetime with either is rejected rather than normalized; the time grammar has
no place to offer one.

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

A `time` and a `datetime` resolve to a whole second, and text carrying a
fractional second is rejected.

The runtimes cannot agree on a fraction. Python's `datetime.time` and
`datetime.datetime` record whole microseconds as integers. R's temporal
carriers use a binary64 count of seconds, which represents most fractions only
approximately and prints them under its own rounding. Admitting one would mean
two implementations that store, compare, and render the same collected value
differently, and the design requires them to agree.

Whole seconds are exact in both, which is what fixes the representation:

| Runtime | `date` | `time` | `datetime` |
|---|---|---|---|
| Python | `datetime.date` | `datetime.time`, `tzinfo` unset, `microsecond` zero | `datetime.datetime`, `tzinfo` unset, `microsecond` zero |
| R | `Date` | `difftime` since midnight with `units` set to `"secs"` | `POSIXct` with `tzone` set to `"UTC"` |

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
| `time` | `hh:mm:ss` |
| `datetime` | `YYYY-MM-DDThh:mm:ss` |

with every field zero-padded to its width and, for a `time` or `datetime`, the
seconds always present. This is the text a temporal value converts to under
R011's `str` row and the text the artifact records for a temporal column, so a `str`
column derived from one and the artifact's own rendering of that same value
never disagree. R011 fixes the same relationship for `float`.

**Canonical text is not the collected text.** A value parsed from
`2025-01-12T14:00` renders as `2025-01-12T14:00:00`, because what is rendered
is the value and the value names second zero. This is the rule R011 already
states for `float`, where `1.50` renders as `1.5`: a declared type stores a
value rather than the characters it arrived as. A variable that must carry the
collected characters unchanged is `str`, which keeps them and still orders
chronologically under R007.

Unlike `float`, none of the forms takes a project setting. This rule fixes the
rendered precision at one day and one second, so a project has nothing left to
declare.

**Canonical text carries the fields alone, so collected precision is not
observable outside the derivation.** A temporal value converted to `str` under
R011's row, the artifact's record of a temporal column, and the typed value
R018 encodes for a function argument all carry the day, time, or moment and
nothing about how much of it was collected. This is deliberate: the property
answers a question about a study's collection, and a reader holding only the
text has no way to check an answer to it. A specification that must carry
precision past any of those three boundaries derives a column from
`date_precision`, which is data the artifact records like any other.

## Comparison and ordering

Two values of the same temporal type compare field by field, most significant
first. A `date` compares year, month, and day; a `time` compares hour, minute,
and second; a `datetime` compares all six fields in that order.
Every pair of non-missing values of one type is therefore ordered, and that
order is chronological. Because no value carries a zone, no comparison can be
between a civil time and an instant, so no comparison of two datetimes fails.

All three are comparable types wherever a rule requires mutually comparable values.
`greatest` and `least` reduce them across a row, an `order_by` term orders by
one, and R013's `MIN` and `MAX` reduce one; R007 places missing values by the
term's `nulls`, as it does for every other type.

**Collected precision takes no part in a comparison.** Two values compare by
the fields above, and precision is not one of them. A value completed from a
year and a month therefore orders against a fully collected one on the day it
names, wins a `greatest` it is the latest operand of, and satisfies a
predicate the day satisfies. Every pair of non-missing values of one type
stays ordered, which is what keeps an `order_by` term total and R007's
comparability argument intact.

This is a decision and not an omission, and it is the one the imputed value
itself forces. A completed date names a day: that is what completing it did.
An imputed operand that lost a comparison would have to denote something else
-- the interval its collected components still admit, or a day carrying a rank
against collected ones -- and either is a different value space with its own
ordering, its own canonical text, and its own conversions. That is a type this
design does not have, not a property of the three it does.

The cost is worth stating plainly, because it is the case the property was
added for: an imputed start still decides whether an event is treatment
emergent, and precision does not stop it. What precision changes is that the
specification classifying the event can now see that the day was supplied,
and can say so in the artifact it writes. A specification that needs a
supplied day not to reach a classification bounds the imputation with
`not_before` below, or states the constraint it wants as a verification under
R009. Neither is a comparison, which is why neither is this section's to
define.

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
| `str` to a temporal type | parse that type's lexical form above; anything else fails |
| a temporal type to `str` | that type's canonical text above |
| a temporal type to itself | identity |
| one temporal type to another | fail; use an explicit registered operation where one exists |
| `int`, `float`, or `bool` to a temporal type | fail |
| a temporal type to `int` or `float` | fail |
| missing to a temporal type | missing |

**Temporal types do not convert into each other.** A `date` converted to a
`datetime` would invent a time of day, a `time` converted to a `datetime` would
invent a date, and a `datetime` converted to either component would silently
discard the other. `to_date` explicitly requests date extraction, and
`to_datetime` explicitly supplies both components needed to compose a moment.

A temporal value never enters arithmetic. R010's grammar is numeric, and a
difference between two values uses the temporal operation below that names the
intended calendar or whole-second meaning.

## Partial collected dates

A study collects dates that are truncated to a year or to a year and month,
and neither is a `date`. Such a value is carried as `str` and completed before
it becomes one.

`date_impute` performs that completion as a declared rule rather than as
string surgery. Its result is a `date` like any other in every respect a
comparison can see, and it carries the collected precision of the text it
completed, so which of its components were supplied is a property of the value
rather than a fact only the specification remembers.

**There is one precision ladder, ordered `year` before `month` before `day`**,
and it is spelled twice because it appears at two kinds of site. Where a
specification declares a policy it names a level of that ladder:
`minimum_source_precision` takes `year` or `month`. Where an operation reports
a precision it returns that ladder's code: `date_precision` returns `Y`, `M`,
or `D`. The levels correspond in order and the spellings are not two
vocabularies.

`date_precision` reads a precision from either kind of source. Given the
collected text it reports how much of a date that text carries. Given a `date`
value it reports the collected precision that value carries, which is
what lets a specification derive an imputation flag from the analysis date
itself rather than from the text the date was completed from. Reading the date
binds the flag to the value it describes; reading the text leaves the two in
step only by convention, and nothing detects it when they drift.

**A known day in an unknown month has no representation, and this rule does
not invent one.** The collected text admitted above is prefix truncation only:
a year, or a year and a month. A day known without its month cannot be
collected in the first place, so there is no value for a precision to
describe, and the ladder is a prefix ladder for exactly that reason. A study
that records such a value keeps the collected text as `str`, as it does for
every other text this rule does not admit.

`minimum_source_precision` bounds how much `date_impute` may invent. Its
default is `year`, preserving completion of both year-only and year-month
sources. With `month`, a year-month source may receive the declared day, while
a valid year-only source produces missing because supplying both month and day
would exceed the declared policy. A complete source date is always returned
unchanged. Falling below the minimum is neither a missing source nor invalid
text, so it does not invoke either R008 handler.

**A `day` may name a position in its month instead of a number.** `first` and
`last` are resolved after `month` is fixed, against the month the completed
date lands in, so `last` is 28 or 29 in a February according to the year and
30 or 31 elsewhere. A study placing a partial date at the end of its month
therefore declares one rule rather than one rule per month, and the completed
value is a real calendar date by construction.

**`not_before` bounds the completed date from below, and moves only what
imputation supplied.** The collected components of a truncated source admit an
interval of days -- `2025` admits the whole of that year, `2025-01` the whole
of that January -- and the bound may move the result only inside it. A
completed date already on or after the bound stands. Otherwise the result is
the earliest day the interval admits that satisfies the bound, so the bound
invents no more than it must. When the interval admits no such day the result
is missing, which like falling below the minimum precision is neither a
missing source nor invalid text and invokes neither handler. A missing bound
is no bound.

A complete source date is returned unchanged whatever the bound says, because
it supplied nothing for the bound to move. This is what makes the bound a rule
rather than a comparison a specification could write itself: it constrains an
invented component and never a collected one. A specification constraining
collected dates states a verification under R009, which is where a claim about
data a study recorded belongs.

The parameters therefore apply in a fixed order: a missing or invalid source
answers first, then a source below the minimum precision, then completion from
`month` and `day`, then the bound.

Where both operations read the same source text they answer the same two
conditions about it, so one handler stage in R008 serves both: a missing
source, and a non-missing source that is neither a complete date nor a date
prefix. Text that is not a date is a different defect from an uncollected
value, and a specification may answer them differently. A `date_precision`
reading a value has only the first of the two to answer, because a value that
exists is already a value of its type.

Neither operation answers about a `time` or `datetime`. A truncated moment has
no agreed completion -- an unknown time of day is not the same claim as an
unknown day -- so the collected text stays `str`. A non-date temporal value is
not a `date_precision` source either: a `time` or `datetime` is collected to a
whole second or it is not a value. Component extraction and composition remain
the explicit work of `to_date` and `to_datetime`.

## Operations

R007 registers these operations, orders them, and nests them like any other.
Their input and result types are:

| Operation | Inputs | Result |
|---|---|---|
| `date_diff` | `start` and `end` are `date` | `int` |
| `study_day` | `date` and `reference` are `date` | `int`, never zero |
| `date_impute` | `source` is `str`; `month` is `int`; `day` is `int` or a month position; `minimum_source_precision` is `year` or `month`; `not_before` is `date` | `date` |
| `date_precision` | `source` is `str` or `date` | `str` |
| `to_date` | `source` is `datetime` | `date` with collected precision `day` |
| `to_datetime` | `date` is `date`; `time` is `time` | `datetime` with collected precision `second` |
| `datetime_diff` | `start` and `end` are `datetime` | signed whole-second `int` |

`date_diff` remains a calendar operation. Its `bounds` counts endpoints of a
day range and it is not widened to moments. `datetime_diff` instead returns
`end - start` in whole seconds. It treats a local civil datetime as its ordinal
Gregorian day and seconds since midnight, so the result is exactly

```text
(end ordinal day - start ordinal day) * 86400
  + end seconds since midnight - start seconds since midnight
```

This definition crosses midnight, is negative when `end` precedes `start`, and
does not consult a timezone or daylight-saving rule. The full year range fits
in R011's signed 64-bit `int`. A missing operand yields missing.

`date_impute` requires its `month` and a numeric `day` to lie within the
calendar ranges its registration states, and the date it completes to must be
a real calendar date. The range checks still apply when a component is not
used, so a specification cannot hide an invalid literal behind a precision
policy. A `day` naming a position in its month is not a literal to
range-check, and the calendar-date requirement cannot fail for one: it names
whichever day the target month begins or ends with rather than a number that
month might not have. Any other `day` token is neither a number nor a
position, and is rejected where the specification is read.

`to_date` copies a datetime's calendar fields and drops its time fields.
`to_datetime` copies all fields from its `date` and `time` operands into one
local civil datetime. Neither operation converts an operand implicitly. A
missing required operand yields a missing result; any non-missing operand of
another type is the incompatible-input error R007 defines.

## Ingestion

This rule defines the text form once, so every place text becomes a temporal
value uses it. R014 applies R011's `str` row to a field's declared type, and
that row reaches the grammar above, so a field declared `date`, `time`, or
`datetime` in a `types` declaration parses exactly as a column conversion parses.

The two paths differ only in what answers a bad value, which R014 fixes rather
than this rule: an ingested value that does not parse is rejected before any
derivation runs and no handler applies, while a `str` field converted at the
column that declares a temporal type fails there, where `conversion_failure`
can answer. A specification that wants to see a malformed value therefore
leaves the field `str`, which is what `negative-datetime-zone-offset` does.

## Errors

- Text that is not the lexical form above: not a temporal value. For a `date`
  this includes a truncated date, a date carrying a time of day, and the basic
  format; for a `time` or `datetime` it includes a fractional second, hour 24,
  and a leap second; for a `datetime` it additionally includes a zone designator
  or offset. Reaching a temporal column, it is the conversion failure R011
  defines, handled by
  `conversion_failure` under R008 and otherwise fatal under R005.
- A date part that is not a date in the calendar, such as `2025-02-30`: the
  same failure.
- A year outside `0001` to `9999`: the same failure. The four-digit field
  admits no other year, and that range is also the one Python's `datetime`
  holds.
- A conversion the table above marks `fail`, including any conversion from one
  temporal type to another: fail rather than choose or discard components.
- Comparing or ordering a temporal value against a value of another type:
  fail under R007, which owns comparability.
- `date_diff`, `study_day`, `date_impute`, or `date_precision` given a
  `datetime` or `time`: fail rather than widen the operation. This includes
  either type reaching `date_precision` as a value source.
- `to_date` given anything other than a `datetime`: fail as an incompatible
  input. A missing `datetime` yields a missing date instead.
- `to_datetime` given anything other than one `date` and one `time`: fail as
  an incompatible input. A missing required operand yields a missing datetime.
- `datetime_diff` given anything other than two datetimes: fail as an
  incompatible input. A missing required operand yields a missing integer.
- `date_impute` whose `month` or numeric `day` is outside the calendar range,
  or whose completed value is not a real calendar date: fail. Neither can
  arise from a `day` naming a position in its month.
- `date_impute` whose `day` is a token that is neither a number nor a declared
  position: rejected where the specification is read, before any data is
  seen.
- `date_impute` whose completed value cannot satisfy `not_before` within the
  interval its collected components admit: missing, not a failure. Like a
  source below the minimum precision, it is neither a missing source nor
  invalid text, so no R008 handler answers it.
- A temporal value used as an operand in a `compute` expression: fail under
  R010, which admits only numeric identifiers.
- Storing a value an implementation cannot hold exactly, such as a fractional
  or leap second: never reached, because the text is rejected first. An
  implementation must not round to reach one.
