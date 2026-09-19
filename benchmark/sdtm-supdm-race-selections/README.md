# Record supplemental race selections

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-supdm-race-selections.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** draft - first commit, no review yet.

**Goal:** build one supplemental Demographics (SUPPDM) record per
reported race for a subject reporting several races, carrying
`IDVARVAL`, `QNAM`, `QLABEL`, `QVAL`, `QORIG`, and `QEVAL`.

**Input:** EDC output in long form, one row per collected item;
each race option a subject marks contributes one race row.

**Variables:**

- **IDVARVAL**: the subject identifier the supplemental record
  points back at.
- **QNAM**: qualifier name holding the position of the race in
  the subject list: `RACE1`, `RACE2`.
- **QLABEL**: qualifier label naming that position: `Race 1`,
  `Race 2`.
- **QVAL**: the reported race in the same controlled terms as the
  Demographics race: `WHITE`, `ASIAN`,
  `BLACK OR AFRICAN AMERICAN`,
  `AMERICAN INDIAN OR ALASKA NATIVE`,
  `NATIVE HAWAIIAN OR OTHER PACIFIC ISLANDER`, `OTHER`,
  `NOT REPORTED`, or `UNKNOWN`; only reported races contribute
  records, so it is never blank.
- **QORIG**: origin of the value, always case report form
  (`CRF`).
- **QEVAL**: blank, since a collected value is not an
  assessment.

**Note:** only a subject reporting several races contributes
records, one for the lowest reported race and one for the
highest; the example covers at most two reported races per
subject, so a third selection would not be represented;
records are ordered by study, then subject, then
qualifier name.

**Standard:** SDTM | **Domain:** SUPPDM
