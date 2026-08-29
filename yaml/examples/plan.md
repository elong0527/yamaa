# Plan for schema improvements driven by the example suite

## Purpose

The suite holds 83 examples: 45 successful golden outputs and 38 expected
failures. Two failure examples also commit the completed dataset beside the
structured error. This file records the design gaps they expose, grouped by
root cause, and tracks the schema work that remains.
[`README.md`](README.md) is the reader-facing index of the examples
themselves.

Only open work appears here. A change that lands is deleted from this file
rather than marked, so the git history of this file and of
[`../rules/`](../rules/) is the record of what closed and how. Every open item
names the gap it closes, the evidence that justifies it, and the negative
example the acceptance rule requires.

## Open design gaps

Ten gaps are open across the suite, grouped by root cause rather than by the
example that found them, because most are consequences of a few underlying
decisions rather than independent omissions. A closed gap is deleted and the
list renumbered, so every number below is live.

### A. Literal operands

1. `cut.breaks` is a literal list, so banding criteria that are not
   proportional to a single reference cannot be written as one `cut`. The
   common case is not blocked. Criteria stated as multiples, as CTCAE states
   liver enzymes and creatinine, are a lookup for the reference limit,
   `compute` for the ratio, and `cut` with literal breaks and `right: true`,
   which puts the varying fact in data and the medical rule in the
   specification. Predicate bounds are not blocked either: `sql` compares one
   column with another, as `ASTDT >= TRTSDT` does in
   `adam-adae-treatment-emergent`. What remains is a criterion with no such
   reference, such as an absolute electrolyte threshold that differs per
   parameter, where each parameter needs its own break list.

   The same root cause appears where the formula varies rather than a bound.
   `sdtm-vs-unit-standardization` converts pounds by multiplication and
   Fahrenheit by an affine formula, so what differs per test is the shape of
   the expression, not an operand in it. `compute` takes a column in any
   operand position, so a multiplicative factor could be looked up, but nothing
   selects an expression from data. A row template per test is the workaround
   the example uses: it puts each formula beside the test it belongs to, at the
   cost of leaving rows grouped by test rather than in collection order.
   `sdtm-lb-ctcae-grading` shows the same growth for absolute haemoglobin
   thresholds that vary by sex.

A controlled vocabulary also still needs a `mapping` to give it a numeric proxy
before anything can order it, as `adam-adae-severity-rank` does for reported
severity. The order lives in a dictionary rather than in the vocabulary.

### B. Joins

2. There is no interval join, so a record cannot be matched against a table of
   per-subject intervals of irregular count and length, which is what an
   `EPOCH` derived from collected subject elements needs. Regular structure is
   not blocked: repeating intervals are arithmetic, so a treatment cycle is
   `FLOOR((ADY - 1) / 21) + 1` and its day is `MOD(ADY - 1, 21) + 1`, and a
   three-epoch design is a `case` chain over subject-level start and end dates
   that a lookup supplies. The gap is the irregular table, where the boundaries
   share no structure to compute against. `sdtm-vs-visit-study-day` leaves
   `EPOCH` empty for an unscheduled visit for this reason.

### C. Types, conversion, and missing-value semantics

3. Partial dates have no precision. `date_impute` declares the completion
   rule, so it is no longer string surgery, but the resulting `date` is
   indistinguishable from a collected one and a partial value still cannot be
   carried, compared, or verified as such. No example demonstrates this:
   `adam-adae-partial-dates` imputes without recording which component it
   supplied, so the cost is visible in `TRTEMFL` and nowhere in the artifact.
   The suite also covers only trailing precision loss,
   because the SDTM form for a known day in an unknown month needs an agreed
   representation before an example can assert it.
4. Imputed and collected dates compare identically. Nothing marks a comparison
   made under uncertainty, so an imputed day silently decides classifications
   such as treatment emergence.

### D. The output and pipeline contract stops at one dataset

5. One specification derives one dataset. `sdtm-suppmh-qualifiers` cannot
   assign a parent sequence and consume it in the same run, and
   `sdtm-dm-reference-dates` depends on DM being derived before the domains
   that reference it without being able to say so. R001 cycle detection is per
   specification, so a cross-dataset cycle cannot be reported either.
6. Nothing controls output row order, and verifications are row-wise over the
   completed output. Rows leave in row-template order rather than a submission
   sort order, and referential integrity between a SUPPQUAL record and its
   parent domain cannot be asserted. Nothing counts rows within a group
   either, so `sdtm-lb-conditional-compartments` cannot assert that every
   subject in an applicable cohort has both of its compartments.

### E. Structure that the data has cannot be declared

7. Conditional applicability, treatment period, relationship degree, and
   analysis window are all real structure in a protocol and none is a concept
   in the schema. Each is re-expressed as a filter, a literal in a predicate,
   or one row template per slot, so the specification grows with the data
   rather than describing the design. `sdtm-lb-conditional-compartments`,
   `adam-adsl-crossover-periods`, and `sdtm-relrec-many-to-many` each show a
   different face of this. The naming carries the structure instead: nothing
   links `adam-adsl-crossover-periods`'s `TRT02A` to its `TR02SDT` and
   `TR02EDT` except the `02`, so no implementation can check the grouping.
