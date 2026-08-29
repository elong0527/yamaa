# Plan for schema improvements driven by the example suite

## Purpose

The suite holds 55 examples: 47 successful golden outputs and eight expected
failures. Three failure examples also commit a CSV beside the structured
error: two record the completed dataset presented to a failing check, and one
records the artifact a blocked derivation would produce. This file records
the design gaps they expose, grouped by root cause, and tracks the schema work
those findings justify.
[`README.md`](README.md) is the reader-facing index of the examples
themselves.

This file tracks the schema and rule work those findings justify. It lists what
has landed in one table and then only the work that remains. Every open item
names the gap it closes, the evidence that justifies it, and the negative
example the acceptance rule requires.

## Landed

| Change | Effect |
|---|---|
| `output: false` on a column, R005 | 28 columns across 11 examples became internal, so named intermediates no longer pollute the artifact |
| `compute` and R010, replacing `add`, `subtract`, `multiply`, `percent_change` | one closed numeric grammar with columns in every operand position, a defined missing policy, and row-wise numeric extremes |
| `order_by_term` with `direction` and `nulls` | descending and null placement are declared, and two negated companion columns were deleted |
| `filter` on `multiple_matches`, R003 and R008 | ordered selection narrows its right side before ordering |
| `column_type` and R011 | `column.type` became a closed enumeration and the conversion matrix is defined, leaving float-to-string as its one open cell |
| `mapping_from.source` and `mapping_from.key` as lists, R003 and R007 and R008 | a lookup declares a compound key and pairs it by position |
| `filter` on `row_number`, R001 and R003 and R007 | a window states its eligibility once; two `TEORD` sort columns and six duplicated flag predicates deleted |
| float-to-text, R005 and R011 | one form for the declared conversion and the artifact; the schema fixes no precision, the project does, and this suite declares four decimal places |
| `date_impute`, R007 and R008 and R011 | the completion rule for a truncated ISO 8601 date is declared rather than spelled out; five internal columns and three regular expressions deleted from `adam-adae-partial-dates` |
| `study_day`, R007 | the SDTM no-Day-0 rule is one expression instead of a guarded `date_diff`, a `compute`, and a conditional; one internal column deleted from `sdtm-vs-visit-study-day` |
| `date_diff.bounds`, and a stated missing-operand result | an inclusive or strictly-between count is declared rather than adjusted afterwards; two internal columns, two `compute` calls, and three guarding predicates deleted |
| `greatest` and `least`, R007 | a row-wise extreme over any comparable type; `sdtm-dm-reference-dates` replaced a three-way null-guarded `case` chain with one expression, and R010's numeric functions stay where they are |
| Form-scoped ODM context, R002 | contextual item lookup now includes every available collection level, including `FormOID`, and fixes zero-match and multiple-match behavior |
| `str_template` and R012 | a closed interpolation grammar makes composite strings concise while keeping dependencies visible and host-language code out |
| `row_value`, R001 and R007 | one window reads any row of an ordered partition, so `adam-adrs-confirmed-response` became a golden output; a signed offset carries the direction rather than a `lead` and `lag` pair whose field lists would be identical, and reading a column's own earlier value stays a cycle rather than an iteration |
| `sum` and `count`, R003 and R007 and R008 | the aggregate registry covers ordinary reductions, so `adam-adex-cumulative-dose` became a golden output; `count` counts non-missing values, an all-missing group totals to missing rather than zero, and four rules stopped naming `min` and `max` by hand. Replacing the entries with one grouping expression over a reducer grammar was deferred to issue #30 and landed in the row below |

| one `aggregate` expression and R013, replacing `min`, `max`, `sum`, and `count` | one closed reducer grammar in place of an entry per reducer, so a fifth reduction is a table row rather than a registry entry; arithmetic over the records being reduced became expressible, which issue #30 named as its real motivation; the aggregate `missing:` handler was removed rather than relocated, because an expression body gives it no operand to attach to; twenty call sites across eight examples moved with no change of meaning |

Thirteen gaps closed and were removed from the catalogue in
[`README.md`](README.md); fifteen remain, and the numbering below refers to
that renumbered list. Three of the fifteen were reworded rather than closed:
gaps 1, 2, and 4 each stated a limitation broader than the evidence supported,
and each now names only the case that is actually blocked.

Three lessons from those changes are worth keeping.

**A closed grammar beat an expression per operator.** The original plan
proposed widening `add` and `multiply` and adding `divide`, `round`, `abs`,
`greatest`, and `least`. One `compute` entry replaced all of it, and R010
states the semantics once. Prefer the same shape wherever a family of
operators is proposed.

