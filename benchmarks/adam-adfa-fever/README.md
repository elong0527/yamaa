# Fever Occurrence

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adfa-fever.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `PARAMCD`, `PARAM`, `AVAL`, `AVALC`, `ADT`, `SRCDOM`,
`SRCVAR`, and `SRCSEQ` for each qualifying temperature record in an
ADaM analysis dataset (ADFA).

**Input:** one row per collected Vital Signs (VS) result, carrying
the test code, the category, the numeric result, the unit, and the
date and time of the assessment.

**Variables:**

- `PARAMCD` and `PARAM` are always `FEVER` and `Fever Occurrence`.
- `AVALC` is `Y` for a temperature of 38 degrees Celsius or higher
  and `N` for a lower one; blank when the result is missing or its
  unit is not Celsius (`C`).
- `AVAL` is 1 when the occurrence flag is `Y`, 0 when it is `N`, and
  blank when the flag is blank.
- `ASEQ` numbers the subject's analysis records 1, 2, 3 ... in
  assessment-date order, the collected sequence breaking ties.
- `ADT` is the calendar date of the assessment.
- `SRCDOM`, `SRCVAR`, and `SRCSEQ` point back to the source result:
  always `VS`, `VSSTRESN`, and the collected sequence number.

**Note:** only temperature records in the reactogenicity category
qualify; other vital signs records give no row.

**Standard:** ADaM | **Domain:** ADFA
