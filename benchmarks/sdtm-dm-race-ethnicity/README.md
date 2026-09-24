# Record race and ethnicity, with one supplemental record per race

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-race-ethnicity.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one Demographics (DM) record per subject carrying race
(`RACE`) and ethnicity (`ETHNIC`), and one supplemental Demographics
(SUPPDM) record per reported race for each subject who marked several.

**Input:** EDC output in long form, one row per collected item; each race
option a subject marks contributes one race row, and ethnicity contributes
its own row. A race marked twice counts once.

**Variables:**

- **RACE** (DM): reported race in controlled terms: `WHITE`, `ASIAN`,
  `BLACK OR AFRICAN AMERICAN`, `AMERICAN INDIAN OR ALASKA NATIVE`,
  `NATIVE HAWAIIAN OR OTHER PACIFIC ISLANDER`, `MULTIPLE` when several
  races were marked, `OTHER` for a free-text answer the sponsor does not
  map, `UNKNOWN` for a refused answer, and `NOT REPORTED` for a recorded
  non-answer; blank when no race item was marked.
- **ETHNIC** (DM): reported ethnicity in controlled terms:
  `HISPANIC OR LATINO`, `NOT HISPANIC OR LATINO`, `NOT REPORTED`, or
  `UNKNOWN`; blank when no ethnicity item was collected.
- **IDVARVAL** (SUPPDM): the subject identifier, linking the record to its
  DM row through `IDVAR` = `USUBJID`.
- **QNAM** / **QLABEL** (SUPPDM): `RACE1` / `Race 1`, `RACE2` / `Race 2`,
  and so on, one per distinct race the subject marked, numbered in
  alphabetical order of the collected answer because the input carries no
  report order.
- **QVAL** (SUPPDM): that race in controlled terms, as in **RACE**.
- **QORIG** (SUPPDM): the origin, always `CRF`; **QEVAL** is blank.

**Note:** SUPPDM carries race records exactly for the subjects whose DM
record says `MULTIPLE`, however many races they marked.

**Standard:** SDTM | **Domain:** DM
