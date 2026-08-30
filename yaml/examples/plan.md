# Plan for schema improvements driven by the example suite

## Purpose

This file tracks the design gaps the example suite exposes and the schema work
that remains. [`README.md`](README.md) is the reader-facing index of the
examples themselves.

Only open work appears here. A change that lands is deleted rather than marked,
so the git history of this file and of [`../rules/`](../rules/) is the record
of what closed and how.

The suite currently holds 105 examples: 54 successful golden outputs and 51
expected failures. Seven failure examples also commit a CSV beside the
structured error: five record the dataset presented to the failing check, and
two record the artifact the missing capability would produce.

## Open work

Seventeen design gaps remain, grouped under six work items. Each item states
the gap, the evidence from the suite, and what a solution requires.

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

**Also settled.** The obvious benchmark answer is unavailable here.
`pharmaverse/admiral` grades laboratory results from a criteria dataset whose
columns hold executable R, and varies a derivation's arguments by a filter the
same way. Both put host-language code, or a choice of expression, into data,
which is what `cut_from` was rejected for and what the version 1.0 boundary
confines to `function`. What the benchmark does show is that the varying part
is a criterion set keyed by a group rather than an operand in a formula, and
that the published form of `sdtm-lb-ctcae-grading` runs to hundreds of rows and
is unwritable by repetition.

**Action.** No action until a second use shows that the repeated bands justify
a portable construct rather than a study-specific function. The answer may lie
inside T5 rather than in a widened field. Anything that does land is a keyed
table of declared bands, with the comparison still in the specification, and
never a predicate carried as data, so the open question is whether a portable
form exists at all rather than whether to adopt the benchmark's.

### T2. Dates and times (gaps 3–4, 17)

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

**Gap 17.** Imputation cannot be bounded, stated relatively, or held back.
`date_impute` takes a literal `month` and a literal `day`, both required, and
three things a partial date routinely needs have no form:

- *how much may be supplied.* Both fields are required, so a source carrying
  only a year is always completed. `adam-adae-partial-dates` cannot say
  "supply a missing day and leave a year-only value missing", which is the
  usual protocol rule for an event start.
- *a rule that resolves against the month it lands in.* Placing a partial date
  on the last day of its month is `day: 31` against a February source, which
  `negative-date-impute-nonexistent-day` rejects as the calendar-range error it
  is. End-of-month imputation is therefore one row template per month rather
  than one statement.
- *a bound.* Nothing keeps an imputed event start from preceding the treatment
  start it will be classified against. The bound has to apply only within the
  range the missing components could take, so collected information is never
  overwritten, which is what makes it a rule rather than a comparison a
  specification could write itself.

**Action.** All three still return a `date` from a `date`, so each widens
`date_impute` rather than adding an entry, as `date_diff.bounds` did. The
calendar-range and ceiling counter-examples exist or are one edit away, so this
is the smallest piece of open work here and does not wait on the precision
decision above.

### T3. Output and pipeline contract (gaps 5–6, 14–15)

**Gap 5.** One specification derives one dataset. `sdtm-suppmh-qualifiers`
cannot assign a parent sequence and consume it in the same run, and
`sdtm-dm-reference-dates` depends on DM being derived before the domains that
reference it without being able to say so. R001 cycle detection is per
specification, so a cross-dataset cycle cannot be reported either.

**Gap 6.** Nothing controls output row order, and verifications are row-wise
over the completed output. Rows leave in row-template order rather than a
submission sort order, and referential integrity between a SUPPQUAL record and
its parent domain cannot be asserted. Nothing counts rows within a group
either, so `sdtm-lb-conditional-compartments` cannot assert that every subject
in an applicable cohort has both of its compartments. An assertion over an
ordered series reaches one neighbour and no further:
`negative-adrs-partial-response-after-complete-response` rejects a partial
response directly after a complete one, and the same fault with an intervening
assessment passes.

