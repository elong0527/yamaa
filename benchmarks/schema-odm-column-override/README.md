# Replace Collected Sex with a Reviewed Unknown Value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-odm-column-override.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** record `SEX` as unknown for each subject after review, even when the
extract contains a collected answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item, including a repeated sex answer at another visit.

**Variables:**

- `SEX` is `U` for each subject. The reviewed value replaces the collected
  answer, including when that answer appears more than once.

**Standard:** SDTM | **Domain:** DM
