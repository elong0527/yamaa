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

---

# Temporal values

## Intent

Define the two temporal values this design admits, `date` and `datetime`: what
each denotes, how much of it a study collected, the text it is read from and
written back to, how two of them order, which operations read them, and what
fails. R and Python must produce the same value, the same collected precision,
the same order, output text, and failures for the same input.

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
behavior. This rule states what is shared: the types those parameters carry and
the value they produce.

R004 owns the predicate grammar. A predicate comparing two temporal values
orders them as this rule defines; no other predicate behavior is owned here.

## The two values

**R016-1.** A `date` is a complete proleptic Gregorian calendar date. It names
day.

**R016-2.** A `datetime` is a complete local civil datetime: a date of the same
kind together with a time of day resolved to a whole second. It names a reading
on a wall clock. It is not an instant on a timeline, because it carries no zone
and no offset.

**R016-3.** Their fields and ranges are exactly:

| Field | Range | In `date` | In `datetime` |
|---|---|---|---|
| year | `0001` to `9999` | yes | yes |
| month | `01` to `12` | yes | yes |
| day | `01` to the length of that month in that year | yes | yes |
| hour | `00` to `23` | -- | yes |
| minute | `00` to `59` | -- | yes |
| second | `00` to `59` | -- | yes |

**R016-4.** Like every column type, both additionally admit the missing value.

**R016-5.** Every combination of fields in range names one day or one civil
moment, and every day or civil moment in range has one combination of fields.
Both value spaces are total and gapless.

**R016-6.** Every value is complete, and every value records how much of it was
collected.** A value is complete or it is not a value of the type, so a
truncated collected value stays text until something completes it; *Partial
collected dates* below defines this design's one completion. Beside its
fields, a value carries one further property: its **collected precision**, the
finest field the collected source supplied.

**R016-7.** For a `date` that property is `year`, `month`, or `day`. For a
`datetime` it is always `second`, because this rule admits no truncated moment:
an omitted `ss` names second zero rather than claiming a coarser value, so a
`datetime` is collected in full or it is not a value.

**R016-8.** Where a value gets its collected precision is fixed by where the
value came from, and only four origins exist:

| Origin | Collected precision |
|---|---|
| parsed text | `day`; `datetime`: `second` |
| `date_impute` | the precision its source carried |
| `to_date` | `day` |
| selecting an existing value | unchanged; property follows selected value |

**R016-9.** The first row has no other reachable answer: the grammar below
admits only complete text, so nothing a parse produces was collected in part.
R007 fixes the fourth, for the extreme, conditional, coalescing, and offset
expressions that return an operand rather than computing one.

**R016-10.** Provenance is read off that one property rather than recorded
beside it. A component finer than the collected precision was supplied by
`date_impute`, and a value whose collected precision is `day` was collected in
full. A second flag would be a second place to keep correct, and the two could
disagree.

## Lexical form

**R016-11.** Text becomes a temporal value in exactly one shape each:

    date     := YYYY "-" MM "-" DD
    datetime := date "T" time
    time     := hh ":" mm [ ":" ss ]

**R016-12.** `YYYY` is four ASCII digits and `MM`, `DD`, `hh`, `mm`, and `ss`
are two each, zero-padded to that width. One production defines the calendar
half of both types, so a date parses identically wherever it appears.

**R016-13.** An omitted `ss` names second `00`, and it is the only omission
either form permits. Nothing else is defaulted, no sign or surrounding
whitespace is accepted, and no other separator or field order is recognised.

**R016-14.** Rejecting everything else is what makes two implementations agree.
Each runtime's own parser accepts a wider and a different set: a space
separator, lowercase `t`, bare date read as a moment, and trailing `Z` are
each read by one of them and not the other, so a rule admitting whatever a
runtime happened to accept would not be portable. These are the cases that
decision costs:

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

**R016-15.** `24:00` and `23:59:60` are rejected for the same reason as the
and not only because they are unusual. `2025-01-12T24:00` names the moment
`2025-01-13T00:00` already names, and the two spellings disagree about the day,
so admitting the first would leave the date a value carries depending on which
spelling arrived. A leap second is not a value either runtime holds.
Neither R's nor Python's representation has a sixty-first second, so none
can be stored.

**R016-16.** `14:00:00` is rejected for a different reason from the rest of the
table, and the difference is worth naming. Every other row is a spelling of a
value one of these two types holds; a clock reading carrying no date is not,
because both types name a position on the calendar. A study that collects one,
as the `--TM` family does, keeps the collected text as `str`. Admitting one
be a third temporal type rather than a widening of `datetime` -- a new entry in
R011's closed vocabulary -- and it enters when an example needs a time of day
that no date accompanies.