**Widening a field beat adding an entry, again.** T3 was written as a new
`lookup` expression with a `dataset`/`on`/`value` payload. Under the design's
own constraint — left join only, one column added per call — that entry was a
second spelling of `mapping_from`, and its only new capability was the compound
key. Widening `source` and `key` to lists bought exactly that, left the
registry unchanged, and rewrote no existing call site. Ask what a proposed
entry does that an existing one could not be widened to do.

**Predictions about golden output were wrong twice.** `compute` was expected to
move nothing and moved nothing, but the `multiple_matches` filter was also
expected to move nothing and changed `adam-adsl-crossover-periods`, because two
columns existed only to expose the workaround it removed. Assume any change
that removes a workaround also removes the columns that documented it.

## Open design gaps

Fifteen gaps are open across the suite, grouped by root cause rather than by
the example that found them, because most are consequences of a few underlying
decisions rather than independent omissions. Closed gaps are removed from this
list rather than marked; `plan.md` records what was closed and how.

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
   separate question and is not blocked: R007 lets an `aggregate` over
   current-output columns declare `group_by`, reduce those rows within each
   partition, and broadcast the result, so a predicate comparing each row with
   that value flags the whole tied set. That example flags one record because that is the rule it models.

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

   The interval is one case of a wider one: a right-side `filter` is a
   predicate over right-side records only, so no match can be narrowed by a
   value belonging to the output row being derived.
   `negative-adrs-response-before-progression` fails for exactly that reason,
   because which assessments are eligible depends on the subject's own
   progression date. The same shape blocks a running extreme, where each row
   reduces the rows before it: with `adam-adtr-sum-of-target-diameters`
   summing lesions to the assessment grain, taking the lowest of the earlier
   sums still has nowhere to say `earlier`, which is also the two-level
   reduction issue #30 records as having no example.

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
   came from one record. `adam-adrs-best-overall-response` and
   `adam-adtte-duration-of-response` repeat it: each derives a date and the
   record identity beside it from separate reductions that agree only because
   they are ordered the same way.
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
   representation before an example can assert it. The imputed components are
   also literals, so a rule stated against the period rather than a fixed
   number, such as the last day of whichever month was collected, cannot be
   written at all; `adam-adrs-overall-response-records` takes the first day of
   the period instead.
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
    subject in an applicable cohort has both of its compartments. An assertion
    over an ordered series reaches one neighbour and no further:
    `negative-adrs-partial-response-after-complete-response` rejects a partial
    response directly after a complete one, and the same fault with an
    intervening assessment passes.

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
    `adam-adrs-best-overall-response` shows the cost in a published
    definition: which response wins, and whether an assessment that came too
    early leaves the subject progressive or not evaluable, is carried by the
    order of the branches and by nothing a reader can check.
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
already lets an `aggregate` over current-output columns declare `group_by`,
reduce those rows within a partition, and broadcast the result, so a predicate
against that value flags the whole tied set without a new expression.

Decision it forces: tie semantics for a rank number.

Negative example: ranking on a column whose ordering is not total.

### T5. Selection that returns a record

Two gaps have the same cause: an expression selects a value, never a row.

- Gap 5: `sdtm-dm-reference-dates` derives an extreme date with an `aggregate`
  and its associated dose with an ordered `source`, and nothing ties them to
  the same EX record. `sdtm-ae-effective-transaction` runs four independent selections
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

The window half of this item was filed here and did not belong: restricting
which rows a window sees is separable from returning a record, and it landed as
`row_number.filter`.

### T6. Dates and times

Evidence: gaps 8 and 9. `sdtm-ae-effective-transaction` carries an audit
timestamp as `str` and orders it correctly only because ISO 8601 text sorts
chronologically.

Two of the three parts are settled. R011 closed the vocabulary: `column_type`
is closed, a declared `date` is complete or nothing, and there is no datetime
type. `date_impute` closed the declared imputation rule, so completion is a
statement in the specification rather than regular expressions and string
defaults.

What remains is the part neither addressed. A `date` produced by imputation is
indistinguishable from a collected one, so precision can only be recovered from
the source text, and nothing marks a comparison made against an imputed
operand. A precision concept would close both at once, and a `date_precision`
expression would close the first alone. Gap 9 is untouched.

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

The conversion half of this item has landed. Float-to-text was the last
undefined cell in R011's matrix, and closing it corrected two committed values
rather than the three the item predicted: the golden files were written by R's
default fifteen-significant-digit output, so `adam-adlb-bds` was already
consistent with a four-decimal setting and only `adam-adsl-bmi-compute` and
`sdtm-vs-unit-standardization` moved. Ingestion is the harder half and is
untouched, because it is about recognising a value rather than rendering one.

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

