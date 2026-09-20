# Carry each coded ATC path onto its own record

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adcm-atc-classes.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** draft - first commit.

**Goal:** carry each coded Anatomical Therapeutic Chemical (ATC) path onto
its own concomitant medication record, adding `CMDECOD`, `PATHSEQ`,
`ATC1`-`ATC4`, and `ATC1CD`-`ATC4CD`.

**Input:** medication records carrying sequence number (`CMSEQ`), reported
name (`CMTRT`), and standardized name, one record per coded ATC path, and
treatment start (`TRTSDT`) and end (`TRTEDT`) dates from the subject-level
analysis dataset (ADSL).

**Variables:**

- `CMDECOD`: standardized medication name; blank when no standard name is
  assigned.
- `PATHSEQ`: number of the ATC path within the medication, starting at 1.
- `ATC1`: anatomical main-group name; blank when the medication is uncoded.
- `ATC2`: therapeutic subgroup name; blank when the medication is uncoded.
- `ATC3`: pharmacological subgroup name; blank when the medication is uncoded.
- `ATC4`: chemical subgroup name; blank when the medication is uncoded.
- `ATC1CD`: anatomical main-group code; blank when the medication is uncoded.
- `ATC2CD`: therapeutic subgroup code; blank when the medication is uncoded.
- `ATC3CD`: pharmacological subgroup code; blank when the medication is
  uncoded.
- `ATC4CD`: chemical subgroup code; blank when the medication is uncoded.

**Note:** an uncoded medication keeps one record with `PATHSEQ` 1 and all ATC
names and codes blank. The coded terms are invented illustrations; no
dictionary was copied.

**Standard:** ADaM | **Domain:** ADCM