## No zone, no offset

**R016-17.** A `datetime` carries no timezone and no offset, and text carrying
either is rejected rather than normalized.

**R016-18.** Admitting both a local and an offset-aware value would put two
kinds of datetime in one column type, and the target runtimes disagree about
that pair. Python refuses to order a naive datetime against an aware one and
raises instead. R has no naive datetime at all: a `POSIXct` always carries a
`tzone`, and an empty one resolves against the machine's timezone, so the same
specification would order the same column differently on two machines. Neither
behavior is this design's to choose, because each is a property of that
runtime's type.

**R016-19.** Prohibiting the zone removes the disagreement rather than
arbitrating it, and it costs a study nothing it collects: a CDISC `--DTC` value
is local site time and carries no offset. A study that does record one keeps it
in its own column, where it is data a specification can read, and an instant-
typed value can be added later without invalidating any specification written
under this rule.

**R016-20.** Two consequences are worth stating, because both are failure modes
this type does not have:

- **R016-21.** No civil time is nonexistent or ambiguous. A daylight-saving gap
  or repetition arises only when a wall-clock reading is mapped to a timeline.
  A zone supplies that mapping. Without a zone, every field combination
  in the ranges above is a value of the type and denotes the reading it spells,
  so there is nothing to reject as unrepresentable and nothing to disambiguate.
- **R016-22.** A datetime is never shifted. Nothing normalizes it into another
  zone, so the value an implementation holds carries the fields the text it
  parsed carried.

## Whole seconds

**R016-23.** A `datetime` resolves to a whole second, and text carrying a
fractional second is rejected.

**R016-24.** The runtimes cannot agree on a fraction. Python's
`datetime.datetime` records whole microseconds as integers. R's `POSIXct` is a
binary64 count of seconds, which represents most fractions only approximately
and prints them under its own rounding. Admitting one would mean two
implementations that store, compare, and render the same collected value
differently, and the design requires them to agree.

**R016-25.** Whole seconds are exact in both, which is what fixes the
representation:

| Runtime | `date` | `datetime` |
|---|---|---|
| Python | `datetime.date` | `datetime`; tzinfo unset; microsecond zero |
| R | `Date` | `POSIXct` with `tzone` set to `"UTC"` |

**R016-26.** Integral seconds below 2^53 are exact in R's binary64
representation, which covers all years above. R's `tzone` is a carrier, not a
value claim. `UTC` has no offset or daylight-saving rule. It cannot shift a
value or make one ambiguous, and it keeps the machine's timezone out of the
result. An implementation must set it rather than leave it empty.


**R016-27.** A study that collects sub-second times keeps the collected text in
a `str` column until a rule fixes a representation both runtimes share.

## Canonical text

**R016-28.** A temporal value is written back in exactly one form:

| Type | Canonical text |
|---|---|
| `date` | `YYYY-MM-DD` |
| `datetime` | `YYYY-MM-DDThh:mm:ss` |

**R016-29.** With every field zero-padded to its width and, for a `datetime`,
the seconds always present. This is the text a temporal value converts to under
R011's `str` row and the text the artifact records for a temporal column, so a
`str` column derived from one and the artifact's own rendering of that same
value never disagree. R011 fixes the same relationship for `float`.

**R016-30.** Canonical text is not the collected text. A value parsed from
`2025-01-12T14:00` renders as `2025-01-12T14:00:00` because the value names
second zero. Like `float`, a declared type stores a value rather than the
characters it received. For example, `1.50` renders as `1.5`. A variable
that must retain collected characters unchanged is `str`. It still orders
chronologically under R007.


**R016-31.** Unlike `float`, neither form takes a project setting. This rule
fixes rendered precision at one day and one second, so a project has nothing
left to declare.

**R016-32.** Canonical text carries the fields alone, so collected precision is
not observable outside the derivation.** A temporal value converted to `str`
under R011's row, the artifact's record of a temporal column, and the typed
value R018 encodes for a function argument all carry the day or the moment and
nothing about how much of it was collected. This is deliberate: the property
answers a question about a study's collection, and a reader holding only the
text has no way to check an answer to it. A specification that must carry
precision past any of those three boundaries derives a column from
`date_precision`, which is data the artifact records like any other.

## Comparison and ordering

**R016-33.** Two values of the same temporal type compare field by field, most
significant first: year, then month, then day; a `datetime` then uses hour,
minute, and second. Every pair of non-missing values of one type is ordered,
and that order is chronological. No value carries a zone, so a comparison
cannot be between a civil time and an instant. No datetime comparison fails.

