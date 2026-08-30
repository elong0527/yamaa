# Plan for schema improvements driven by the example suite

## Purpose

This file tracks the design gaps the example suite exposes and the schema work
that remains. [`README.md`](README.md) is the reader-facing index of the
examples themselves.

Only open work appears here. A change that lands is deleted rather than marked,
so the git history of this file and of [`../rules/`](../rules/) is the record
of what closed and how.

The suite currently holds 97 examples: 51 successful golden outputs and 46
expected failures. Four failure examples also commit the completed dataset
beside the structured error.

## Open work

Twelve design gaps remain, grouped under five work items. Each item states the
gap, the evidence from the suite, and what a solution requires.

### T1. Literal operands (gap 1)

**Gap.** `cut.breaks` is a literal list. A parameter whose criteria are
absolute rather than multiples of a reference needs its own break list, and an
expression shape that varies per row (e.g. affine vs. multiplicative
conversion) cannot be selected from data.

**Evidence.**

- `sdtm-lb-ctcae-grading` repeats absolute haemoglobin thresholds for each
  sex because the bands are not proportional to a reference limit.
- `sdtm-vs-unit-standardization` uses one row template per test because pounds
  convert by multiplication and Fahrenheit by an affine formula.
- `adam-adae-severity-rank` maps a controlled vocabulary through a dictionary
  to give it a numeric proxy before anything can order it.

**Scope.** Narrower than it first looked. Grading rules stated as multiples
normalize first and cut on literal breaks, putting the reference limit in data
and the medical rule in the specification. Predicate bounds were never
restricted (`sql` compares columns freely). A `cut_from` reading bands from a
keyed dataset was drafted and rejected: it moved the medical rule out of the
specification and into a reference table.

**Action.** No action until a second use shows that the repeated bands justify
a portable construct rather than a study-specific function. The answer may lie
inside T5 rather than in a widened field.

### T2. Dates and times (gaps 3–4)

**Gap 3.** A `date` value carries no precision. `date_precision` now reports
what was supplied, and `adam-adae-partial-dates` carries the flag ADaM expects,
but the date itself is still indistinguishable from a collected one. Nothing
binds a flag to the date it describes, so the two stay in step by convention.
The suite also covers only trailing precision loss, because a known day in an
unknown month needs an agreed representation before an example can assert it.

**Gap 4.** Imputed and collected dates compare identically. An imputed day
silently decides classifications such as treatment emergence.

**Evidence.**

- `adam-adae-partial-dates` derives `ASTDTF` from the same source it imputes
  from, but nothing prevents the flag and the date from drifting apart.
- `adam-adrs-overall-response-records` completes an assessment date collected
  without a day and carries no flag at all, so the completed `ADT` is a date
  like any other. Every later comparison — study day, an eligibility window,
  which assessment came first — treats a chosen day as a collected one.

**Action.** A precision attached to the value would close both gaps. This is a
change to the type vocabulary rather than a registry entry. If a precision
concept lands here, `greatest` and `least` must say whether an imputed operand
can win, so this item owns that decision rather than R007.

**Also open here, from the datetime type.** R016 closed the time of day, and
three consequences of how it closed it stay with this item. Each is a question
the type raises rather than a limitation an example has hit, so none is a gap
of its own:

- Nothing converts between a `date` and a `datetime`. R016 fails in both
  directions rather than inventing a time of day or discarding a collected
  one, so a specification holding a moment and needing a day has no expression
  to ask for it.
- Nothing measures between two moments. `date_diff` counts whole calendar
  units and its `bounds` counts endpoints of a day range, and neither has a
  meaning between two instants, so R016 keeps both operations on `date`.
- A time of day alone is still not a value. R016 requires a complete date
  beside the time, so the `--TM` family stays text.

Each enters the vocabulary when an example needs it, under the acceptance rule
below.

### T3. Output and pipeline contract (gaps 5–6)

