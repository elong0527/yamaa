# Plan for schema improvements driven by the example suite

## Purpose

The suite holds 47 examples: 42 successful golden outputs and five expected
failures. One failure example also commits the completed dataset beside the
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

Fifteen gaps are open across the suite, grouped by root cause rather than by
the example that found them, because most are consequences of a few underlying
decisions rather than independent omissions. A closed gap is deleted and the
list renumbered, so every number below is live.

### A. Literal operands and ordering

1. `cut.breaks` is a literal list, so banding criteria that are not
   proportional to a single reference cannot be written as one `cut`. The
   common case is not blocked. Criteria stated as multiples, as CTCAE states
   liver enzymes and creatinine, are `mapping_from` for the reference limit,
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
2. Only `row_number` is registered. Without `rank` and `dense_rank` a tie
   cannot carry a rank number, so distinct-level counts, and any rule whose
   output is the rank itself, cannot be expressed.
   `adam-adae-worst-severity` has two events tied on severity and date and
   numbers them 1 and 2. Flagging every record tied at a worst value is a
   separate question and is not blocked: R007 lets `max` declare `group_by`,
   reduce constructed output rows within each partition, and broadcast the
   result, so a predicate comparing each row with that value flags the whole
   tied set. That example flags one record because that is the rule it models.

A controlled vocabulary also still needs a `mapping` to give it a numeric proxy
before anything can order it. The order lives in a dictionary rather than in
the vocabulary.

### B. Joins

3. `mapping_from` returns one column per call, so reading several columns from
   one matched record repeats the match. `sdtm-vs-visit-study-day` calls it
   twice against one `TV` row, and `sdtm-lb-reference-range-indicator` calls it
   three times for the unit and two bounds. A multi-column return conflicts
   with one expression producing one value and belongs with gaps 5 and 6.
4. There is no interval join, so a record cannot be matched against a table of
   per-subject intervals of irregular count and length, which is what an
   `EPOCH` derived from collected subject elements needs. Regular structure is
   not blocked: repeating intervals are arithmetic, so a treatment cycle is
   `FLOOR((ADY - 1) / 21) + 1` and its day is `MOD(ADY - 1, 21) + 1`, and a
   three-epoch design is a `case` chain over subject-level start and end dates
   that `mapping_from` supplies. The gap is the irregular table, where the
   boundaries share no structure to compute against. `sdtm-vs-visit-study-day`
   leaves `EPOCH` empty for an unscheduled visit for this reason.

### C. Aggregates and selection operate on values, not rows

5. An extreme value and the values associated with it come from two independent
   reductions that nothing ties to the same right-side record. A shared
   `filter` can make them see the same records, not the same one.
   `sdtm-dm-reference-dates` takes the last exposure end date and the dose
   given at it as two separate selections, and keeps the second as an internal
   column solely to show that they agree only because both order the same way.
   `adam-adtte-progression-free-survival` makes the consequence submission-
   facing: its event date and `SRCDOM`/`SRCVAR`/`SRCSEQ` traceability are
   independently selected, so nothing guarantees that the value and identity
   came from one record.
6. A missing aggregate result cannot distinguish no matching record from
   matching records whose values are all missing. In
   `sdtm-dm-reference-dates` a subject who was never exposed and one whose
   exposure dates were never collected produce the same empty reference dates.

### D. Types, conversion, and missing-value semantics

7. Source-format missing values and type inference have no normative rule.
   Every example assumes an empty CSV field is missing and distinguishes it
   from a nonempty malformed value.
8. Partial dates have no precision. `date_impute` declares the completion
   rule, so it is no longer string surgery, but the resulting `date` is
   indistinguishable from a collected one and a partial value still cannot be
   carried, compared, or verified as such. No example demonstrates this:
   `adam-adae-partial-dates` imputes without recording which component it
   supplied, so the cost is visible in `TRTEMFL` and nowhere in the artifact.
   The suite also covers only trailing precision loss,
   because the SDTM form for a known day in an unknown month needs an agreed
   representation before an example can assert it.
9. Imputed and collected dates compare identically. Nothing marks a comparison
   made under uncertainty, so an imputed day silently decides classifications
   such as treatment emergence.

### E. The output and pipeline contract stops at one dataset

10. One specification derives one dataset. `sdtm-suppmh-qualifiers` cannot
    assign a parent sequence and consume it in the same run, and
    `sdtm-dm-reference-dates` depends on DM being derived before the domains
    that reference it without being able to say so. R001 cycle detection is per
    specification, so a cross-dataset cycle cannot be reported either.
11. Nothing controls output row order, and verifications are row-wise over the
    completed output. Rows leave in row-template order rather than a submission
    sort order, and referential integrity between a SUPPQUAL record and its
    parent domain cannot be asserted. Nothing counts rows within a group
    either, so `sdtm-lb-conditional-compartments` cannot assert that every
    subject in an applicable cohort has both of its compartments.

### F. Structure that the data has cannot be declared

