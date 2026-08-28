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

All nine verification keywords are exercised across the fixtures;
`adam-adsl-mapping` covers the generic named `predicate`, while the challenge
probes cover `all_or_none` and `implies`.

## Edge-case assessment

The expanded fixtures demonstrate that local handlers remain readable for
missing contextual items, unmapped terminology, failed numeric conversion, and
missing inputs to banding. Row filters handle absent optional records, and the
zero-baseline rule produces an intentional missing percentage without a special
handler.

Eleven design gaps became visible:

1. Operations now consume named variables, which removes arbitrary expression
   nesting and keeps mappings concise. A future named-intermediate or
   definitions mechanism may still be useful when a real multi-step
   transformation should not create an output column.
2. The fixtures assume an empty CSV field is missing and distinguish it from a
   nonempty malformed value. Source-format missing-value and type-inference
   behavior needs a normative ingestion rule before implementations can be
   portable.
3. Hierarchical first-occurrence flags can use an eligibility sort column for
   a Boolean treatment-emergence rule, but `row_number` still cannot exclude
   ineligible rows. More general conditional windows need an explicit filter.
4. `mapping_from` returns one column per call, so a multi-column visit or
   parameter lookup repeats the same dictionary match. There is no explicit
   equality join that copies several columns, and no interval join that could
   assign `EPOCH` to an unscheduled record.
5. Analysis windows are expressible with `cut`, and first-in-window selection
   is expressible with `row_number`. Closest-to-target selection is not: no
   expression computes a distance to a declared target day, so the two records
   competing for one window in `adam-advs-analysis-visit` are resolved by
   arrival order instead of proximity.
6. Unit conversion needs `divide` and `round`, which are unregistered, and it
   needs a literal subtrahend, which `subtract` does not accept. `multiply`
   takes only a literal factor, so each unit pair needs its own `case` branch.
7. R007 defines missing-input handlers for mapping, banding, and string
   expressions but not for arithmetic. Specifications must guard `add`,
   `subtract`, and `multiply` with explicit predicates until a rule states
   whether arithmetic propagates missing or fails.
8. `VSSTRESC` needs the character form of a float. Deriving it by declared type
   alone depends on the unresolved R005 conversion matrix;
   `sdtm-vs-unit-standardization` proposes a shortest-round-trip rule.
9. SUPPQUAL linkage needs an explicit multi-column equality join. The R003
   automatic join uses only output keys shared with the right side, and
   `mapping_from` takes a single key column, so a subject-plus-repeat-key match
   cannot be written.
10. Nothing controls output row order. `keys` declares identity and column
    order controls layout, but rows leave in row-template order, which is not
    the conventional SUPPQUAL sort order.
11. Verifications are row-wise over the completed output, so referential
    integrity between a SUPPQUAL record and its parent domain cannot be
    asserted.

Positive fixtures do not prove failure behavior. Negative fixtures are still
needed for duplicate dictionary keys, unhandled mappings, failed verifications,
duplicate output keys, illegal expression contexts, and nested expressions in
variable-only operand fields.