**Gap 5.** One specification derives one dataset. `sdtm-suppmh-qualifiers`
cannot assign a parent sequence and consume it in the same run, and
`sdtm-dm-reference-dates` depends on DM being derived before the domains that
reference it without being able to say so. R001 cycle detection is per
specification, so a cross-dataset cycle cannot be reported either.

**Gap 6.** Nothing controls output row order, and verifications are row-wise
over the completed output. `keys` states which variables identify a row and
nothing states the order the rows leave in, so a domain contract can declare
only half of itself here: rows leave in row-template order rather than a
submission sort order, and referential integrity between a SUPPQUAL record and
its parent domain cannot be asserted. Nothing counts rows within a group
either, so `sdtm-lb-conditional-compartments` cannot assert that every subject
in an applicable cohort has both of its compartments. An assertion over an
ordered series reaches one neighbour and no further:
`negative-adrs-partial-response-after-complete-response` rejects a partial
response directly after a complete one, and the same fault with an intervening
assessment passes.

**Evidence.** Ordering within an operation fails closed, and examples provoke
it: `negative-record-lookup-unordered-keep` rejects records ordered but not
chosen, and `negative-baseline-flag-tied-date` rejects a selection whose
candidates no declared order separates. The artifact's own row order is the
one ordering no example reaches, because no field declares it, so it is
recorded here rather than shown.

**Action.** Needs a manifest, cross-specification dependency inference, cycle
reporting, and an output sort-order declaration. Write a design document before
the schema change.

### T4. Governed metadata (gap 10)

**Gap.** Metadata is an ungoverned string map. Labels are first class and a
value's length is now enforceable, but origin, the declared lengths, and
controlled terminology are free-form text that no implementation can validate,
and no expected metadata artifact exists to assert them.

**Evidence.** `sdtm-dm-metadata-contract` declares origin, length, and
codelist as free-form strings, marks `USUBJID` as `Derived` by hand although
`str_concat` already encodes that, and declares a codelist name next to an
unrelated `allowed_values` list. Its `USUBJID` now carries both a declared
length and a `max_length` check, and nothing requires the two to agree.

**Action.** Needs a vocabulary, a link between a declared codelist and its
enforced values, a link between a declared length and the `max_length` that
enforces it, and an expected metadata artifact. `max_length` supplies the
enforced half; what remains is binding the declared half to it. Until that
artifact is defined, examples must not invent its shape.

### T5. Declarable study structure (gaps 2, 7–9, 11–12)

**Gap 2.** There is no interval join, so a record cannot be matched against a
table of per-subject intervals of irregular count and length. Regular structure
is not blocked: repeating intervals are arithmetic (e.g.
`FLOOR((ADY - 1) / 21) + 1` for a treatment cycle), and a three-epoch design
is a `case` chain over subject-level dates. The gap is the irregular table.
`sdtm-vs-visit-study-day` leaves `EPOCH` empty for an unscheduled visit for
this reason.

**Gap 7.** Conditional applicability, treatment period, relationship degree,
and analysis window are all real structure in a protocol and none is a concept
in the schema. Each is re-expressed as a filter, a literal in a predicate, or
one row template per slot, so the specification grows with the data rather than
describing the design. `sdtm-lb-conditional-compartments`,
`adam-adsl-crossover-periods`, and `sdtm-relrec-many-to-many` each show a
different face of this. Naming carries the structure instead: nothing links
`adam-adsl-crossover-periods`'s `TRT02A` to its `TR02SDT` and `TR02EDT` except
the `02`, so no implementation can check the grouping.

**Gap 8.** Row construction cannot consume values resolved during column
derivation. A logically removed record cannot be dropped, because `row.filter`
sees only the row driver and nothing deletes a row afterwards.
`sdtm-ae-effective-transaction` commits a record that must not exist.

