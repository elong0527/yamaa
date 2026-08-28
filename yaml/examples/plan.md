# Plan for schema improvements driven by the example suite

## Purpose

The example suite is complete for Priority 1 and Priority 2 of the earlier
assessment plan: 32 fixtures, each covering one derivation boundary with
committed golden output. Their READMEs record 26 design gaps, grouped by root
cause in [`README.md`](README.md).

This file tracks the schema and rule work those findings justify. It lists what
has landed in one table and then only the work that remains. Every open item
names the gap it closes, the evidence that justifies it, and the negative
fixture the acceptance rule requires.

## Landed

| Change | Effect |
|---|---|
| `output: false` on a column, R005 | 28 columns across 11 fixtures became internal; closes gap 6 |
| `compute` and R010, replacing `add`, `subtract`, `multiply`, `percent_change` | one closed numeric grammar with columns in every operand position; closes gaps 2, 3, 16, the arithmetic half of 1, and the numeric half of 10 |
| `order_by_term` with `direction` and `nulls` | two negated companion columns deleted; closes gaps 4 and 14 |
| `filter` on `multiple_matches`, R003 and R008 | ordered selection narrows its right side before ordering; closes the right-side half of gap 13 |
| `column_type` and R011 | `column.type` became a closed enumeration and the conversion matrix is defined; closes gap 20 and the definitional half of gap 18, leaving gap 17 as the one open cell |

Seven gaps are closed, four are partly closed, and fifteen are open.

Two lessons from the first four changes are worth keeping.

**A closed grammar beat an expression per operator.** The original plan
proposed widening `add` and `multiply` and adding `divide`, `round`, `abs`,
`greatest`, and `least`. One `compute` entry replaced all of it, and R010
states the semantics once. Prefer the same shape wherever a family of
operators is proposed.

**Predictions about golden output were wrong twice.** `compute` was expected to
move nothing and moved nothing, but S4 was also expected to move nothing and
changed `adam-adsl-crossover-periods`, because two columns existed only to
expose the workaround it removed. Assume any change that removes a workaround
also removes the fixture columns that documented it.

## Open work

### T1. A variable wherever a literal list is accepted

The last item in group A. `cut.breaks` and the window bounds written as
predicate literals cannot be read from a column, so an analysis window bound or
a per-test grading threshold cannot be data.

`compute` solved this for arithmetic by accepting a column in every operand
position. The same question for `cut` is harder, because `breaks` is a list and
a per-row list has no meaning; the likely answer is a parameter dataset and a
lookup rather than a variable in `breaks`. `adam-advs-analysis-visit` and
`adam-adlb-closest-visit` both hold window bounds as literals inside
predicates, so this overlaps with group G and should not be settled by widening
one field.

Evidence: gap 1. Decide the shape before writing schema.

### T2. `rank` and `dense_rank`

Registry entries with the same fields as `row_number`.

Evidence: gap 5. `adam-adae-worst-severity` has two events tied on severity and
date; `row_number` assigns 1 and 2 so exactly one is flagged, and a sponsor who
flags every record tied at the worst severity cannot express it. Distinct-level
counts are unavailable for the same reason.

Decision it forces: tie semantics, and whether a flag over a tied set is a
window expression or a verification concern.

Negative fixture: ranking on a column whose ordering is not total.

### T3. `lookup`, an explicit multi-column join

```yaml
lookup:
    - dataset: {type: str, required: true}
    - on: {type: "list[join_key_class]", required: true}
    - value: {type: str, required: true}
    - missing: {type: literal_value, required: false}
    - unmapped: {type: literal_value, required: false}
```

Evidence: gaps 7 and 8. `sdtm-vs-visit-study-day` calls `mapping_from` twice
against the same dictionary row to get `VISITNUM` and `EPOCH`.
`sdtm-suppmh-qualifiers` cannot join a parent sequence on subject plus repeat
key at all, and `sdtm-relrec-many-to-many` needs the same join to reach `IDVAR`
and `IDVARVAL` from a link table.

It returns one column per call. A multi-column return conflicts with one
expression producing one value and should be considered separately, after
`lookup` exists.

Negative fixtures: a duplicate right-side key, and no match with no `unmapped`
handler.

### T4. Row-wise extremes over dates

`GREATEST` and `LEAST` closed this for numbers. R010 is numeric, so
`sdtm-dm-reference-dates` still writes a three-way latest-participation date as
null-guarded `case` branches that widen with each candidate.

Evidence: the date half of gap 10. Decide whether R010 grows a comparable-typed
form or whether a separate `greatest`/`least` expression covers dates.

### T5. Selection that returns a record

Three separate gaps have the same cause: an expression selects a value, never a
row.

- Gap 11: `sdtm-dm-reference-dates` derives an extreme date with `max` and its
  associated dose with an ordered `source`, and nothing ties them to the same
  EX record. `sdtm-ae-effective-transaction` runs four independent selections
  that agree only because all four declare the same ordering.
- Gap 12: a missing aggregate cannot distinguish no matching record from
  matching records whose values are all missing.
- The window half of gap 13: `row_number` still cannot filter, so eligibility
  is expressed as a sort column that ranks ineligible records last.

