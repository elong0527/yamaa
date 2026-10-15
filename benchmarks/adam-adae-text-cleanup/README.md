# Text Cleanup

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-text-cleanup.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** standardize the free text a site enters for each adverse event
(the reported term and the causality) and pull the reference number out of
the sponsor's identifier.

**Input:** the site's AE records (`input/ae.csv`) carrying the collected
`AESPID`, `AETERM`, and `AEREL`.

**Variables:**

- `AEREFNUM` is the number taken from `AESPID` values shaped like `AE-001`;
  a missing identifier gives `0` and any other shape gives `-1`.
- `AETERMLO` is the reported term in lower case.
- `AERELLC` is the causality in lower case; a missing causality gives `not
  reported`.
- `AREL` is the analysis causality in upper case: `RELATED`, `POSSIBLY
  RELATED`, `NOT RELATED`, or `NOT REPORTED`.

**Note:** the identifier pattern accepts exactly three digits, so `AE-000`
gives `0` (the same value a missing identifier gives), while `AE-1000`
and a lowercase `ae-007` each give `-1`.

**Standard:** ADaM | **Domain:** ADAE
