# Derivation schema examples

These fixtures exercise `yaml/schema.yaml` with small inputs and exact expected
outputs. They are intended for human review, automated tests, and AI-assisted
implementation.

Execution behavior is defined by the normative rules in
[`../rules/README.md`](../rules/README.md). Example READMEs describe only the
fixture-specific application of those rules.

`odm.csv` is a tabular projection of ODM clinical data, not an ODM exchange
document itself. Its fields map to the official
[CDISC ODM 2.0 clinical-data schema](https://github.com/cdisc-org/DataExchange-ODM/blob/main/schema/ODM-clinicaldata.xsd).

Dataset declarations, variable references, and ODM contextual lookups are
governed by [R002](../rules/R002-source-binding.md).

Examples are ordered by increasing complexity:

1. `sdtm-dm-basic` — direct mapping, literals, terminology mapping, and a
   current-dataset reference.
2. `sdtm-lb-findings` — row construction, wide-to-long Findings conversion,
   missing-result filtering, and sequence generation.
3. `sdtm-relrec-related-records` — row construction from multiple source
   datasets for a one-to-many relationship between records.
4. `adam-adlb-bds` — source-dataset enrichment, baseline selection, change from
   baseline, percentage change, and analysis sequence.

Each example contains a specification, source CSV files, an expected CSV, and
a README defining behavior that an implementation must reproduce.

## Schema probes

A probe is a fixture written to test whether the schema can express a real
derivation pattern. A probe that does not pass is a design finding, not a defect
in the fixture, and its README records what the schema could not express.

5. `adam-adsl-mapping` — value mapping with an inline dictionary, deriving
   ADaM numeric companions, including a value the dictionary does not define.
6. `sdtm-ae-dictionary-coding` — value mapping where the dictionary is an
   external file, including an uncoded term and the dictionary version.
7. `adam-adsl-bmi-function` — BMI derivation through a function supplied by the
   project's global R environment.
8. `adam-adsl-bmi-compute` — the same BMI derivation as one closed numeric
   expression, with a zero-height guard, `NULL` propagation, and a genuine
   rounding tie.

All four pass. The first two cover value standardization from inline and
external dictionaries. The third exercises the runtime-function extension
point with source and literal arguments. The fourth supersedes the third as the
portable way to write arithmetic and leaves it as the only `function`
coverage.

## Challenge probes

9. `sdtm-lb-multiform` — a compact CATH-derived ODM-to-SDTM probe consolidating
   serum, skin-biopsy, saliva, and tape-strip data into LB. It covers form-
   specific contextual dates, structural absence versus explicit missingness,
   a collected nonnumeric result, repeated item groups, heterogeneous specimen
   metadata, and deterministic ordering ties.
10. `adam-adsl-identifier-parsing` — parses a site from `USUBJID`, falls back
    to collected `SITEID`, and constructs a subject reference.
11. `adam-adsl-geography-normalization` — normalizes collected country text
    and maps country codes to regions, including missing and unmapped paths.
12. `adam-adsl-treatment-selection` — a standalone SDTM-to-ADaM probe deriving
    subject-level treatment from DM and EX. It covers filtered first/last dates,
    ordered treatment selection, placebo dose zero, no-match subjects, fallback,
    and inclusive duration.
13. `adam-adsl-disposition` — a standalone SDTM-to-ADaM probe deriving final
    subject disposition from DM and DS. It covers filtered final dates, ordered
    associated values, no-match subjects, and deterministic same-day selection.
14. `adam-adsl-population-flags` — derives safety and intent-to-treat flags
    from a small pre-derived ADSL slice.
15. `adam-adae-treatment-emergent` — classifies AE start dates against an
    inclusive ADSL treatment interval, including both boundaries and a subject
    with no ADSL match.
16. `adam-adae-occurrence-flags` — derives first treatment-emergent occurrence
    flags at subject, SOC, and preferred-term levels, including same-day ties.
17. `adam-adae-string-handlers` — isolates lowercase normalization and the
    distinct missing/no-match paths for sponsor event identifiers.
18. `adam-adae-severity-override` — applies one approved final correction and
    demonstrates that a dependent numeric severity sees the corrected value.
19. `sdtm-vs-visit-study-day` — looks up visit metadata by an explicit key and
    derives study day under the SDTM no-Day-0 rule, including an unscheduled
    visit, a missing date, and a subject with no reference date.
20. `adam-advs-analysis-visit` — assigns records to analysis windows by study
    day, separates `AVISIT`/`AVISITN` from collected `VISIT`/`VISITNUM`, and
    flags one analysis record per window.
21. `sdtm-vs-unit-standardization` — keeps a collected result and its
    standardized companion separate, converting pounds and Fahrenheit as one
    formula each and committing an unrounded product.
22. `sdtm-suppmh-qualifiers` — reshapes non-standard qualifier columns into
    SUPPQUAL rows and links them to a pre-assigned parent sequence.
23. `sdtm-dm-reference-dates` — reduces EX, DS, and AE into the DM reference
    dates, including a three-way latest-participation date, a screen failure
    with no exposure, and an adverse event ending after the last dose.
24. `adam-adae-partial-dates` — rebuilds analysis dates from year-only,
    year-month, complete, unparseable, and uncollected values, with an
    imputation flag and its effect on treatment emergence.
25. `adam-adlb-closest-visit` — selects the record nearest a target study day,
    including a tie broken toward the later record and a record outside the
    window.
26. `adam-adae-worst-severity` — selects the worst-severity event per
    preferred term, ordering a controlled vocabulary through a numeric proxy.
27. `sdtm-lb-conditional-compartments` — separates a structurally inapplicable
    compartment from an applicable sample that was not collected.
28. `sdtm-ae-effective-transaction` — selects the effective state of a record
    from an insert/update/remove transaction log.
29. `adam-adsl-crossover-periods` — derives period-scoped treatments and dates
    across a washout, including a subject who never crossed over.
30. `sdtm-relrec-many-to-many` — represents records belonging to two
    relationships at once.
31. `adam-adrs-composite-response` — combines an efficacy threshold, a safety
    condition, and a discontinuation rule into one responder value.
32. `adam-adsl-dependency-order` — declares every column in reverse dependency
    order, with dependencies reachable only through predicates and window
    fields.
33. `sdtm-dm-metadata-contract` — declares labels, origins, lengths, and
    codelists on every column and shows how little of that is governed.

## Coverage gaps

All 17 registered non-leaf expressions are exercised by at least one fixture.
`compute` replaced every use of `multiply`, `add`, `subtract`, and
`percent_change`, and those four keywords were deleted rather than left
registered and unexercised. `adam-adae-string-handlers` closes the previously identified
`str_lower` and `str_extract.missing` gaps;
`adam-adae-severity-override` closes the `override` gap.

`sdtm-lb-multiform` covers `case` and implication checks.
The five focused ADSL probes separately cover identifier parsing and fallback,
geography normalization, treatment selection and duration, disposition
selection, and population flags. Together they retain the combined fixture's
`compute`, `coalesce`, `date_diff`, filtered `min`/`max`, `str_concat`,
`str_extract.no_match`, `str_upper`, ordered `multiple_matches`, and grouped
completeness coverage.
The four focused ADAE probes separately cover treatment-interval
classification, deterministic hierarchical occurrence flags, string-handler
paths, and final correction.
`sdtm-vs-visit-study-day` is the first fixture to contrast the R003 automatic
join with an explicit-key `mapping_from` lookup, and the first to exercise
`date_diff` at an SDTM boundary.
`adam-advs-analysis-visit` is the first fixture to use `cut` for interval
membership rather than value banding, and the first to exercise the `cut` and
`mapping` `missing` handlers together.
`sdtm-vs-unit-standardization` is the first fixture to nest `compute` and
`mapping` inside `case` branches, and the pair of spellings it records —
`(VSORRESN - 32) * 5 / 9` returning exactly `37` where `(VSORRESN - 32) / 1.8`
returns `36.99999999999999` — is the suite's only evidence for R010's
prohibition on reassociating a formula.
`sdtm-suppmh-qualifiers` is the first fixture to build a SUPPQUAL structure
and the first to reshape several qualifier columns of one row driver into rows.
`sdtm-dm-reference-dates` is the first fixture to reduce three different
right-side datasets into one output row and the first to combine an aggregate
with an ordered selection over the same records.
`adam-adae-partial-dates` is the first fixture to treat a date as text,
exercising `str_extract`, `coalesce` defaults, and `str_concat` together, and
the first to use every declared handler on one source value.
`adam-adlb-closest-visit` closes the closest-to-target question left open by
`adam-advs-analysis-visit`: the distance is `ABS(ADY - AWTARGET)`, one
expression reading the target from the column that publishes it, and the
tie-break is an order term rather than a negated companion column. It is the
first fixture to carry a selection rule with no workaround column at all.
`adam-adae-worst-severity` extends that answer to a categorical criterion: a
controlled vocabulary can be ordered by preference only after a `mapping` gives
it a numeric proxy, though the proxy no longer has to be negated.
The seven Priority 2 fixtures added last cover conditional compartments,
transactional sources, crossover periods, many-to-many relationships, composite
endpoints, dependency ordering, and the metadata contract.
`adam-adsl-dependency-order` is the only fixture that tests R001 dependency
inference directly, and the only one whose column layout is deliberately not
submission order.
`adam-adlb-bds` is the only fixture where `compute` runs during row
construction, so it is the only one that exercises a row-driver-qualified
identifier in a numeric expression.
R010's grammar has no rounding function at all: a derivation carries full
precision and the number of places shown is decided when the value is reported.
No fixture rounds, and none can.

`adam-adlb-closest-visit` and `adam-adae-worst-severity` are the only fixtures
that declare an order term rather than a bare variable, and between them they
cover `direction: desc` over a numeric column and over a mapped vocabulary.

All nine verification keywords are exercised across the fixtures;
`adam-adsl-mapping` covers the generic named `predicate`, while the challenge
probes cover `all_or_none` and `implies`.

## Edge-case assessment

The fixtures demonstrate that local handlers remain readable for missing
contextual items, unmapped terminology, failed numeric conversion, and missing
inputs to banding. Row filters handle absent optional records, and the
zero-baseline rule produces an intentional missing percentage without a special
handler.

Twenty-six design gaps have been recorded across the suite. Group B is closed
and most of group A is, leaving the arithmetic workarounds behind. They are
grouped by root
cause rather than by the fixture that found them, because most of them are
consequences of six underlying decisions rather than independent omissions.

### A. Literal operands and ascending ordering (mostly closed)

This constraint produced more workarounds than any other. Arithmetic is now out
from under it; banding, windowing, and ordering are not.

1. `cut.breaks` and window bounds are still literal lists and cannot be read
   from a column, so an analysis window bound or a per-test grading threshold
   cannot be data. **The arithmetic half is closed by `compute`, defined in
   R010:** a column is accepted in every operand position, so a conversion
   factor or a target day may now be data.
2. **Closed by `compute`.** `add.addend` was a literal and `subtract` typed
   both operands as variables, so the schema could subtract two columns but not
   add them. One numeric expression now accepts columns on both sides of every
   operator, and the four asymmetric keywords were deleted.
3. **Closed by `compute`.** Division, absolute value, exponentiation, and
   roots are in R010's closed function table. `sdtm-vs-unit-standardization`
   shows why the boundary had to be drawn precisely: two algebraically equal
   spellings of one conversion return different doubles, so R010 forbids
   reassociating a formula. Rounding is deliberately absent from the grammar;
   derivations carry full precision.
4. **Closed by the order term.** `row_number.order_by` and
   `multiple_matches.order_by` were ascending with no direction option, so any
   preference for a later or larger value needed a negated companion column —
   a workaround available only to numbers, leaving a date or a string with no
   descending expression at all. `{variable: X, direction: desc}` applies to
   any comparable type. A controlled vocabulary still needs a `mapping` to give
   it a numeric proxy, because the order lives in a dictionary rather than in
   the vocabulary; that half is unchanged.
5. Only `row_number` is registered. Without `rank` and `dense_rank`, ties can
   be broken but not preserved, so a flag cannot cover every record tied at a
   worst value and distinct-level counts cannot be expressed.

The consequence is that one concept in the protocol becomes several unlinked
pieces of specification. In `adam-adlb-closest-visit` a window's bounds, its
target, and the distance to that target are three separate constructs, and the
target day appears both as a column and as a literal with nothing keeping them
consistent. In `sdtm-vs-unit-standardization` each unit pair
needs its own `case` branch, and while `compute` now expresses each conversion
as one formula, the factor still cannot come from a per-unit dictionary.
`adam-adlb-closest-visit` no longer belongs in this list for its distance, which
reads its target from a column, but its window bounds are still predicate
literals. Closest-to-target and worst-severity selection are both expressible,
and only the descending-sort workaround in A4 remains under them.

### B. Named intermediates (closed)

6. Multi-step logic used to require emitting every step as an output column,
   which was a conformance problem rather than an inconvenience:
   `sdtm-vs-visit-study-day` published a DM reference date inside an SDTM VS
   dataset, and `adam-adae-partial-dates` had more scaffolding than analysis.
   **Closed by `output: false` on a column, defined in R005.** Twenty-eight
   columns across eleven fixtures are now internal: derived, converted,
   verified, and available to dependents, but absent from the artifact.

   Two things did not change. A selection still cannot be audited as one
   object, because its reasoning remains several unrelated columns rather than
   one construct; `adam-adlb-closest-visit` now chooses to publish `AWTARGET`
   and `ADIST` as an audit trail and hide the rest, which is an improvement in
   the artifact and not a fix for the gap. And a dataset verification may still
   name an internal column, which is deliberate: those assertions are about the
   derivation, not the output.

### C. Joins are limited to one automatic key join and a single-column lookup

7. `mapping_from` returns one column per call, so a multi-column visit or
   parameter lookup repeats the same dictionary match.
8. There is no explicit multi-column equality join. The R003 automatic join
   uses only output keys shared with the right side, so a subject-plus-repeat-key
   match such as SUPPQUAL linkage cannot be written.
9. There is no interval join, so `EPOCH` cannot be assigned to a record the
   trial design does not name.

### D. Aggregates and selection operate on values, not rows

10. **Closed for numbers by `compute`.** `GREATEST` and `LEAST` in R010's
    function table reduce several columns row-wise, so a largest-of-several
    numeric value is one expression rather than null-guarded `case` branches
    that grow with each candidate. It is not closed for dates: R010 is numeric,
    so a latest-of-several date still needs the `case` chain that
    `sdtm-dm-reference-dates` writes.
11. An extreme value and the values associated with it come from two
    independent reductions that nothing ties to the same right-side record.
12. A missing aggregate result cannot distinguish no matching record from
    matching records whose values are all missing.
13. `row_number` cannot filter. An eligibility sort column expresses a Boolean
    condition, but a general conditional window needs an explicit filter.
    Ordered `source.multiple_matches` **can** now be filtered, which closed the
    right-side half of this gap; the window half remains.
14. **Closed by the order term.** Ordering across missing values was
    undefined, so eligibility sort columns kept ineligible records out of
    contention without defining how two of them compared and fixtures had to
    avoid the case rather than specify it. `nulls` declares the placement.
    R007 fixes the default at `last` and, unlike PostgreSQL, does not flip it
    under `desc`, so an implementation cannot inherit its engine's convention
    and select a different record.

### E. Types, conversion, and missing-value semantics are unresolved

15. Source-format missing values and type inference have no normative rule. The
    fixtures assume an empty CSV field is missing and distinguish it from a
    nonempty malformed value.
16. **Closed for `compute` by R010:** `NULL` propagates, and division by zero
    fails rather than returning missing, so a specification chooses missing
    explicitly with `NULLIF`. R007 still defines no missing-input behavior for
    the deleted `multiply`, `add`, and `subtract` keywords, which is why every
    fixture using them carried a guarding predicate. Those guards are gone with
    the keywords.
17. Float-to-string conversion is undefined. `sdtm-vs-unit-standardization`
    proposes a shortest-round-trip rule and commits a value to force the
    decision. `adam-adsl-bmi-compute` is the second piece of evidence and the
    sharper one: it commits `24.999999999999996` where
    `adam-adsl-bmi-function` records `25` for the same formula and the same
    inputs. One of the two expected outputs is wrong, and nothing in R005 says
    which. `adam-adlb-bds` carries the same problem independently: its ALTSI
    `AVAL` of `0.167` is the shortened form of `0.16699999999999998`, and its
    `CHG` and `PCHG` inherit it. Three fixtures now disagree with
    full-precision arithmetic in their golden output.
18. A declared `date` is complete or nothing. Partial dates have no precision,
    so imputation is written as regular-expression extraction, string defaults,
    and reassembly, and the rule itself is invisible to the schema.
19. Imputed and collected dates compare identically. Nothing marks a comparison
    made under uncertainty, so an imputed day silently decides classifications
    such as treatment emergence.
20. There is no datetime type distinct from `date`.
    `sdtm-ae-effective-transaction` carries an audit timestamp as `str` and
    orders it correctly only because ISO 8601 text sorts chronologically.

### F. The output and pipeline contract stops at one dataset

21. One specification derives one dataset. `sdtm-suppmh-qualifiers` cannot
    assign a parent sequence and consume it in the same run, and
    `sdtm-dm-reference-dates` depends on DM being derived before the domains
    that reference it without being able to say so. R001 cycle detection is per
    specification, so a cross-dataset cycle cannot be reported either.
22. Nothing controls output row order, and verifications are row-wise over the
    completed output. Rows leave in row-template order rather than a
    submission sort order, and referential integrity between a SUPPQUAL record
    and its parent domain cannot be asserted.

### G. Structure that the data has cannot be declared

23. Conditional applicability, treatment period, relationship degree, and
    analysis window are all real structure in a protocol and none of them is a
    concept in the schema. Each is re-expressed as a filter, a literal in a
    predicate, or one row template per slot, so the specification grows with
    the data rather than describing the design. `sdtm-lb-conditional-compartments`,
    `adam-adsl-crossover-periods`, and `sdtm-relrec-many-to-many` each show a
    different face of this.
24. Row construction cannot consume values resolved during column derivation.
    A logically removed record cannot be dropped, because `row.filter` sees
    only the row driver and nothing deletes a row afterwards, as
    `sdtm-ae-effective-transaction` shows by committing a record that must not
    exist.
25. A derivation cannot carry both a value and the reason for it.
    `adam-adrs-composite-response` writes the same four predicates twice, once
    for the endpoint and once for its audit trail, with nothing linking them.
26. Metadata is an ungoverned string map. Labels are first class, but origin,
    length, and controlled terminology are free-form text that no
    implementation can validate, and no expected metadata artifact exists to
    assert them, as `sdtm-dm-metadata-contract` records.

### What the grouping changes

Groups A and B are mostly closed. `compute` removed the arithmetic
workarounds from group A in one registry entry, which is a better trade than
the several operator keywords first proposed: R010 states the numeric semantics
once, and R001 already extracted identifiers from SQL text, so dependency
inference needed no new machinery. What remains of A is cheap and unrelated to
arithmetic: accepting a variable in `cut.breaks` and window bounds. Giving
`order_by` a direction and a null placement has since landed, which closed
gaps 4 and 14 and removed the suite's last two workaround columns.
`plan.md` sequences the rest.

Groups C, D, F, and G are language additions rather than relaxations and need
their failure behavior fixed by negative fixtures before they are specified.
Group G is the largest of them and the least explored: it asks the schema to
describe study design, not just data movement.

Two things in the suite are now blocked on negative fixtures rather than on
design. R001 cycle reporting has no positive expression, so
`adam-adsl-dependency-order` can only prove that valid graphs are sorted
correctly. And every claim about fail-closed behavior in these READMEs is
still an assertion.

Positive fixtures do not prove failure behavior. Negative fixtures are still
needed for duplicate dictionary keys, unhandled mappings, failed verifications,
duplicate output keys, illegal expression contexts, and nested expressions in
variable-only operand fields.
