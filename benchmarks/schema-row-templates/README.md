# Row Templates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-row-templates.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build output rows from input records and groups with the three
row-template shapes: record-driven templates with input filters, a case
split across several templates, and a grouped template whose own filter
decides each candidate row.

**Input:** `LB` carries one record per lab test per visit: subject, visit
number, test code, and result. The tests interleave in the file. Subject
`02` visit 2 has two records whose results are missing.

**Templates:**

- `alt` keeps each `ALT` record with a present result, writing the result
  as the analysis value (`AVAL`). The filter reads the input record
  before any row is built, so the missing-result record is dropped and
  never reaches a row.
- `ast` keeps each `AST` record with a present result. All `ALT` rows
  come before every `AST` row in the output even though the input
  interleaves the tests: templates are sections and concatenate in
  specification order.
- `visit_total` builds one candidate row per subject and visit, in the
  order each group first appears in the input, reducing the group to a
  single sum. Its filter reads the finished candidate's own columns
  after the reduction: a sum of 70, 95, or 30 fails the comparison and
  the candidate is dropped, while the visit whose results are all
  missing sums to missing, which is neither true nor false, and is
  dropped as well. Only the visit totaling 115 survives, with `DTYPE`
  set to `CALCULATION`.

**Shared columns:** the study identifier, subject, and visit number are
the same for every template, so they are stated once at column level.
The subject and visit read the input record on the record-driven
templates and the group keys on the grouped template.

**Standard:** ADaM | **Domain:** ADLB
