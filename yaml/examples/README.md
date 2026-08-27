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

## Challenge probe

8. `sdtm-lb-multiform` — a compact CATH-derived ODM-to-SDTM probe consolidating
   serum, skin-biopsy, saliva, and tape-strip data into LB. It covers form-
   specific contextual dates, structural absence versus explicit missingness,
   a collected nonnumeric result, repeated item groups, heterogeneous specimen
   metadata, and deterministic ordering ties.
9. `adam-adsl-treatment-disposition` — a standalone SDTM-to-ADaM probe deriving
   subject-level treatment and disposition from DM, EX, and DS. It covers
   filtered first/last dates, ordered associated values, placebo dose zero,
   no-match subjects, identifier fallback, inclusive duration, and final
   disposition selection.

## Coverage gaps

Of the 20 registered non-leaf expressions, only `str_lower` is not exercised by
any fixture.

The remaining unexercised handler behaviors are `str_extract.missing` and
`override`; `str_lower`, including its `missing` path, is also uncovered.

`sdtm-lb-multiform` covers `case` and implication checks.
`adam-adsl-treatment-disposition` covers `add`, `coalesce`, `date_diff`,
`min`/`max` aggregate filters and R003 right-side reduction, `str_concat`,
`str_extract.no_match`, `str_upper`, ordered `multiple_matches`, and grouped
completeness verification.

All nine verification keywords are exercised across the fixtures;
`adam-adsl-mapping` covers the generic named `predicate`, while the two
challenge probes cover `all_or_none` and `implies`.

## Edge-case assessment

The expanded fixtures demonstrate that local handlers remain readable for
missing contextual items, unmapped terminology, failed numeric conversion, and
missing inputs to banding. Row filters handle absent optional records, and the
zero-baseline rule produces an intentional missing percentage without a special
handler.

Two design gaps became visible:

1. Operations now consume named variables, which removes arbitrary expression
   nesting and keeps mappings concise. A future named-intermediate or
   definitions mechanism may still be useful when a real multi-step
   transformation should not create an output column.
2. The fixtures assume an empty CSV field is missing and distinguish it from a
   nonempty malformed value. Source-format missing-value and type-inference
   behavior needs a normative ingestion rule before implementations can be
   portable.

Positive fixtures do not prove failure behavior. Negative fixtures are still
needed for duplicate dictionary keys, unhandled mappings, failed verifications,
duplicate output keys, illegal expression contexts, and nested expressions in
variable-only operand fields.
