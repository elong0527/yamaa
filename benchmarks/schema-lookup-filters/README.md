# Lookup Filters

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-lookup-filters.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** pin the lookup filter machinery: a driver-correlated filter
that changes which donor record a lookup resolves to, with qualified
donor fields, an ordering, and keep-last selection.

**Input:** `PLAN` carries the planned visits per subject; `LB`
carries lab records, one with a missing result and one outside the
planned visit list.

**Lookup:** the `DONOR` intermediate implements
last-observation-carried-forward. For each planned visit it keeps the
subject's latest lab record whose result is present and whose visit
number does not exceed the driver's. The filter correlates with the
driver row through the driver's visit number while donor fields stay
qualified with the lookup dataset; the eligible records sort by visit
number and the last is kept. The missing-result record never qualifies,
so week 1 carries the baseline value forward; the off-schedule record
still qualifies by visit number and serves week 4. The subject whose
first lab record comes after baseline finds no donor and reads
missing.

**Columns:** `VISIT` names the planned visit at visit number
`VISITN`; the carried-forward lab result and day land in `AVAL` and
`LBDY`.

**Standard:** CDISC | **Domain:** ADLB