**Gap 14.** A run cannot read the dataset it is producing. A parameter derived
from the analysis values of other parameters — a ratio between two
transaminases, a mean arterial pressure from two blood pressures — reads
records the same run is building, and nothing addresses one by key. The output
`domain` cannot be a dataset identifier, so a keyed lookup cannot name it, and
reducing the constructed rows instead reaches the very value being defined,
which R001 reports as a cycle.
`negative-source-output-self-reference` shows the naming prohibition and
`negative-adlb-computed-parameter` shows the cycle; both commit the artifact
the capability would produce.

**Gap 15.** Reshaping runs one way only. `rows` builds records out of one
record's fields, and nothing goes the other way, so a keyed set of records
cannot become fields of one row. `sdtm-suppmh-qualifiers` and
`sdtm-suppmh-parent-linkage` produce supplemental qualifiers and no example
consumes them, which is half of a submission-standard round trip.
`adam-adqs-subscale-score` scores a questionnaire held as one record per item;
the same instrument held as one field per item cannot be scored at all, because
nothing counts the answered ones across a row.

**Action.** Needs a manifest, cross-specification dependency inference, cycle
reporting, and an output sort-order declaration. It must also say what a run
may read of itself (gap 14) and whether a keyed set of records can become
fields (gap 15); a transpose is the first workaround a reviewer proposes for
gap 14, so the two are decided together. Write a design document before the
schema change.

### T4. Governed metadata (gap 10)

**Gap.** Metadata is an ungoverned string map. Labels are first class and a
value's length is now enforceable, but origin, the declared lengths, and
controlled terminology are free-form text that no implementation can validate,
and no expected metadata artifact exists to assert them. Two further
attributes cannot be written at all: a variable's core designation and its
display format have no field and no settled metadata key, and the dataset's
own description sits in the same ungoverned map as its class and structure.

**Evidence.** `sdtm-dm-metadata-contract` declares origin, length, and
codelist as free-form strings, marks `USUBJID` as `Derived` by hand although
`str_concat` already encodes that, and declares a codelist name next to an
unrelated `allowed_values` list. Its `USUBJID` now carries both a declared
length and a `max_length` check, and nothing requires the two to agree. No
example declares a core designation or a display format, and the superseded
`R/cdiscbuilder/inst/specs/adam/schema.yaml` governed `core` with a closed
value list, so this vocabulary is narrower than the one it replaced.

**Action.** Needs a vocabulary naming the attributes it governs — at least
origin, length, codelist, display format, core, and the dataset's own
description — a link between a declared codelist and its enforced values, a
link between a declared length and the `max_length` that enforces it, and an
expected metadata artifact. `max_length` supplies the enforced half; what
remains is binding the declared half to it. Until that artifact is defined,
examples must not invent its shape.

### T5. Declarable study structure (gaps 2, 7–9, 11–12, 16)

**Gap 2.** There is no interval join, so a record cannot be matched against a
table of per-subject intervals of irregular count and length. Regular structure
is not blocked: repeating intervals are arithmetic (e.g.
`FLOOR((ADY - 1) / 21) + 1` for a treatment cycle), and a three-epoch design
is a `case` chain over subject-level dates. The gap is the irregular table.
`sdtm-vs-visit-study-day` leaves `EPOCH` empty for an unscheduled visit for
this reason, and `negative-advs-analysis-window-table` shows the study-level
face of the same match: a table of analysis windows is one table for every
subject and is still unreachable, because the bound compared against is a
value of the record being classified.

**Gap 7.** Conditional applicability, treatment period, relationship degree,
and analysis window are all real structure in a protocol and none is a concept
in the schema. Each is re-expressed as a filter, a literal in a predicate, or
one row template per slot, so the specification grows with the data rather than
describing the design. `sdtm-lb-conditional-compartments`,
`adam-adsl-crossover-periods`, and `sdtm-relrec-many-to-many` each show a
different face of this. Naming carries the structure instead: nothing links
`adam-adsl-crossover-periods`'s `TRT02A` to its `TR02SDT` and `TR02EDT` except
the `02`, so no implementation can check the grouping.

