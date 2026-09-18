# Clean event text and reference numbers

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-string-handlers.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** reviewed - has discussion comments or GitHub issues.

**Goal:** standardize adverse event terms and causality for analysis
and pull reference numbers from identifiers.

**Input:** AE records carrying `AESPID`, `AETERM`, and `AEREL`, with
`AETERM` carried through as collected.

**Variables:**

- `AEREFNUM` is the number taken from `AESPID` values shaped like
  `AE-001`; a missing identifier gives `0` and any other shape gives
  `-1`.
- `AETERMLO` is `AETERM` (reported term) in lower case.
- `AERELLC` is `AEREL` (causality) in lower case; a missing causality
  gives `not reported`.
- `AREL` is `AERELLC` in upper case, holding `RELATED`, `POSSIBLY
  RELATED`, `NOT RELATED`, or `NOT REPORTED`.

**Note:** a blank `AESPID` counts as missing, so it gives the `0`
for no recorded identifier.

**Standard:** ADaM | **Domain:** ADAE