### T11. The fold: rows at a derived grain

A specification can name one grain, its own output, plus one implicit grain,
the right side of an R003 join. Row construction can fan out and never fold, so
`summarise()` has no spelling: a grouped grain can only be bought as an input
file. `adam-adex-cumulative-dose` buys its subject-treatment grain from
`input/subject_treatment.csv` although `EX` already contains those pairs, and
four examples start from a `*_pre.csv` whose provenance no specification
states.

The `aggregate` grammar closed the reducer half of this and left the fold
open, deliberately: a reducer computes a value and never a row. What remains is
issue #30's two-level reduction — group `EX` by subject and cycle, total each,
then take the largest — output rows at a derived grain, and, through them, gaps
5, 11, and 13. R013 rejects a nested reduction for exactly this reason: the
intermediate grain has no name.

Three shapes were considered and none was taken with the reducer work:

- `group_by` on `row_class`, so a row template emits one row per distinct
  combination of its driver. The smallest of the three; R001 phase 1 already
  permits a row-count change, and only fan-out uses it today. It folds the
  output itself, so it needs R002 and R003 to say what a qualified driver
  reference means when there is no current record.
- A summarise-only view: a named relation with a driver, a filter, keys, and
  reduced columns, reached through the R003 join like any dataset. It reaches
  two-level reduction because a view may drive a view, and it leaves R002 and
  R003 untouched, because a view is foreign to the output.
- A full view, which also derives per-record columns. The only shape that
  reaches gaps 5 and 13, and the largest.

Per the acceptance rule, a second example needing an intermediate grain is the
trigger.

Decision it forces: whether a fold produces the output grain itself or a named
relation beside it, and whether that relation may select a record as well as
reduce one, which is T5's question seen from here.

## Sequencing

1. **T2**, with its negative example. It is registry work with committed
   evidence and bounded semantics. T3 landed as a widened `mapping_from` and
   still owes the four negative examples listed below.
2. **T7**, which is rule text rather than schema. Its conversion half landed
   with float-to-text; what remains is source-format recognition, still
   required before any implementation can claim R and Python parity.
3. **T5, T6, T8, T9, T10, T11** are design documents. Write the document before
   the schema change, and expect each to retire several gaps at once, as
   `compute` did. T5, T8, and T11 overlap enough that the three should be
   decided together rather than in sequence.
4. **T1** last, because its answer probably lies inside T10 rather than in a
   widened field.

Expected catalogue edits: T2 retires gap 2, T5 retires gaps 3, 5, and 6, T6
retires gaps 8 and 9, T7 retires gap 7, T8 retires gaps 10 and 11, T9 retires
gap 15, and T10 retires gaps 4, 12, 13, and 14 along with whatever remains of
gap 1. T11 claims no gap of its own: it reaches gaps 5, 11, and 13 from a
different direction, so whichever of T5, T8, and T11 is decided first should
record which of those it actually closed.

## Negative examples this plan requires

The acceptance rule needs failure behavior fixed before a feature is added.
Eight expected-failure examples now establish a self-referential ordered
window, duplicate implicit-join matches, unmapped dictionary values, malformed
string templates, dataset-predicate reporting, a reducer expression reading a
value that varies within its group, an ordered-series predicate over one
neighbour, and a right-side filter reaching for the current output row. The
table below lists the remaining
contracts. ODM form scoping is a positive example backed by normative rule
text.

