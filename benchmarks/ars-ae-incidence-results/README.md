# Adverse Event Incidence Results

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/ars-ae-incidence-results.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** emit long-format adverse event incidence results, retaining the
analysis definition with every value in an Analysis Results Standard (ARS)
style dataset.

**Input:** subject analysis records with planned treatment, treatment code
and safety flag, and adverse event analysis records with emergence flag.

**Variables:**

- `ANALYSISID`, `ANALYSISSETID`, `DATASET`, `VARIABLE` and `METHODID` are
  AEINC, SAFETY, ADAE, USUBJID and AEINCIDENCE, respectively.
- `ANALYSISSET` is SAFFL = 'Y'; `DATASUBSET` is TRTEMFL = 'Y'. These
  conditions limit both the safety denominator and qualifying event count.
- `GROUPINGID` is TRT01P. `GROUPORDER` and `GROUPLEVEL` are the planned
  treatment code and name from subject analysis records.
- `OPERATIONID` is COUNT or PERCENT. `RESULT` is the number of safety
  subjects with at least one treatment-emergent adverse event, or that
  number as a percentage of all safety subjects in the treatment group.
  Neither result is rounded.

**Note:** repeated qualifying events count once per subject. A safety
treatment group without qualifying events retains both result rows at 0;
a group without safety subjects has no result rows. This flat results
dataset illustrates ARS analysis-to-result linkage, not an ARS document.

**Standard:** ARS | **Domain:** RESULTS