**Gap 9.** A derivation cannot carry both a value and the reason for it.
`adam-adrs-composite-response` writes the same four predicates twice, once for
the endpoint and once for its audit trail, with nothing linking them. Whether a
missing component means not evaluable or non-response is carried only by where
a branch sits in the list, so no declaration states the policy and two studies
cannot be compared without reading their branch order.
`adam-adrs-best-overall-response` shows the cost in a published definition:
which response wins, and whether an assessment that came too early leaves the
subject progressive or not evaluable, is carried by the order of the branches
and by nothing a reader can check. `adam-adtte-duration-of-response` shows it
against a submission requirement: selection returns the winning date and not
the record that supplied it, so `EVNTDESC`, `CNSDTDSC`, and the `SRCDOM`,
`SRCVAR`, and `SRCSEQ` triplet are each rebuilt by a parallel `case` chain
over the value that won, and nothing ties the triplet to the date it claims to
trace.

**Gap 11.** A right-side match or reduction cannot be narrowed by a value from
the current output row. Its filter sees only right-side records, so a
subject-specific cutoff derived on the left cannot restrict the records being
summarized. The deferred `negative-adrs-response-before-progression` case
therefore cannot limit response assessments to those on or before that
subject's first progression.

**Gap 12.** A reduction cannot consume another reduction. R013 closes nesting
rather than leaving it implementation-defined, so grouping `EX` by subject and
cycle, totalling each, and then taking the largest across cycles needs an
intermediate grain no expression can name. `adam-adtr-sum-of-target-diameters`
is where the suite stops at that first level: it sums the target lesion
diameters at each assessment and flags the assessments that measured every
target lesion. The lowest of a subject's earlier flagged sums — the nadir
RECIST measures progression against — needs the per-assessment grain named,
and gap 11 besides, because *earlier* is relative to the row being derived. A
design that gives a reduction a grain of its own would retire this with gap
11.

**Action.** The largest open area. Write a design document before the schema
change, and expect it to retire several gaps at once, as `compute` and
`record_lookups` did. The interval join (gap 2), row-relative matching (gap
11), and a nameable intermediate grain (gap 12) are the comparison frames that
analysis windows, `EPOCH` assignment, subject-specific cutoffs, and two-level
reductions need.

## Sequencing

1. **T2, T3, T4, T5** are design documents. Write the document before the
   schema change, and expect each to retire several gaps at once.
2. **T1** last, because its answer probably lies inside T5 rather than in a
   widened field.

Expected catalogue edits: T2 retires gaps 3 and 4, T3 retires gaps 5 and 6,
T4 retires gap 10, and T5 retires gaps 2, 7, 8, 9, 11, and 12 along with
whatever remains of gap 1.

## Untested rule text

Every fail-closed contract targeted by the completed negative-example audit is
now provoked by an example, with two exceptions.

R014 rejects a `types` entry for a field whose container already supplies a
type, and every source in this suite is a delimited file that supplies none.
That claim stays rule text until the suite reads a container carrying its own
types, which is also what would let a dataset state its types once for every
specification that reads it.

R016's rejection table has fourteen rows and the suite provokes two of them:
`negative-datetime-zone-offset` rejects an offset, which is the row the type
turns on, and `negative-conversion-incomplete-date` rejects a truncated date.
The remaining rows are one grammar failing one way each, so they stay rule
text rather than becoming twelve examples of the same rejection.

## Pilot 7 coverage

The live examples and open-gap catalogue retain every actionable finding
from the
[RConsortium pilot 7 synthetic-data](https://github.com/RConsortium/submissions-pilot7-synthetic-data)
survey, and fields that the source declares but never varies are not used as
evidence for a rule. Two of its ideas remain undemonstrated: the irregular
`EPOCH` interval join, which is gap 2, and a questionnaire scale score, which
is a row-wise reduction over columns rather than over a partition and has no
item-level input in the pilot source to demonstrate.

## Acceptance rule for adding a schema feature

A feature should enter the portable vocabulary only when at least one positive
example needs it, a negative or edge example fixes its failure behavior, and R
and Python can implement the same semantics. Sponsor-specific algorithms should
remain behind `function`; common CDISC operations demonstrated by multiple
examples should become closed, documented expressions instead.
