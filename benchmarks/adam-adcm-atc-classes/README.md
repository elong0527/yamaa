# ATC Classes for Coded Medications

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adcm-atc-classes.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the coded medication name (`CMDECOD`) and its
Anatomical Therapeutic Chemical (ATC) classes at levels 1 to 4
into ADCM, as `ATC1`-`ATC4` (class names) and `ATC1CD`-`ATC4CD`
(codes).

**Input:** medication records carrying the reported name
(`CMTRT`) and the coded name (`CMDECOD`); Findings About records
(FACM) assigning one ATC level per row - each row names the level
(`FATESTCD` of `ATC1` to `ATC4`) and the code (`FAORRES`) for one
medication record; and a small ATC dictionary mapping each code
to its class name.

**Variables:**

- `CMSEQ`: the medication record sequence, so each medication's ATC
  path stays with its own record.
- `ATC1CD`, `ATC2CD`, `ATC3CD`, `ATC4CD`: the ATC code at each level,
  read from the medication's FACM rows.
- `ATC1`, `ATC2`, `ATC3`, `ATC4`: the ATC class name at each level,
  looked up from the dictionary by code.

**Note:** a medication with no coded name and no FACM rows (the
herbal tea) keeps all eight ATC columns empty, while a coded
medication always carries a full four-level path.

**Standard:** ADaM | **Domain:** ADCM
