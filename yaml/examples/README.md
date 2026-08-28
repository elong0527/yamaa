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

All three pass. The first two cover value standardization from inline and
external dictionaries. The third exercises the runtime-function extension
point with source and literal arguments.

## Challenge probes

8. `sdtm-lb-multiform` — a compact CATH-derived ODM-to-SDTM probe consolidating
   serum, skin-biopsy, saliva, and tape-strip data into LB. It covers form-
   specific contextual dates, structural absence versus explicit missingness,
   a collected nonnumeric result, repeated item groups, heterogeneous specimen
   metadata, and deterministic ordering ties.
9. `adam-adsl-identifier-parsing` — parses a site from `USUBJID`, falls back to
   collected `SITEID`, and constructs a subject reference.
10. `adam-adsl-geography-normalization` — normalizes collected country text and
    maps country codes to regions, including missing and unmapped paths.
11. `adam-adsl-treatment-selection` — selects actual treatment and first/last
    exposure dates, including placebo dose zero, no EX match, and inclusive
    duration.
12. `adam-adsl-disposition` — selects final disposition values, including
    same-day ties, protocol milestones, no DS match, and screen failure.
13. `adam-adsl-population-flags` — derives safety and intent-to-treat flags from
    a small pre-derived ADSL slice.
14. `adam-adae-treatment-emergent` — classifies AE start dates against an
    inclusive ADSL treatment interval, including both boundaries and a subject
    with no ADSL match.
15. `adam-adae-occurrence-flags` — derives first treatment-emergent occurrence
    flags at subject, SOC, and preferred-term levels, including same-day ties.
16. `adam-adae-string-handlers` — isolates lowercase normalization and the
    distinct missing/no-match paths for sponsor event identifiers.
17. `adam-adae-severity-override` — applies one approved final correction and
    demonstrates that a dependent numeric severity sees the corrected value.
18. `sdtm-vs-visit-study-day` — looks up visit metadata by an explicit key and
    derives study day under the SDTM no-Day-0 rule, including an unscheduled
    visit, a missing date, and a subject with no reference date.
19. `adam-advs-analysis-visit` — assigns records to analysis windows by study
    day, separates `AVISIT`/`AVISITN` from collected `VISIT`/`VISITNUM`, and
    flags one analysis record per window.
20. `sdtm-vs-unit-standardization` — keeps a collected result and its
    standardized companion separate, converting pounds and Fahrenheit without
    `divide` or `round`.
21. `sdtm-suppmh-qualifiers` — reshapes non-standard qualifier columns into
    SUPPQUAL rows and links them to a pre-assigned parent sequence.
22. `sdtm-dm-reference-dates` — reduces EX, DS, and AE into the DM reference
    dates, including a three-way latest-participation date, a screen failure
    with no exposure, and an adverse event ending after the last dose.
23. `adam-adae-partial-dates` — rebuilds analysis dates from year-only,
    year-month, complete, unparseable, and uncollected values, with an
    imputation flag and its effect on treatment emergence.
24. `adam-adlb-closest-visit` — selects the record nearest a target study day,
    including a tie broken toward the later record and a record outside the
    window.
25. `adam-adae-worst-severity` — selects the worst-severity event per preferred
    term, ordering a controlled vocabulary through a numeric proxy.
26. `sdtm-lb-conditional-compartments` — separates a structurally inapplicable
    compartment from an applicable sample that was not collected.
27. `sdtm-ae-effective-transaction` — selects the effective state of a record
    from an insert/update/remove transaction log.
28. `adam-adsl-crossover-periods` — derives period-scoped treatments and dates
    across a washout, including a subject who never crossed over.
29. `sdtm-relrec-many-to-many` — represents records belonging to two
    relationships at once.
30. `adam-adrs-composite-response` — combines an efficacy threshold, a safety
    condition, and a discontinuation rule into one responder value.
31. `adam-adsl-dependency-order` — declares every column in reverse dependency
    order, with dependencies reachable only through predicates and window
    fields.
32. `sdtm-dm-metadata-contract` — declares labels, origins, lengths, and
    codelists on every column and shows how little of that is governed.

## Coverage gaps

All 20 registered non-leaf expressions are now exercised by at least one
fixture. `adam-adae-string-handlers` closes the previously identified
`str_lower` and `str_extract.missing` gaps;
`adam-adae-severity-override` closes the `override` gap.