12. Conditional applicability, treatment period, relationship degree, and
    analysis window are all real structure in a protocol and none is a concept
    in the schema. Each is re-expressed as a filter, a literal in a predicate,
    or one row template per slot, so the specification grows with the data
    rather than describing the design. `sdtm-lb-conditional-compartments`,
    `adam-adsl-crossover-periods`, and `sdtm-relrec-many-to-many` each show a
    different face of this. The naming carries the structure instead: nothing
    links `adam-adsl-crossover-periods`'s `TRT02A` to its `TR02SDT` and
    `TR02EDT` except the `02`, so no implementation can check the grouping.
13. Row construction cannot consume values resolved during column derivation.
    A logically removed record cannot be dropped, because `row.filter` sees
    only the row driver and nothing deletes a row afterwards, as
    `sdtm-ae-effective-transaction` shows by committing a record that must not
    exist.
14. A derivation cannot carry both a value and the reason for it.
    `adam-adrs-composite-response` writes the same four predicates twice, once
    for the endpoint and once for its audit trail, with nothing linking them.
    The same example shows the related loss: whether a missing component means
    not evaluable or non-response is carried only by where a branch sits in the
    list, so no declaration states the policy and two studies cannot be
    compared without reading their branch order.
15. Metadata is an ungoverned string map. Labels are first class, but origin,
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

### T2. `rank` and `dense_rank`

Registry entries with the same fields as `row_number`.

Evidence: gap 2. `adam-adae-worst-severity` has two events tied on severity and
date, and `row_number` numbers them 1 and 2, so a rule whose output is the rank
itself cannot preserve the tie. Distinct-level counts are unavailable for the
same reason.

Flagging every record tied at a worst value is not evidence for this item. R007
already lets `max` declare `group_by`, reduce output rows within a partition,
and broadcast the result, so a predicate against that value flags the whole
tied set without a new expression.

Decision it forces: tie semantics for a rank number.

Negative example: ranking on a column whose ordering is not total.

### T5. Selection that returns a record

Two gaps have the same cause: an expression selects a value, never a row.

- Gap 5: `sdtm-dm-reference-dates` derives an extreme date with `max` and its
  associated dose with an ordered `source`, and nothing ties them to the same
  EX record. `sdtm-ae-effective-transaction` runs four independent selections
  that agree only because all four declare the same ordering.
  `adam-adtte-progression-free-survival` independently selects its endpoint
  date and traceability sequence, even though ADaM requires them to identify
  one source record.
- Gap 6: a missing aggregate cannot distinguish no matching record from
  matching records whose values are all missing.

`filter` on `multiple_matches` narrowed this but did not close it: two
selections can now be made to see the same records without being tied to the
same one. A construct that selects a right-side record once and reads several
columns from it would close both. This is design work, not a registry entry.

### T6. Dates and times

Evidence: gaps 8 and 9. `sdtm-ae-effective-transaction` carries an audit
timestamp as `str` and orders it correctly only because ISO 8601 text sorts
chronologically.

The type vocabulary and the declared imputation rule are settled; precision is
not. A `date` produced by imputation is indistinguishable from a collected one,
so precision can only be recovered from the source text, and nothing marks a
comparison made against an imputed operand. A precision concept would close
both at once, and a `date_precision` expression would close the first alone.
Gap 9 is untouched.

No example now carries an imputation flag, so both halves are argued from the
rules rather than shown in golden output. An example that records which
component was supplied is the first thing this item needs.

`greatest` and `least` compare dates as ordinary comparable values. If a
precision concept lands here, a row-wise extreme over dates has to say whether
an imputed operand can win, so this item owns that decision rather than R007.

### T7. Source-format ingestion

Gap 7: source-format missing values and type inference have no normative rule.
Every example assumes an empty CSV field is missing and distinguishes it from a
nonempty malformed value, and nothing says so.

Conversion is defined by R011's matrix. Ingestion is the harder half and is
untouched, because it is about recognising a value rather than rendering one.
It is rule text rather than schema, and it is required before any
implementation can claim R and Python parity.

### T8. The output and pipeline contract

- Gap 10: one specification derives one dataset. `sdtm-suppmh-qualifiers`
  cannot assign a parent sequence and consume it in one run, and
  `sdtm-dm-reference-dates` depends on an execution order it cannot state.
  R001 cycle detection is per specification, so a cross-dataset cycle cannot be
  reported. Needs a manifest, cross-specification dependency inference, and
  cycle reporting.
- Gap 11: nothing controls output row order, and verifications are row-wise
  over the completed output. `sdtm-suppmh-qualifiers` leaves rows in
  row-template order rather than a submission order, and referential integrity
  between a SUPPQUAL record and its parent domain cannot be asserted.

### T9. Governed metadata

Evidence: gap 15. `sdtm-dm-metadata-contract` declares origin, length, and
codelist as free-form strings, marks `USUBJID` as `Derived` by hand although
`str_concat` already encodes that, and declares a codelist name next to an
unrelated `allowed_values` list.

