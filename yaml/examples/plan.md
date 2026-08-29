# Plan for schema improvements driven by the example suite

## Purpose

This file tracks the design gaps the example suite exposes and the schema work
that remains. [`README.md`](README.md) is the reader-facing index of the
examples themselves.

Only open work appears here. A change that lands is deleted rather than marked,
so the git history of this file and of [`../rules/`](../rules/) is the record
of what closed and how.

The suite currently holds 93 examples: 50 successful golden outputs and 43
expected failures. Four failure examples also commit the completed dataset
beside the structured error.

## Open work

Ten design gaps remain, grouped under five work items. Each item states the
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

- `sdtm-ae-effective-transaction` carries an audit timestamp as `str` and
  orders it correctly only because ISO 8601 text sorts chronologically.
- `adam-adae-partial-dates` derives `ASTDTF` from the same source it imputes
  from, but nothing prevents the flag and the date from drifting apart.

**Action.** A precision attached to the value would close both gaps. This is a
change to the type vocabulary rather than a registry entry. If a precision
concept lands here, `greatest` and `least` must say whether an imputed operand
can win, so this item owns that decision rather than R007.

### T3. Output and pipeline contract (gaps 5–6)

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

**Action.** Needs a manifest, cross-specification dependency inference, cycle
reporting, and an output sort-order declaration. Write a design document before
the schema change.

### T4. Governed metadata (gap 10)

**Gap.** Metadata is an ungoverned string map. Labels are first class, but
origin, length, and controlled terminology are free-form text that no
implementation can validate, and no expected metadata artifact exists to assert
them.

**Evidence.** `sdtm-dm-metadata-contract` declares origin, length, and
codelist as free-form strings, marks `USUBJID` as `Derived` by hand although
`str_concat` already encodes that, and declares a codelist name next to an
unrelated `allowed_values` list.

**Action.** Needs a vocabulary, a link between a declared codelist and its
enforced values, a length concept connected to the declared type, and an
expected metadata artifact. Until that artifact is defined, examples must not
invent its shape.

### T5. Declarable study structure (gaps 2, 7–9)

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
and by nothing a reader can check.

**Action.** The largest open area. Write a design document before the schema
change, and expect it to retire several gaps at once, as `compute` and
`selections` did. The interval join (gap 2) is what an analysis window or
`EPOCH` assignment actually needs.

## Sequencing

1. **T2, T3, T4, T5** are design documents. Write the document before the
   schema change, and expect each to retire several gaps at once.
2. **T1** last, because its answer probably lies inside T5 rather than in a
   widened field.

Expected catalogue edits: T2 retires gaps 3 and 4, T3 retires gaps 5 and 6,
T4 retires gap 10, and T5 retires gaps 2, 7, 8, and 9 along with whatever
remains of gap 1.

## Untested rule text

Every fail-closed contract targeted by the completed negative-example audit is
now provoked by an example, with one exception: R014 rejects a `types` entry
for a field whose container already supplies a type, and every source in this
suite is a delimited file that supplies none. That claim stays rule text until
the suite reads a container carrying its own types, which is also what would
let a dataset state its types once for every specification that reads it.

## Pilot 7 coverage

The live examples retain every actionable finding from the
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