`sdtm-lb-multiform` covers `case` and implication checks.
The five focused ADSL probes separately cover identifier parsing and fallback,
geography normalization, treatment selection and duration, disposition
selection, and population flags. Together they retain the combined fixture's
`add`, `coalesce`, `date_diff`, filtered `min`/`max`, `str_concat`,
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
`sdtm-vs-unit-standardization` is the first fixture to nest `multiply` and
`mapping` inside `case` branches and the first to use a negative `add` addend
in place of an unavailable literal subtraction.
`sdtm-suppmh-qualifiers` is the first fixture to build a SUPPQUAL structure
and the first to reshape several qualifier columns of one row driver into rows.
`sdtm-dm-reference-dates` is the first fixture to reduce three different
right-side datasets into one output row and the first to combine an aggregate
with an ordered selection over the same records.
`adam-adae-partial-dates` is the first fixture to treat a date as text,
exercising `str_extract`, `coalesce` defaults, and `str_concat` together, and
the first to use every declared handler on one source value.
`adam-adlb-closest-visit` closes the closest-to-target question left open by
`adam-advs-analysis-visit`: the selection is expressible, using a spelled-out
absolute value and a negated companion column for descending preference.
`adam-adae-worst-severity` extends that answer to a categorical criterion: a
controlled vocabulary can be ordered by preference only after a `mapping` gives
it a numeric proxy to negate.
The seven Priority 2 fixtures added last cover conditional compartments,
transactional sources, crossover periods, many-to-many relationships, composite
endpoints, dependency ordering, and the metadata contract.
`adam-adsl-dependency-order` is the only fixture that tests R001 dependency
inference directly, and the only one whose column layout is deliberately not
submission order.

All nine verification keywords are exercised across the fixtures;
`adam-adsl-mapping` covers the generic named `predicate`, while the challenge
probes cover `all_or_none` and `implies`.

## Edge-case assessment

The fixtures demonstrate that local handlers remain readable for missing
contextual items, unmapped terminology, failed numeric conversion, and missing
inputs to banding. Row filters handle absent optional records, and the
zero-baseline rule produces an intentional missing percentage without a special
handler.

Twenty-six design gaps have been recorded across the suite, of which one is
now closed. They are grouped by root
cause rather than by the fixture that found them, because most of them are
consequences of six underlying decisions rather than independent omissions.

### A. Operands must be literals, and ordering is ascending and numeric

This single constraint produces more workarounds than any other.

1. Arithmetic takes a variable source and literal operands only. `add.addend`,
   `multiply.factor`, `cut.breaks`, and window bounds cannot be read from a
   column, so a target day, conversion factor, or window bound cannot be data.
2. `subtract` types both operands as variables and so cannot subtract a
   literal, while `add` accepts one. The two are asymmetric and only `add` is
   usable for a literal offset.
3. `divide`, `round`, and absolute value are unregistered. A reciprocal literal
   replaces division, a `case` that multiplies by `-1` replaces absolute value,
   and nothing replaces rounding.
4. `row_number.order_by` is ascending with no direction option. Preferring a
   later or larger value requires a negated companion column. A controlled
   vocabulary can be ordered only after a `mapping` gives it a numeric proxy;
   a categorical with no meaningful numeric order still cannot be ranked by
   preference.
5. Only `row_number` is registered. Without `rank` and `dense_rank`, ties can
   be broken but not preserved, so a flag cannot cover every record tied at a
   worst value and distinct-level counts cannot be expressed.

The consequence is that one concept in the protocol becomes several unlinked
pieces of specification. In `adam-adlb-closest-visit` a window's bounds, its
target, and the distance to that target are three separate constructs, and the
target day appears both as a column and as a literal with nothing keeping them
consistent. In `sdtm-vs-unit-standardization` each unit pair needs its own
`case` branch. Closest-to-target and worst-severity selection are both
expressible, but only through these workarounds.

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

10. There is no row-wise maximum over several derived columns. `min` and `max`
    reduce one right-side dataset and `coalesce` returns the first non-missing
    value, so a latest-of-several date is written as null-guarded `case`
    branches that grow with each candidate.
11. An extreme value and the values associated with it come from two
    independent reductions that nothing ties to the same right-side record.
12. A missing aggregate result cannot distinguish no matching record from
    matching records whose values are all missing.
13. `row_number` cannot filter. An eligibility sort column expresses a Boolean
    condition, but a general conditional window needs an explicit filter.
    Ordered `source.multiple_matches` cannot be filtered either.
14. Ordering across missing values is undefined. Eligibility sort columns keep
    ineligible records out of contention without defining how two of them
    compare, so fixtures must avoid the case rather than specify it.

### E. Types, conversion, and missing-value semantics are unresolved

15. Source-format missing values and type inference have no normative rule. The
    fixtures assume an empty CSV field is missing and distinguish it from a
    nonempty malformed value.
16. R007 defines missing-input handlers for mapping, banding, and string
    expressions but not for arithmetic. Specifications must guard `add`,
    `subtract`, and `multiply` with explicit predicates until a rule states
    whether arithmetic propagates missing or fails.
17. Float-to-string conversion is undefined. `sdtm-vs-unit-standardization`
    proposes a shortest-round-trip rule and commits a value to force the
    decision.
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

Group B is closed. Group A is the remaining cheap one: allowing a variable
wherever a literal operand is accepted, adding `divide`, `round`, and absolute
value, and giving `order_by` a direction would remove workarounds from at least
seven fixtures without changing any semantics already fixed by a golden output.
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
