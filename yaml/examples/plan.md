# Plan for schema improvements driven by the example suite

## Purpose

The suite holds 33 fixtures, each covering one derivation boundary with
committed golden output. Their READMEs record the design gaps, grouped by root
cause in [`README.md`](README.md).

This file tracks the schema and rule work those findings justify. It lists what
has landed in one table and then only the work that remains. Every open item
names the gap it closes, the evidence that justifies it, and the negative
fixture the acceptance rule requires.

## Landed

| Change | Effect |
|---|---|
| `output: false` on a column, R005 | 28 columns across 11 fixtures became internal, so named intermediates no longer pollute the artifact |
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

Eleven gaps closed and were removed from the catalogue in
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
key. Widening `source` and `key` to lists bought exactly that, left the registry
unchanged, and rewrote no existing call site. Ask what a proposed entry does
that an existing one could not be widened to do.

**Predictions about golden output were wrong twice.** `compute` was expected to
move nothing and moved nothing, but the `multiple_matches` filter was also
expected to move nothing and changed `adam-adsl-crossover-periods`, because two
columns existed only to expose the workaround it removed. Assume any change
that removes a workaround also removes the fixture columns that documented it.

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
were. Normalizing splits the two correctly. The remaining case is rare enough
that `function` is the honest answer until a fixture proves otherwise.

Evidence: gap 1. No action until a fixture needs it.

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

Negative fixture: ranking on a column whose ordering is not total.

### T5. Selection that returns a record

Two gaps have the same cause: an expression selects a value, never a row.

- Gap 5: `sdtm-dm-reference-dates` derives an extreme date with `max` and its
  associated dose with an ordered `source`, and nothing ties them to the same
  EX record. `sdtm-ae-effective-transaction` runs four independent selections
  that agree only because all four declare the same ordering.
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

No fixture now carries an imputation flag, so both halves are argued from the
rules rather than shown in golden output. A fixture that records which
component was supplied is the first thing this item needs.

`greatest` and `least` compare dates as ordinary comparable values. If a
precision concept lands here, a row-wise extreme over dates has to say whether
an imputed operand can win, so this item owns that decision rather than R007.

### T7. Source-format ingestion

Gap 7: source-format missing values and type inference have no normative rule.
Every fixture assumes an empty CSV field is missing and distinguishes it from a
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
artifact. Until that artifact is defined, fixtures must not invent its shape.

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

1. **T2**, with its negative fixtures. It is a registry entry with committed
   evidence and bounded semantics. T3 landed as a widened `mapping_from` and
   still owes the four negative fixtures listed below.
2. **T7**, which is rule text rather than schema. Its conversion half landed
   with float-to-text; what remains is source-format recognition, still
   required before any implementation can claim R and Python parity.
3. **T5, T6, T8, T9, T10** are design documents. Write the document before the
   schema change, and expect each to retire several gaps at once, as `compute`
   did.
4. **T1** last, because its answer probably lies inside T10 rather than in a
   widened field.

Expected README edits: T2 retires gap 2, T5 retires gaps 3, 5, and 6, T6
retires gaps 8 and 9, T7 retires gap 7, T8 retires gaps 10 and 11, T9 retires
gap 15, and T10 retires gaps 4, 12, 13, and 14, along with whatever remains of
gap 1.

## Negative fixtures this plan requires

The acceptance rule needs failure behavior fixed before a feature is added.
**None of these exist.** The N-series of the earlier assessment plan is
entirely unimplemented, which makes this the binding constraint on every open
item above and not a separate workstream.

| Fixture | Provokes | Gates |
|---|---|---|
| non-output column named in `keys` | S1's only new error | already landed, untested |
| unguarded division by zero, and the same expression guarded by `NULLIF` | R010's failure conditions | already landed, untested |
| `SQRT` of a negative value, `LN` of zero, integer overflow | the rest of R010's failure conditions | already landed, untested |
| an expression using `SUM`, `LAG`, a comparison, or a qualified identifier in the column phase | R010's closed grammar | already landed, untested |
| `direction: desc` on a column of mixed types | order-term comparability | already landed, untested |
| a `multiple_matches` filter that empties the right side | R003 treats it as an absent match, not a handled condition | already landed, untested |
| ranking on a column whose ordering is not total | tie semantics | T2 |
| `mapping_from` with a duplicate right-side key on the `key` columns | R007 dictionary uniqueness | already landed, untested |
| `mapping_from` with no match and no `unmapped` handler | join failure behavior | already landed, untested |
| `mapping_from` with one of two sources missing and no `missing` handler | R008 partial-key semantics | already landed, untested |
| `mapping_from` whose `source` and `key` lists differ in length | R007's new error | already landed, untested |
| a `row_number` partition in which every row fails the window `filter` | R007: no rank rather than a spurious rank of one | already landed, untested |
| `date_impute` with `month: 15`, and with a `day` the imputed month does not have | R007's calendar-range error | already landed, untested |
| `date_impute` over an invalid source with no `invalid` handler | fail rather than yield missing | already landed, untested |

All but one gate nothing new, because the features already landed. They are
the more urgent set: every fail-closed claim in the fixture READMEs and in
R003, R005, R007, R008, and R010 is currently an assertion rather than a tested
behavior. `sdtm-suppmh-parent-linkage` states plainly that its own
`not_missing` verification is meaningful only if the lookup fails closed, which
is exactly the claim no fixture tests.

## Acceptance rule for adding a schema feature

A feature should enter the portable vocabulary only when at least one positive
fixture needs it, a negative or edge fixture fixes its failure behavior, and R
and Python can implement the same semantics. Sponsor-specific algorithms should
remain behind `function`; common CDISC operations demonstrated by multiple
fixtures should become closed, documented expressions instead.
