# Execution Lifecycle

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-lifecycle.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** show the order a specification runs in: rows are built first,
then columns are added on top of the finished rows, and rows discarded
along the way never reach the final checks.

**Input:** one `spec.yaml` over one lab-results file. Each record carries
a subject (`USUBJID`), a visit (`VISIT`), a test code (`ALT` or `AST`),
and a result that arrives as text.

**Behavior:**

- A record with no result never becomes a row: it is dropped before any
  row is built. Each surviving record becomes one component row carrying
  its test code, the test's full name (`PARAM`), and its result (`AVAL`).
- One summary row per subject-visit carries that visit's lowest result.
  The visit whose results are all missing is discarded after its summary
  is computed, so the final completeness check on `AVAL` passes: it never
  sees the discarded row.
- Text results become numbers while the rows are built, so the two
  columns added afterwards read numbers, never text: `DOUBLE` holds twice
  the result, and `FLAG` marks results above 50 with `H`.
- Every row carries the study identifier `YAMAA-01` in `STUDYID`.

**Note:** building rows may change the row count while adding columns may
not: each finished row gets exactly one `DOUBLE` and one `FLAG`.

**Standard:** ADaM | **Domain:** ADLB