| Example | Provokes | Gates |
|---|---|---|
| non-output column named in `keys` | S1's only new error | already landed, untested |
| unguarded division by zero, and the same expression guarded by `NULLIF` | R010's failure conditions | already landed, untested |
| `SQRT` of a negative value, `LN` of zero, integer overflow | the rest of R010's failure conditions | already landed, untested |
| an expression using `SUM`, `LAG`, a comparison, or a qualified identifier in the column phase | R010's closed grammar | already landed, untested |
| `direction: desc` on a column of mixed types | order-term comparability | already landed, untested |
| a `multiple_matches` filter that empties the right side | R003 treats it as an absent match, not a handled condition | already landed, untested |
| a cross-dataset `source` with duplicate applicable keys | R003 right-side uniqueness | `negative-source-duplicate-right-key` |
| a right-side `filter` naming a current-output column | R003's right-side-only predicate scope | `negative-adrs-response-before-progression` |
| ranking on a column whose ordering is not total | tie semantics | T2 |
| a column reading its own value from an earlier row of its partition | R001 reports a cycle rather than iterating | `negative-row-value-self-reference` |
| `row_value` with `offset: 0` | R007 keeps `source` the only spelling of the current row | already landed, untested |
| `SUM` over a string column | R013's reducer input types | already landed, untested |
| `SUM(EX.EXDOSE) / EX.EXPLDOS` where `EX.EXPLDOS` is not grouped on | R013's grain rule | `negative-adex-relative-dose-intensity` |
| a reducer name outside R013's table | R013's closed vocabulary | already landed, untested |
| a nested reducer, `MAX(SUM(EX.EXDOSE))` | R013 rejects it rather than inventing an intermediate grain | already landed, untested |
| an `expr` naming two datasets, and one mixing a qualified identifier with an unqualified column | R013's one-relation rule | already landed, untested |
| a `group_by` column that is not an output key, and an unqualified `expr` with no `group_by` | R013's grain declarations | already landed, untested |
| a window, `CASE`, or comparison inside an `expr` | R013's boundary against R004 and R007 | already landed, untested |
| `mapping_from` with a duplicate right-side key on the `key` columns | R007 dictionary uniqueness | already landed, untested |
| `mapping_from` with no match and no `unmapped` handler | join failure behavior | already landed, untested |
| `mapping_from` with one of two sources missing and no `missing` handler | R008 partial-key semantics | already landed, untested |
| `mapping_from` whose `source` and `key` lists differ in length | R007's new error | already landed, untested |
| a `row_number` partition in which every row fails the window `filter` | R007: no rank rather than a spurious rank of one | already landed, untested |
| `date_impute` with `month: 15`, and with a `day` the imputed month does not have | R007's calendar-range error | already landed, untested |
| `date_impute` over an invalid source with no `invalid` handler | fail rather than yield missing | already landed, untested |
| a `mapping` whose `dict` keys collide once folded under `case_sensitive: false` | R007 rejects the dictionary rather than picking one | already landed, untested |
| a non-missing value absent from `dict` with no `unmapped` handler | R008 makes the condition fatal | `negative-mapping-unmapped-value` |
| a column type outside `column_type` | R011's closed vocabulary | already landed, untested |
| unparseable numeric text, an incomplete date, and a non-integral value converted to `int` | R011's conversion failures | already landed, untested |
| `greatest` whose `sources` mix incomparable types | R007 comparability | already landed, untested |
| duplicate output keys | R005 key uniqueness | already landed, untested |
| a failed column verification | R009 reporting | already landed, untested |
| a failed dataset predicate | R009 reporting | `negative-adam-adsl-stratification-reconciliation` |
| an operator inside a string-template placeholder | R012's closed grammar | `negative-adsl-subject-reference` |
| a nested expression in a field typed as `variable` | the version 1.0 input-shape boundary | already landed, untested |

Most gate nothing new because the features already landed. They remain urgent:
many fail-closed claims in R003, R005, R007, R008, and R010 are still rule text
rather than tested behavior. `sdtm-suppmh-parent-linkage` states plainly that
its own `not_missing` check is meaningful only if the lookup fails closed,
which is exactly the claim no example tests.

## Pilot 7 coverage audit

The live examples retain every actionable finding from the
[RConsortium pilot 7 synthetic-data](https://github.com/RConsortium/submissions-pilot7-synthetic-data)
survey. Endpoint values in its `RE` form are simulated, so
`adam-adtte-progression-free-survival` computes its own target from `RS` and
`DS`. ODM form context is normative in R002 and exercised by
`odm-form-scoped-item-resolution`. Fields that the source declares but never
varies are not used as evidence for a rule.

Ideas that did not become separate directories are covered as follows:

| Survey idea | Current coverage |
|---|---|
| multi-parameter ADTTE row templates | `adam-adlb-bds` already shows one template per parameter; gap 12 records the structural growth |
| metastatic-site count across columns | R010 `compute` accepts columns on both sides of `+`; no language boundary remains |
| Fridericia QT correction | R010 `compute` supports data divisors and fractional `POWER`; `adam-adsl-bmi-compute` already fixes both operation families |
| irregular `EPOCH` interval join | gap 4 is already evidenced by `sdtm-vs-visit-study-day` and `adam-adsl-crossover-periods` |
| questionnaire scale score | `aggregate` reduces records within a partition, but a total across item columns is a row-wise reduction over columns instead, and the pilot source has no item-level input to demonstrate it |
| invalid endpoint date | `negative-adam-adsl-stratification-reconciliation` fixes the same dataset-predicate failure contract without duplicating a PFS example |

## Acceptance rule for adding a schema feature

A feature should enter the portable vocabulary only when at least one positive
example needs it, a negative or edge example fixes its failure behavior, and R
and Python can implement the same semantics. Sponsor-specific algorithms should
remain behind `function`; common CDISC operations demonstrated by multiple
examples should become closed, documented expressions instead.
