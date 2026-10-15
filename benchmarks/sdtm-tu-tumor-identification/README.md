# Tumor Identification

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-tu-tumor-identification.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one record per identified tumor or lesion, carrying
the link identifier, the identification test and result, the
anatomical location, the identification method, the evaluator, and
the visit: `TULNKID`, `TUTESTCD`, `TUTEST`, `TUORRES`, `TUSTRESC`,
`TULOC`, `TULAT`, `TUMETHOD`, `TUEVAL`, `VISITNUM`, and `TUDTC`.

**Input:** collected lesion identification with one row per lesion,
carrying the collected lesion category, lesion number, location,
laterality, method, evaluator, visit, and identification date, plus
study, subject, and sequence (`TUSEQ`) identifiers.

**Variables:**

- `TUSEQ` numbers a subject's identification records in collection
  order, starting at 1.
- `TULNKID` is the link identifier tying the lesion to its
  assessments in the tumor results (TR) domain: `T` plus the lesion
  number for a target lesion, `NT` for a non-target lesion, `NEW`
  for a new lesion.
- `TUTESTCD` is `TUMIDENT` and `TUTEST` is `Tumor Identification`
  on every record.
- `TUORRES` is the lesion category as collected, standardized to
  `TARGET`, `NON-TARGET`, or `NEW`.
- `TUSTRESC` repeats the standardized category.
- `TULOC` is the anatomical location as collected, standardized to
  uppercase.
- `TULAT` is the laterality as collected, standardized to `LEFT`
  or `RIGHT`; empty when the site recorded none.
- `TUMETHOD` is the identification method as collected.
- `TUEVAL` names the evaluator as collected.
- `VISITNUM` is the visit number as collected.
- `TUDTC` is the date of the scan or examination that identified
  the lesion, as collected.

**Note:** a lymph-node target lesion is identified like any other
target lesion: it keeps a `T` link identifier and its category stays
`TARGET`; its location names it as a lymph node, and the results
domain measures its short axis. A new lesion is identified once, at
the visit where it first appears, and earlier visits keep no record
of it.

**Standard:** SDTM | **Domain:** TU
