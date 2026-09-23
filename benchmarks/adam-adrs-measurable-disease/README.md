# Flag Measurable Disease at Baseline

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adrs-measurable-disease.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `AVALC` and `AVAL` flagging whether each subject had
measurable disease at baseline.

**Input:** one record per subject from the subject-level analysis
dataset (ADSL), plus tumor identification (TU) records carrying the
tumor test code (`TUTESTCD`), the visit (`VISIT`), the standardized
result (`TUSTRESC`), and a sequence number (`TUSEQ`).

**Variables:**

- `AVALC` is `Y` when the subject has at least one screening tumor
  identification record (`TUTESTCD` of `TUMIDENT`, `VISIT` of
  `SCREENING`) whose result is target disease (`TUSTRESC` of
  `TARGET`); otherwise `N`. A screening record with a blank result,
  or a result such as `NON-TARGET` or `BENIGN ABNORMALITY`, is not
  target disease, and neither are target records from later visits.
- `AVAL` is `1` when `AVALC` is `Y` and `0` when `AVALC` is `N`.

**Note:** every subject in the subject-level dataset gets one flag
record, even with no screening tumor assessment, so such a subject is
`N` / `0`. A tumor record for a subject outside the subject-level
dataset contributes to no flag and creates none.

**Standard:** ADaM | **Domain:** ADRS