8. Row construction cannot consume values resolved during column derivation.
   A logically removed record cannot be dropped, because `row.filter` sees
   only the row driver and nothing deletes a row afterwards, as
   `sdtm-ae-effective-transaction` shows by committing a record that must not
   exist.
9. A derivation cannot carry both a value and the reason for it.
    `adam-adrs-composite-response` writes the same four predicates twice, once
    for the endpoint and once for its audit trail, with nothing linking them.
    The same example shows the related loss: whether a missing component means
    not evaluable or non-response is carried only by where a branch sits in the
    list, so no declaration states the policy and two studies cannot be
    compared without reading their branch order.
10. Metadata is an ungoverned string map. Labels are first class, but origin,
    length, and controlled terminology are free-form text that no
    implementation can validate, and no expected metadata artifact exists to
    assert them, as `sdtm-dm-metadata-contract` records.

## Open work

### T1. Banding criteria that are not proportional

`cut.breaks` is a literal list, so a parameter whose criteria are absolute
rather than multiples of a reference needs its own break list.

The scope is much narrower than it first looked, and no schema change is
obviously justified. A grading rule stated as multiples normalizes first and
cuts on literal breaks, so the data carries the reference limit and the
specification carries the rule. Predicate bounds were never restricted.

A `cut_from` reading bands from a keyed dataset was drafted and rejected. It
worked, but it moved the medical rule out of the specification and into a
reference table, so a reviewer had to open a CSV to learn what the criteria
were. Normalizing splits the two correctly. The remaining case is now concrete
in `sdtm-lb-ctcae-grading`: absolute haemoglobin limits differ by sex, so the
example repeats the bands for each sex.

Evidence: gap 1. No action until a second use shows that the repeated bands
justify a portable construct rather than a study-specific function.

### T6. Dates and times

Evidence: gaps 3 and 4. `sdtm-ae-effective-transaction` carries an audit
timestamp as `str` and orders it correctly only because ISO 8601 text sorts
chronologically.

The type vocabulary and the declared imputation rule are settled; precision is
not. A `date` produced by imputation is indistinguishable from a collected one,
so precision can only be recovered from the source text, and nothing marks a
comparison made against an imputed operand. A precision concept would close
both at once, and a `date_precision` expression would close the first alone.
Gap 4 is untouched.

No example now carries an imputation flag, so both halves are argued from the
rules rather than shown in golden output. An example that records which
component was supplied is the first thing this item needs.

`greatest` and `least` compare dates as ordinary comparable values. If a
precision concept lands here, a row-wise extreme over dates has to say whether
an imputed operand can win, so this item owns that decision rather than R007.

### T8. The output and pipeline contract

- Gap 5: one specification derives one dataset. `sdtm-suppmh-qualifiers`
  cannot assign a parent sequence and consume it in one run, and
  `sdtm-dm-reference-dates` depends on an execution order it cannot state.
  R001 cycle detection is per specification, so a cross-dataset cycle cannot be
  reported. Needs a manifest, cross-specification dependency inference, and
  cycle reporting.
- Gap 6: nothing controls output row order, and verifications are row-wise
  over the completed output. `sdtm-suppmh-qualifiers` leaves rows in
  row-template order rather than a submission order, and referential integrity
  between a SUPPQUAL record and its parent domain cannot be asserted.

### T9. Governed metadata

Evidence: gap 10. `sdtm-dm-metadata-contract` declares origin, length, and
codelist as free-form strings, marks `USUBJID` as `Derived` by hand although
`str_concat` already encodes that, and declares a codelist name next to an
unrelated `allowed_values` list.

Needs a vocabulary, a link between a declared codelist and its enforced values,
a length concept connected to the declared type, and an expected metadata
artifact. Until that artifact is defined, examples must not invent its shape.

### T10. Declarable study structure

Group E, gaps 7 to 9, and the largest open area.

- Conditional applicability, treatment period, relationship degree, and
  analysis window are protocol structure re-expressed as filters, predicate
  literals, and one row template per slot, so a specification grows with the
  data rather than describing the design.
- Row construction cannot consume values resolved during column derivation, so
  `sdtm-ae-effective-transaction` commits a record whose last transaction
  removed it.
- A derivation cannot carry both a value and the reason for it, so
  `adam-adrs-composite-response` writes the same four predicates twice.

Also here: gap 2, the absent interval join, which is what an analysis window or
an `EPOCH` assignment actually needs.

## Sequencing

1. **T6, T8, T9, T10** are design documents. Write the document before the
   schema change, and expect each to retire several gaps at once, as `compute`
   and `selections` did.
2. **T1** last, because its answer probably lies inside T10 rather than in a
   widened field.

Expected catalogue edits: T6 retires gaps 3 and 4, T8 retires gaps 5 and 6, T9
retires gap 10, and T10 retires gaps 2, 7, 8, and 9 along with whatever remains
of gap 1.

## Untested rule text

Every fail-closed claim in the rules is now provoked by an example, with one
exception: R013 rejects a `types` entry for a field whose container already
supplies a type, and every source in this suite is a delimited file that
supplies none. That claim stays rule text until the suite reads a container
carrying its own types, which is also what would let a dataset state its types
once for every specification that reads it.

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