`filter` on `multiple_matches` narrowed this but did not close it: two
selections can now be made to see the same records without being tied to the
same one. A construct that selects a right-side record once and reads several
columns from it would close all three. This is design work, not a registry
entry.

### T6. Dates and times

Evidence: gaps 18, 19, and 20. `adam-adae-partial-dates` rebuilds dates with
regular expressions and string defaults because a declared `date` is complete
or nothing. `sdtm-ae-effective-transaction` carries an audit timestamp as `str`
and orders it correctly only because ISO 8601 text sorts chronologically.

R011 settled the vocabulary half: `column_type` is closed, a declared `date` is
complete or nothing, and there is no datetime type, so gap 20 is closed and the
type split is a decision already recorded rather than an open question. What
remains is the harder half: a precision concept, a declared imputation rule
with its flag, and a statement about comparison when an operand is imputed.
Still the largest Tier 3 item, and gap 19 is untouched by R011.

### T7. Ingestion and conversion rules

Two unrelated questions that both block portability.

- Gap 15: source-format missing values and type inference have no normative
  rule. Every fixture assumes an empty CSV field is missing and distinguishes
  it from a nonempty malformed value.
- Gap 17: float-to-string conversion is undefined.
  `sdtm-vs-unit-standardization` proposes a shortest-round-trip rule and
  commits `37` rather than `37.0` to force the decision. R010 made this more
  urgent, not less: derivations now carry full precision by design, so three
  fixtures hold golden values whose rendering is unspecified. R011 defined
  every other conversion and deliberately left this one open, so it is now
  the single undefined cell in the matrix and the same decision as the text
  an artifact writes for a `float`.

### T8. The output and pipeline contract

- Gap 21: one specification derives one dataset. `sdtm-suppmh-qualifiers`
  cannot assign a parent sequence and consume it in one run, and
  `sdtm-dm-reference-dates` depends on an execution order it cannot state.
  R001 cycle detection is per specification, so a cross-dataset cycle cannot be
  reported. Needs a manifest, cross-specification dependency inference, and
  cycle reporting.
- Gap 22: nothing controls output row order, and verifications are row-wise
  over the completed output. `sdtm-suppmh-qualifiers` leaves rows in
  row-template order rather than a submission order, and referential integrity
  between a SUPPQUAL record and its parent domain cannot be asserted.

### T9. Governed metadata

Evidence: gap 26. `sdtm-dm-metadata-contract` declares origin, length, and
codelist as free-form strings, marks `USUBJID` as `Derived` by hand although
`str_concat` already encodes that, and declares a codelist name next to an
unrelated `allowed_values` list.

Needs a vocabulary, a link between a declared codelist and its enforced values,
a length concept connected to the declared type, and an expected metadata
artifact. Until that artifact is defined, fixtures must not invent its shape.

### T10. Declarable study structure

Group G, gaps 23 to 25, and the largest open area.

- Conditional applicability, treatment period, relationship degree, and
  analysis window are protocol structure re-expressed as filters, predicate
  literals, and one row template per slot, so a specification grows with the
  data rather than describing the design.
- Row construction cannot consume values resolved during column derivation, so
  `sdtm-ae-effective-transaction` commits a record whose last transaction
  removed it.
- A derivation cannot carry both a value and the reason for it, so
  `adam-adrs-composite-response` writes the same four predicates twice.

Also here: gap 9, the absent interval join, which is what an analysis window or
an `EPOCH` assignment actually needs.

## Sequencing

1. **T2 and T3**, one at a time, each with its negative fixtures. Both are
   registry entries with committed evidence and bounded semantics.
2. **T4**, once the `lookup` work has settled whether R010 grows comparable
   types or a separate expression appears.
3. **T7**, which is rule text rather than schema and unblocks nothing else but
   is required before any implementation can claim R and Python parity.
4. **T5, T6, T8, T9, T10** are design documents. Write the document before the
   schema change, and expect each to retire several gaps at once, as `compute`
   did.
5. **T1** last, because its answer probably lies inside T10 rather than in a
   widened field.

Expected README edits: T2 retires gap 5, T3 retires gaps 7 and 8, T4 retires
the rest of gap 10, T5 retires gaps 11, 12, and the window half of 13, T6
retires gaps 18 to 20, T7 retires gaps 15 and 17, T8 retires gaps 21 and 22,
T9 retires gap 26, and T10 retires gaps 9, 23, 24, and 25, along with whatever
remains of gap 1.

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
| `lookup` with a duplicate right-side key, and with no match | join failure behavior | T3 |

The first six gate nothing new, because the features already landed. They are
the more urgent set: every fail-closed claim in the fixture READMEs and in
R003, R005, R008, and R010 is currently an assertion rather than a tested
behavior.

## Acceptance rule for adding a schema feature

A feature should enter the portable vocabulary only when at least one positive
fixture needs it, a negative or edge fixture fixes its failure behavior, and R
and Python can implement the same semantics. Sponsor-specific algorithms should
remain behind `function`; common CDISC operations demonstrated by multiple
fixtures should become closed, documented expressions instead.