**R016-34.** Both are comparable types wherever a rule requires mutually
comparable values. `greatest` and `least` reduce them across a row, an
`order_by` term orders by one, and R013's `MIN` and `MAX` reduce one; R007
places missing values by the term's `nulls`, as it does for every other type.

**R016-35.** Collected precision takes no part in a comparison. Two values
compare by the fields above; precision is not one of them. A value completed
from a year and a month therefore orders against a fully collected one on the
day it names, wins a `greatest` it is the latest operand of, and satisfies a
predicate the day satisfies. Every pair of non-missing values of one type stays
ordered, which is what keeps an `order_by` term total and R007's comparability
argument intact.

**R016-36.** This is a decision and not an omission, and it is the one the
imputed value itself forces. A completed date names a day: that is what
completing it did. An imputed operand that lost a comparison would have to
denote something else: the interval its collected components still admit, or a
day carrying a rank against collected ones -- and either is a different value
space with its own ordering, its own canonical text, and its own conversions.
That is a type this design does not have, not a property of the two it does.

**R016-37.** The cost is worth stating plainly, because it is the case the
property was added for: an imputed start still decides whether an event is
treatment emergent, and precision does not stop it. What precision changes is
that the specification classifying the event can now see that the day was
supplied. The specification can record that fact in the artifact. A
specification that needs a supplied day not to reach a classification
bounds the imputation with `not_before`, or states a verification under
R009. Neither is a comparison, so neither belongs in this section.

**R016-38.** A temporal value is comparable only with its own type. R007 admits
no implicit conversion between operation inputs. A source list or ordering term
mixing a `datetime` with a `date`, number, or string is an error
rather than a comparison over a coerced value. Ordering a moment against a day
first need a rule saying which moment a day stands for, and this rule declines
to invent one for the same reason it declines to convert between them.

## Conversion

**R016-39.** R011's conversion table routes every temporal cell here:


| Conversion | Result |
|---|---|
| `str` to `date` or `datetime` | parse lexical form; anything else fails |
| `date` to `str`, `datetime` to `str` | the canonical text above |
| `date` to `date`, `datetime` to `datetime` | identity |
| `date` <-> `datetime` | fail; use an explicit operation |
| `int`, `float`, or `bool` to either | fail |
| either to `int` or `float` | fail |
| missing to either | missing |

**R016-40.** A `date` and a `datetime` do not convert in either direction.
`date` to `datetime` would invent a time, and `datetime` to `date` would
discard a collected time. Each conversion would silently decide what a
specification did not state. This is why a non-integral `float` does not
become an `int`. `to_date` explicitly discards time and returns a calendar
date. No operation composes a moment from a date. An operation enters the
vocabulary when an example needs one. Its registration declares its intent
and any time of day it supplies. The conversion cell stays `fail` whether
or not such an operation is registered, because inventing a component is an
operation's to declare and never a conversion's to perform.


**R016-41.** A temporal value never enters arithmetic. R010's grammar is
numeric, and a difference between two values is `date_diff` or `study_day`
below.

## Partial collected dates

**R016-42.** A study collects dates that are truncated to a year or to a year
and month, and neither is a `date`. Such a value is carried as `str` and
completed before it becomes one.

**R016-43.** `date_impute` performs that completion as a declared rule rather
than string surgery. Its result is a `date` like any other. It carries the
collected precision of its source text, so the value records which components
were supplied rather than leaving that fact to the specification.


**R016-44.** One precision ladder orders `year`, `month`, then `day`.
A policy names a ladder level: `minimum_source_precision` takes `year`
or `month`. `date_precision` returns the ladder code: `Y`, `M`, or `D`.
The levels correspond in order. The spellings are not two vocabularies.



**R016-45.** `date_precision` reads a precision from either kind of source.
Given the collected text it reports how much of a date that text carries. Given
a temporal value it reports that value's collected precision. This lets a
specification derive an imputation flag from the analysis date itself.
Reading the date binds the flag to its value. Reading the text leaves the
flag and value in step only by convention, and drift is undetected.


**R016-46.** A known day in an unknown month has no representation, and this
rule does not invent one.** The collected text admitted above is prefix
truncation only: a year, or a year and a month. A day known without its month
cannot be collected in the first place, so there is no value for a precision to
describe, and the ladder is a prefix ladder for exactly that reason. A study
that records such a value keeps the collected text as `str`, as it does for
every other text this rule does not admit.

