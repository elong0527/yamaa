# Row Templates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-row-templates.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build output rows from input records and groups with the three
row-template shapes: record-driven templates with input filters, a case
split across several templates, and a grouped template whose own filter
decides each candidate row.

**Input:** one `spec.yaml` over `LB`, which carries one record per lab
test per visit: subject, visit number, test code, and result, with the
tests interleaved in the file. A result may be missing.

**Templates:**

- `alt` makes one `ALT` row per `ALT` record that has a result, writing
  the result as the analysis value (`AVAL`) and leaving `DTYPE` blank.
  Its filter reads the input record, so a record with a missing result
  is dropped before any row is built.
- `ast` does the same for `AST`. Every `ALT` row comes before every
  `AST` row even though the input interleaves the tests: templates are
  sections of the output and concatenate in specification order.
- `visit_total` builds one candidate `TOTAL` row per subject and visit,
  in the order each pair first appears in the input, with `AVAL` the
  sum of that visit's results, skipping missing ones. Its filter reads
  the finished candidate and keeps it only when the total exceeds 100,
  with `DTYPE` set to `CALCULATION`. A visit whose results are all
  missing sums to missing, which is neither above 100 nor not, so its
  candidate is dropped too.

**Shared columns:** the study identifier, subject, and visit number are
the same for every template, so they are stated once at column level.
The subject and visit read the input record on the record-driven
templates and the group keys on the grouped template.

**Standard:** ADaM | **Domain:** ADLB
