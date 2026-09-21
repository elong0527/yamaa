# Parquet Review Log

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/spec-parquet-sidecar.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each subject's `AGE` into ADSL while writing the review
log in the Parquet container.

**Input:** the single `spec.yaml` over collected demographics carrying
age (`AGE`); one subject falls outside the expected 18-to-100 range.

**Note:** the review log is a side file the runner governs: its path
extension selects the container, so `adsl-violations.parquet` uses
Parquet while the primary artifact stays human-readable CSV. The log
records `LOG_VERSION`, `ARTIFACT`, `SEVERITY`, `CONDITION`,
`REQUIREMENT`, `SPEC_PATH`, `VERIFICATION_ID`, `FAILURE_COUNT`,
`OFFENDING_KEYS`, and `DETAILS`, so the out-of-range value can be
investigated without stopping the run.

**Standard:** ADaM | **Domain:** ADSL
