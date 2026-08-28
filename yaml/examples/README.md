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
3. `adam-adlb-bds` — source-dataset enrichment, baseline selection, change from
   baseline, percentage change, and analysis sequence.

Each example contains a specification, source CSV files, an expected CSV, and
a README defining behavior that an implementation must reproduce.

## Schema probes

A probe is a fixture written to test whether the schema can express a real
derivation pattern. A probe that does not pass is a design finding, not a defect
in the fixture, and its README records what the schema could not express.

4. `adam-adsl-mapping` — value mapping with an inline dictionary, deriving
   ADaM numeric companions, including a value the dictionary does not define.
5. `sdtm-ae-dictionary-coding` — value mapping where the dictionary is an
   external file, including an uncoded term and the dictionary version.
6. `adam-adsl-bmi-function` — BMI derivation through a function supplied by the
   project's global R environment.
7. `adam-adsl-bmi-compute` — the same BMI derivation as one closed numeric
   expression, with a zero-height guard, `NULL` propagation, and a genuine
   rounding tie.

All four pass. The first two cover value standardization from inline and
external dictionaries. The third is the suite's only coverage of the
runtime-function extension point; the fourth is how the same arithmetic should
be written portably.

## Challenge probes

8. `sdtm-lb-multiform` — a compact CATH-derived ODM-to-SDTM probe consolidating
   serum, skin-biopsy, saliva, and tape-strip data into LB. It covers form-
   specific contextual dates, structural absence versus explicit missingness,
   a collected nonnumeric result, repeated item groups, heterogeneous specimen
   metadata, and deterministic ordering ties.
9. `adam-adsl-identifier-parsing` — parses a site from `USUBJID`, falls back
   to collected `SITEID`, and constructs a subject reference.
10. `adam-adsl-geography-normalization` — normalizes collected country text
    and maps country codes to regions, including missing and unmapped paths.
11. `adam-adsl-treatment-selection` — a standalone SDTM-to-ADaM probe deriving
    subject-level treatment from DM and EX. It covers filtered first/last dates,
    ordered treatment selection, placebo dose zero, no-match subjects, fallback,
    and inclusive duration.
12. `adam-adsl-disposition` — a standalone SDTM-to-ADaM probe deriving final
    subject disposition from DM and DS. It covers filtered final dates, ordered
    associated values, no-match subjects, and deterministic same-day selection.
13. `adam-adsl-population-flags` — derives safety and intent-to-treat flags
    from a small pre-derived ADSL slice.
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
    standardized companion separate, converting pounds and Fahrenheit as one
    formula each and committing an unrounded product.
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
25. `adam-adae-worst-severity` — selects the worst-severity event per
    preferred term, ordering a controlled vocabulary through a numeric proxy.
26. `sdtm-lb-conditional-compartments` — separates a structurally inapplicable
    compartment from an applicable sample that was not collected.
27. `sdtm-ae-effective-transaction` — selects the effective state of a record
    from an insert/update/remove transaction log.
28. `adam-adsl-crossover-periods` — derives period-scoped treatments and dates
    across a washout, including a subject who never crossed over.
29. `sdtm-relrec-many-to-many` — row construction from multiple source
    datasets, covering a one-to-many relationship between records and
    records that belong to two relationships at once.
30. `adam-adrs-composite-response` — combines an efficacy threshold, a safety
    condition, and a discontinuation rule into one responder value.
31. `adam-adsl-dependency-order` — declares every column in reverse dependency
    order, with dependencies reachable only through predicates and window
    fields.
32. `sdtm-dm-metadata-contract` — declares labels, origins, lengths, and
    codelists on every column and shows how little of that is governed.
33. `sdtm-suppmh-parent-linkage` — looks up a parent sequence on subject plus
    reported term, where the right side is unique on the pair and on neither
    column alone.

## Coverage

All 17 registered non-leaf expressions and all nine verification keywords are
exercised by at least one fixture. A few fixtures are the only coverage of
something and should not be deleted without a replacement:

- `adam-adsl-bmi-function` is the only use of the `function` extension point.
- `adam-adsl-dependency-order` is the only test of R001 dependency inference,
  and the only fixture whose column layout is deliberately not submission order.
- `adam-adlb-bds` is the only fixture where `compute` runs during row
  construction, so the only one exercising a row-driver-qualified identifier in
  a numeric expression.
- `sdtm-vs-unit-standardization` is the only evidence for R010's prohibition on
  reassociating a formula: `(VSORRESN - 32) * 5 / 9` returns exactly `37` where
  `(VSORRESN - 32) / 1.8` returns `36.99999999999999`.
- `adam-adsl-mapping` is the only use of the generic named `predicate`
  verification.
- `sdtm-suppmh-parent-linkage` is the only compound-key `mapping_from`.

No fixture rounds a value, and none can: R010 has no rounding function, so a
derivation carries full precision and the number of places shown is decided
when the value is reported.

## Open design gaps

Eighteen gaps are open across the suite, grouped by root cause rather than by
the fixture that found them, because most are consequences of a few underlying
decisions rather than independent omissions. Closed gaps are removed from this
list rather than marked; `plan.md` records what was closed and how.

### A. Literal operands and ordering