**R016-47.** `minimum_source_precision` bounds how much `date_impute` may
invent. Its default is `year`, allowing year-only and year-month sources.
With `month`, a year-month source may receive the declared day. A valid
year-only source produces missing because supplying month and day would
exceed the declared policy. A complete source date is always returned
unchanged. Falling below the minimum is neither a missing source nor invalid
text, so it does not invoke either R008 handler.

**R016-48.** A `day` may name a position in its month instead of a number.
`first` and `last` are resolved after `month` is fixed, against the month the
completed date lands in, so `last` is 28 or 29 in a February according to the
year and 30 or 31 elsewhere. A study placing a partial date at the end of its
month therefore declares one rule rather than one rule per month, and the
completed value is a real calendar date by construction.

**R016-49.** `not_before` bounds the completed date from below, and moves only
what imputation supplied.** The collected components of a truncated source
admit a day interval: `2025` admits its year and `2025-01` admits its month.
The bound may move the result only within that interval. A completed
date on or after the bound stands. Otherwise the result is the earliest
day the interval admits that satisfies the bound, so the bound invents no more

than it must. When no day in the interval satisfies the bound, the result is
missing. Like falling below minimum precision, this is neither missing nor
invalid text and invokes no handler. A missing bound is no bound.
**R016-50.** A complete source date is returned unchanged whatever the bound
says, because it supplied nothing for the bound to move. This is what makes the
bound a rule rather than a comparison a specification could write itself: it
constrains an invented component and never a collected one. A specification
constraining collected dates states a verification under R009, which is where a
claim about data a study recorded belongs.

**R016-51.** The parameters therefore apply in a fixed order: a missing or
invalid source answers first, then a source below the minimum precision, then
completion from `month` and `day`, then the bound.

**R016-52.** Where both operations read the same source text they answer the
same two conditions about it, so one handler stage in R008 serves both: a
missing source, and a non-missing source that is neither a complete date nor a
date prefix. Text that is not a date is a different defect from an uncollected
value, and a specification may answer them differently. A `date_precision`
reading a value has only the first of the two to answer, because a value that
exists is already a value of its type.

**R016-53.** Neither operation answers about a `datetime`. A truncated moment
has no agreed completion -- an unknown time of day is not the same claim as an
unknown day -- so the collected text stays `str`. A `datetime` value is not a
`date_precision` source either, for the same reason it is not an operand of any
other date operation, and there would be nothing for it to report: a `datetime`
is collected in full or it is not a value.

## Operations

**R016-54.** R007 registers these operations, orders them, and nests them like
any other. Their input and result types are:

| Operation | Inputs | Result |
|---|---|---|
| `date_diff` | `start` and `end` are `date` | `int` |
| `study_day` | `date` and `reference` are `date` | `int`, never zero |
| `date_impute` | inputs in R016-42--53 | `date` |
| `date_precision` | `source` is `str` or `date` | `str` |
| `to_date` | `source` is `datetime` | `date` with collected precision `day` |

**R016-55.** Every temporal operation other than `to_date` is a date operation.
A `datetime` operand to one of those operations is an error rather than a
widened one. `date_diff` counts whole calendar units and its `bounds` field
counts endpoints of a day range, and neither has a meaning between two moments:
`unit: day` between `2025-01-01T23:00:00` and `2025-01-02T01:00:00` could be
`1` or `0`. Widening either operation would make that choice silently, so both
stay on `date`. A difference between two moments enters the vocabulary when an
example needs it.

**R016-56.** `date_impute` requires its `month` and a numeric `day` to lie
within its registered calendar ranges, and the completed value
must be a real calendar date. The range checks still apply when a component is
not used, so a specification cannot hide an invalid literal behind a precision
policy. A `day` naming a position in its month is not a literal to range-check,
and the calendar-date requirement cannot fail for one: it names whichever day
the target month begins or ends with rather than a number that month might not
have. Any other `day` token is neither a number nor a position, and is rejected
where the specification is read.

**R016-57.** A `datetime` is produced only by converting text. It is consumed
by comparisons or by `to_date`, which copies calendar fields and
drops time fields. A missing source returns a missing date. Any other source
type is the incompatible-input error R007 defines; in particular, a `date` is
not accepted as an identity spelling.

## Ingestion

**R016-58.** Every place text becomes a temporal value uses this text form.
R014 applies R011's `str` row, which reaches the grammar above. A field
declared `date` or `datetime` in a `types` declaration parses
exactly as a column conversion parses.


**R016-59.** The two paths differ only in what answers a bad value, which R014
fixes rather than this rule: an ingested value that does not parse is rejected
before any derivation, so no handler applies. A `str` field converted
at the column declaring a temporal type fails there, where
`conversion_failure` can answer. A specification that wants to see a malformed
value therefore leaves the field `str`, which is what `negative-datetime-zone-
offset` does.

