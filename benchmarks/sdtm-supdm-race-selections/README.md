# Carry each reported race in supplemental demographics

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-supdm-race-selections.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry one supplemental Demographics (SUPPDM) record per distinct
reported race for subjects who marked several races.

**Input:** EDC output in long form, one row per collected item; each race
option a subject marks contributes one race row. This example shares its
input with `sdtm-dm-race-ethnicity`.

**Variables:**

- **RDOMAIN**: the related domain, always `DM`.
- **IDVAR** / **IDVARVAL**: the identifying variable and its value,
  `USUBJID` and the subject identifier, linking the record to its DM row.
- **QNAM**: the qualifier name, `RACE1` for the alphabetically first reported
  race and `RACE2` for the second; only subjects with several distinct
  reported races appear. The input carries no report order, so numbering
  follows alphabetical order.
- **QLABEL**: the qualifier label, `Race 1` or `Race 2`.
- **QVAL**: the reported race in controlled terms, as in **RACE** of the
  companion example.
- **QORIG**: the origin, always `CRF`; **QEVAL** is blank.

**Note:** only subjects whose DM record carries `MULTIPLE` get supplemental
records here. This example covers up to two reported races per subject; the
`RACE1`..`RACEn` naming carries on for more.

**Standard:** SDTM | **Domain:** SUPPDM