Needs a vocabulary, a link between a declared codelist and its enforced values,
a length concept connected to the declared type, and an expected metadata
artifact. Until that artifact is defined, examples must not invent its shape.

### T10. Declarable study structure

Group F, gaps 12 to 14, and the largest open area.

- Conditional applicability, treatment period, relationship degree, and
  analysis window are protocol structure re-expressed as filters, predicate
  literals, and one row template per slot, so a specification grows with the
  data rather than describing the design.
- Row construction cannot consume values resolved during column derivation, so
  `sdtm-ae-effective-transaction` commits a record whose last transaction
  removed it.
- A derivation cannot carry both a value and the reason for it, so
  `adam-adrs-composite-response` writes the same four predicates twice.

Also here: gap 4, the absent interval join, which is what an analysis window or
an `EPOCH` assignment actually needs.

## Sequencing

1. **T2**, with its negative example. It is registry work with committed
   evidence and bounded semantics.
2. **T7**, which is rule text rather than schema, and is required before any
   implementation can claim R and Python parity.
3. **T5, T6, T8, T9, T10** are design documents. Write the document before the
   schema change, and expect each to retire several gaps at once, as `compute`
   did.
4. **T1** last, because its answer probably lies inside T10 rather than in a
   widened field.

Expected catalogue edits: T2 retires gap 2, T5 retires gaps 3, 5, and 6, T6
retires gaps 8 and 9, T7 retires gap 7, T8 retires gaps 10 and 11, T9 retires
gap 15, and T10 retires gaps 4, 12, 13, and 14 along with whatever remains of
gap 1.

## Negative examples this plan requires

The acceptance rule needs failure behavior fixed before a feature is added.
Five expected-failure examples now establish a self-referential ordered window,
duplicate implicit-join matches, unmapped dictionary values, malformed string
templates, and dataset-predicate reporting. Every contract below is still rule
text that no example tests. All the features they guard have landed except rank
tie semantics, which arrives with T2.

| Example | Provokes |
|---|---|
| non-output column named in `keys` | S1's only new error |
| unguarded division by zero, and the same expression guarded by `NULLIF` | R010's failure conditions |
| `SQRT` of a negative value, `LN` of zero, integer overflow | the rest of R010's failure conditions |
| an expression using `SUM`, `LAG`, a comparison, or a qualified identifier in the column phase | R010's closed grammar |
| `direction: desc` on a column of mixed types | order-term comparability |
| a `multiple_matches` filter that empties the right side | R003 treats it as an absent match, not a handled condition |
| ranking on a column whose ordering is not total | tie semantics; gated by T2 |
| `row_value` with `offset: 0` | R007 keeps `source` the only spelling of the current row |
| `sum` over a non-numeric source | R007's aggregate input type |
| `mapping_from` with a duplicate right-side key on the `key` columns | R007 dictionary uniqueness |
| `mapping_from` with no match and no `unmapped` handler | join failure behavior |
| `mapping_from` with one of two sources missing and no `missing` handler | R008 partial-key semantics |
| `mapping_from` whose `source` and `key` lists differ in length | R007's list-length error |
| a `row_number` partition in which every row fails the window `filter` | R007: no rank rather than a spurious rank of one |
| `date_impute` with `month: 15`, and with a `day` the imputed month does not have | R007's calendar-range error |
| `date_impute` over an invalid source with no `invalid` handler | fail rather than yield missing |
| a `mapping` whose `dict` keys collide once folded under `case_sensitive: false` | R007 rejects the dictionary rather than picking one |
| a column type outside `column_type` | R011's closed vocabulary |
| unparseable numeric text, an incomplete date, and a non-integral value converted to `int` | R011's conversion failures |
| `greatest` whose `sources` mix incomparable types | R007 comparability |
| duplicate output keys | R005 key uniqueness |
| a failed column verification | R009 reporting |
| a nested expression in a field typed as `variable` | the version 1.0 input-shape boundary |

These gate nothing new, because the features they guard already landed. They
remain urgent: many fail-closed claims in R003, R005, R007, R008, and R010 are
still rule text rather than tested behavior. `sdtm-suppmh-parent-linkage`
states plainly that its own `not_missing` check is meaningful only if the
lookup fails closed, which is exactly the claim no example tests.

## Pilot 7 coverage

The live examples retain every actionable finding from the
[RConsortium pilot 7 synthetic-data](https://github.com/RConsortium/submissions-pilot7-synthetic-data)
survey, and fields that the source declares but never varies are not used as
evidence for a rule. Two of its ideas remain undemonstrated: the irregular
`EPOCH` interval join, which is gap 4, and a questionnaire scale score, which
is a row-wise reduction over columns rather than over a partition and has no
item-level input in the pilot source to demonstrate.

## Acceptance rule for adding a schema feature

A feature should enter the portable vocabulary only when at least one positive
example needs it, a negative or edge example fixes its failure behavior, and R
and Python can implement the same semantics. Sponsor-specific algorithms should
remain behind `function`; common CDISC operations demonstrated by multiple
examples should become closed, documented expressions instead.