The numbered families CDISC uses for the same purpose are the other half of
this. `adam-adae-query-flags` declares one set of variables per query the study
reports, and how many sets a study needs is a property of its query dictionary
rather than of the specification; `negative-query-slot-overflow` shows what
happens when the dictionary outgrows what the specification declared. R005 now
states that the artifact's column list is declared, so a family's members are
declared columns like any other and their count is fixed when the specification
is written. Whether that stays true, or a declared family takes its members
from data, is the fork this gap has to answer.

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
subject's first progression. `negative-advs-analysis-window-table` is the
committed case: a record's analysis window is the one whose day range contains
that record's study day, and the day is a value of the left row, so the study's
window table cannot be matched at all. Writing the boundaries as a literal
list, as `adam-advs-analysis-visit` does, is what works today, and it puts the
protocol's window table in the specification, where no implementation can check
it against the study's own metadata.

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

**Gap 16.** Row construction is append-only and its count is fixed by its
driver. A `rows` entry appends one row per driver record, so a record standing
for a data-dependent number of administrations cannot be expanded into them:
`negative-adex-single-dose-expansion` writes one row template per
administration and loses every administration past the last template it
declares, which is what its committed error reports. The opposite motion is the
same decision — adding a row only where an expected combination has none,
which is what carrying a value forward to an unattended visit needs — and gap
8 is its third face, so one design document answers all three.

**Action.** The largest open area. Write a design document before the schema
change, and expect it to retire several gaps at once, as `compute` and
`record_lookups` did. The interval join (gap 2), row-relative matching (gap
11), and a nameable intermediate grain (gap 12) are the comparison frames that
analysis windows, `EPOCH` assignment, subject-specific cutoffs, and two-level
reductions need.

### T6. Specification inheritance (gap 13)

**Gap.** `root.parents` names the specifications a specification inherits
from, and no rule defines what inheriting does. Nothing states which fields
merge, how a child overrides or removes an inherited column, or what a name
declared by two parents resolves to, so an implementation reading the field
would have to invent all of it.

**Evidence.** No rule mentions `parents` and no example declares it; R006
quotes the field only as an illustration of quoting a bracketed type
expression. The superseded `R/cdiscbuilder/inst/specs/adam/schema.yaml` carried
the same field beside a per-column `drop` flag for removing an inherited
column, so the merge it implies was once partly written down. The design
picture in the repository README rests on it: organization, compound, and
study templates reach a study specification by inheritance and by nothing
else.

**Action.** Either define the merge or remove the field. A validating
implementation today accepts `parents` and can do nothing with it, so the
field is structure that R006 validates and no rule interprets. Decide before
an example declares it.

## Sequencing

1. **T6** first, because it decides the fate of a field that already exists
   rather than adding a capability, and either answer is small.
2. **T2, T3, T4, T5** are design documents. Write the document before the
   schema change, and expect each to retire several gaps at once.
3. **T1** last, because its answer probably lies inside T5 rather than in a
   widened field.

Expected catalogue edits: T2 retires gaps 3, 4, and 17, T3 retires gaps 5, 6,
14, and 15, T4 retires gap 10, T5 retires gaps 2, 7, 8, 9, 11, 12, and 16 along
with whatever remains of gap 1, and T6 retires gap 13.

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
evidence for a rule. One of its ideas remains undemonstrated: the irregular
`EPOCH` interval join, which is gap 2. The questionnaire scale score is now
shown by `adam-adqs-subscale-score`, which supplies the item-level input the
pilot source lacks; scoring the same instrument from one field per item rather
than one record per item stays with gap 15.

## Acceptance rule for adding a schema feature

A feature should enter the portable vocabulary only when at least one positive
example needs it, a negative or edge example fixes its failure behavior, and R
and Python can implement the same semantics. Sponsor-specific algorithms should
remain behind `function`; common CDISC operations demonstrated by multiple
examples should become closed, documented expressions instead.