1. `cut.breaks` and window bounds are literal lists and cannot be read from a
   column, so an analysis window bound or a per-test grading threshold cannot
   be data. `compute` accepts a column in every operand position, so this is
   now confined to banding and to predicate literals:
   `adam-adlb-closest-visit` reads its target day from `AWTARGET` but still
   tests `ADY >= 8 AND ADY <= 22` against literals, and a second window needs
   another `case` branch.
2. Only `row_number` is registered. Without `rank` and `dense_rank`, ties can
   be broken but not preserved, so a flag cannot cover every record tied at a
   worst value and distinct-level counts cannot be expressed.
   `adam-adae-worst-severity` has two events tied on severity and date and
   flags exactly one.

A controlled vocabulary also still needs a `mapping` to give it a numeric proxy
before anything can order it. The order lives in a dictionary rather than in
the vocabulary.

### B. Joins

3. `mapping_from` returns one column per call, so reading several columns from
   one matched record repeats the match. `sdtm-vs-visit-study-day` calls it
   twice against one `TV` row. A multi-column return conflicts with one
   expression producing one value and belongs with gaps 6 and 7.
4. There is no interval join, so `EPOCH` cannot be assigned to a record the
   trial design does not name.

### C. Aggregates and selection operate on values, not rows

5. There is no row-wise maximum for dates. R010's `GREATEST` and `LEAST` are
   numeric, so a latest-of-several date is still the null-guarded `case` chain
   `sdtm-dm-reference-dates` writes, widening with each candidate.
6. An extreme value and the values associated with it come from two independent
   reductions that nothing ties to the same right-side record. A shared
   `filter` can make them see the same records, not the same one.
7. A missing aggregate result cannot distinguish no matching record from
   matching records whose values are all missing.
8. `row_number` cannot filter, so eligibility is expressed as a sort column
   that ranks ineligible records last.

### D. Types, conversion, and missing-value semantics

9. Source-format missing values and type inference have no normative rule.
   Every fixture assumes an empty CSV field is missing and distinguishes it
   from a nonempty malformed value.
10. Float-to-string conversion is undefined, and it is the only undefined cell
    left in R011's matrix. `sdtm-vs-unit-standardization` proposes a
    shortest-round-trip rule and commits `37` to force the decision.
    `adam-adsl-bmi-compute` commits `24.999999999999996` where
    `adam-adsl-bmi-function` records `25` for the same formula and inputs, and
    `adam-adlb-bds` carries an ALTSI `AVAL` of `0.167` for
    `0.16699999999999998`. Three golden outputs disagree with full-precision
    arithmetic and nothing says which is right.
11. Partial dates have no precision. R011 fixes that a declared `date` is
    complete or nothing, so imputation is written as regular-expression
    extraction, string defaults, and reassembly, and the rule itself is
    invisible to the schema.
12. Imputed and collected dates compare identically. Nothing marks a comparison
    made under uncertainty, so an imputed day silently decides classifications
    such as treatment emergence.

### E. The output and pipeline contract stops at one dataset

13. One specification derives one dataset. `sdtm-suppmh-qualifiers` cannot
    assign a parent sequence and consume it in the same run, and
    `sdtm-dm-reference-dates` depends on DM being derived before the domains
    that reference it without being able to say so. R001 cycle detection is per
    specification, so a cross-dataset cycle cannot be reported either.
14. Nothing controls output row order, and verifications are row-wise over the
    completed output. Rows leave in row-template order rather than a submission
    sort order, and referential integrity between a SUPPQUAL record and its
    parent domain cannot be asserted.

### F. Structure that the data has cannot be declared

15. Conditional applicability, treatment period, relationship degree, and
    analysis window are all real structure in a protocol and none is a concept
    in the schema. Each is re-expressed as a filter, a literal in a predicate,
    or one row template per slot, so the specification grows with the data
    rather than describing the design. `sdtm-lb-conditional-compartments`,
    `adam-adsl-crossover-periods`, and `sdtm-relrec-many-to-many` each show a
    different face of this.
16. Row construction cannot consume values resolved during column derivation.
    A logically removed record cannot be dropped, because `row.filter` sees
    only the row driver and nothing deletes a row afterwards, as
    `sdtm-ae-effective-transaction` shows by committing a record that must not
    exist.
17. A derivation cannot carry both a value and the reason for it.
    `adam-adrs-composite-response` writes the same four predicates twice, once
    for the endpoint and once for its audit trail, with nothing linking them.
18. Metadata is an ungoverned string map. Labels are first class, but origin,
    length, and controlled terminology are free-form text that no
    implementation can validate, and no expected metadata artifact exists to
    assert them, as `sdtm-dm-metadata-contract` records.

## Negative fixtures

There are none, and that is the suite's binding limitation. Every fail-closed
claim in these READMEs is an assertion rather than a tested behavior, and R001
cycle reporting has no positive expression, so `adam-adsl-dependency-order` can
only prove that valid graphs are sorted correctly.

Needed: duplicate dictionary keys, unhandled mappings, failed verifications,
duplicate output keys, illegal expression contexts, nested expressions in
variable-only operand fields, a column type outside `column_type`, the R011
conversion failures (unparseable numeric text, an incomplete date, a
non-integral value converted to `int`), and the four `mapping_from` compound-key
failures listed in `plan.md`.
