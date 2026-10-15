# Supplemental Qualifiers

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-suppmh-qualifiers.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one supplemental record per collected qualifier,
carrying `IDVARVAL`, `QLABEL`, `QVAL`, `QORIG`, and `QEVAL`, with
each record pointing back at its parent medical history (MH)
record.

**Input:** a medical history extract with one record per condition,
carrying the parent record sequence and the two collected
qualifiers.

**Variables:**

- `IDVARVAL` is the parent record sequence, written as text; a
  parent record with no sequence number stops the run.
- `QLABEL` is the qualifier label: `Family History` for `MHFAMHX`,
  `Confirmed by Medical Records` for `MHCONF`.
- `QVAL` is the collected qualifier value, `Y` or `N`.
- `QORIG` records the collected origin as `Collected` (Define-XML 2.1).
- `QEVAL` is blank, since a collected value is not an assessment.

**Note:** each parent record contributes one supplemental record
per qualifier actually collected, so a record with only one of the
two collected contributes only one. Records are ordered by study,
then subject, then parent record sequence compared as text (so 10
sorts before 2), then qualifier name.

**Standard:** SDTM | **Domain:** SUPPMH
