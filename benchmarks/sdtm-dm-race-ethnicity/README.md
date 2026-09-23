# Record race and ethnicity in demographics

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-race-ethnicity.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one Demographics (DM) record per subject carrying race
(`RACE`) and ethnicity (`ETHNIC`).

**Input:** EDC output in long form, one row per collected item; each race
option a subject marks contributes one race row, and ethnicity contributes
its own row.

**Variables:**

- **RACE**: reported race in controlled terms: `WHITE`, `ASIAN`,
  `BLACK OR AFRICAN AMERICAN`, `AMERICAN INDIAN OR ALASKA NATIVE`,
  `NATIVE HAWAIIAN OR OTHER PACIFIC ISLANDER`, `MULTIPLE` when several
  races were marked, `OTHER` for a free-text answer the sponsor does not
  map, `UNKNOWN` for a refused answer, and `NOT REPORTED` for a recorded
  non-answer; blank when no race item was marked.
- **ETHNIC**: reported ethnicity in controlled terms:
  `HISPANIC OR LATINO`, `NOT HISPANIC OR LATINO`, `NOT REPORTED`, or
  `UNKNOWN`; blank when no ethnicity item was collected.

**Note:** a subject reporting several races carries `MULTIPLE` in **RACE**;
the companion `sdtm-supdm-race-selections` example carries one supplemental
record per reported race.

**Standard:** SDTM | **Domain:** DM