## Rationale

Each runtime's own parser accepts a wider and a different set of spellings, so
the lexical form rejects all but one extended shape per type. Portability
costs exactly the rejected table. Zones and offsets are refused rather than
normalized because Python orders naive against aware datetimes by raising while
R has no naive datetime at all, and a CDISC `--DTC` value is local site time
anyway. Fractions are refused because Python stores whole microseconds while R
stores a binary64 count of seconds, and whole seconds are exact in both.
Collected precision travels with the value but takes no part in comparisons, so
every pair of values of one type stays ordered. A specification that acts on
imprecision bounds the imputation or states a verification instead.

## Errors

**R016-60.** Text that is not the lexical form above: not a temporal value. For
a `date` this includes a truncated date, a date carrying a time of day, and the
basic format; for a `datetime` it additionally includes a zone designator, an
offset, a fractional second, hour 24, and a leap second. Reaching a temporal
column, it is the conversion failure R011 defines, handled by
`conversion_failure` under R008 and otherwise fatal under R005. **R016-61.** A
date part that is not a date in the calendar, such as `2025-02-30`: the same
failure. **R016-62.** A year outside `0001` to `9999`: the same failure. The
four-digit field admits no other year, and that range is also the one Python's
`datetime` holds. **R016-63.** A conversion the table above marks `fail`,
including `date` to `datetime` and `datetime` to `date`: fail; choose none.
**R016-64.** Comparing or ordering a temporal value against another type:
fail under R007, which owns comparability. **R016-65.** A
date operation other than `to_date` given a `datetime`: fail rather than widen
the operation. This includes a `datetime` reaching `date_precision` as a value
source. **R016-66.** `to_date` given anything other than a `datetime`: fail as
an incompatible input. A missing `datetime` yields a missing date instead.
**R016-67.** `date_impute` whose `month` or numeric `day` is outside the
calendar range, or whose completed value is not a real calendar date: fail.
Neither can arise from a `day` naming a position in its month. **R016-68.**
`date_impute` whose `day` is a token that is neither a number nor a declared
position: rejected where the specification is read, before any data is seen.
**R016-69.** `date_impute` whose completed value cannot satisfy `not_before`
within the interval its collected components admit: missing, no failure. Like
a source below minimum precision, it is neither missing nor invalid
text, so no R008 handler answers it. **R016-70.** A temporal value used as an
operand in a `compute` expression: fail under R010, which admits only numeric
identifiers. **R016-71.** Storing a value no implementation can hold exactly,
such as a fractional or leap second, is never reached because text is rejected
first. An implementation must not round to reach such a value.

## Whole calendar units

**R016-72.** `date_diff` with `unit: day` is the calendar-date difference
`end` minus `start` in days. With `unit: week` it is the number of whole
seven-day blocks that difference holds: the quotient of the day count and
seven, with any remainder discarded.

**R016-73.** With `unit: month`, `date_diff` counts how many monthly
anniversaries of `start` fall on or before `end`. The k-th anniversary
carries the year and month k months after `start`, with its day clamped
to the length of that month. With `unit: year` it counts yearly
anniversaries the same way. Three boundary cases pin the rule:
`2025-01-31` to `2025-02-28` is one month, `2024-02-29` to `2025-02-28`
is twelve months and one year, and `2025-01-31` to `2025-03-01` is one
month, because the March anniversary of January 31 is March 31.

**R016-74.** A February 29 anniversary in a common year falls on February
28. This is the clamping the previous requirement already states, named
here because it is the case an age computation meets every leap year.

**R016-75.** When `end` precedes `start`, the result is the negation of
the count with the operands exchanged. An earlier date therefore
produces a negative result in every unit, and no unit rounds toward
negative infinity.

**R016-76.** `bounds` counts endpoints of a day range and is defined
only with `unit: day`. `exclusive` counts from `start` to `end`
excluding `start`; `inclusive` counts both endpoints and is one greater;
`between` counts neither and is one smaller. With `unit: week`,
`unit: month`, or `unit: year`, `bounds` must be absent or `exclusive`;
any other value has no meaning -- an age of 35 does not become 36 --
and is rejected where the specification is read, before any data is seen.

**R016-77.** A `date_diff` with a non-`exclusive` `bounds` beside a
non-`day` `unit` fails validation with condition `value_not_permitted`,
naming the offending `bounds` value and the permitted value
`exclusive`. Like every validation failure, no handler answers it and
no artifact is accepted.
