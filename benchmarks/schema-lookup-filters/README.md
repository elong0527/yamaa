# Lookup Filters

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-lookup-filters.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** pin the lookup filter machinery: a filter that reads the
current planned visit and so changes which donor record a lookup
resolves to, with qualified donor fields, an ordering, and keep-last
selection.

**Input:** `PLAN` carries the planned visits per subject; `LB`
carries lab records, one with a missing result and one outside the
planned visit list.

**Lookup:** the `DONOR` intermediate implements
last-observation-carried-forward. For each planned visit it keeps the
subject's latest lab record whose result is present and whose visit
number does not exceed the planned visit's. The filter compares each
lab record with the current planned visit's number while the lab fields
stay qualified with the lookup dataset; the eligible records sort by
visit number and the last is kept. A record with a missing result never
qualifies, so its planned visit carries the earlier value forward; a
lab record at an unplanned visit still qualifies by its visit number
and can serve a later planned visit. A planned visit earlier than every
lab record with a result finds no donor, so its result and day are
missing.

**Columns:** `VISIT` names the planned visit at visit number
`VISITN`; the carried-forward lab result and day land in `AVAL` and
`LBDY`.

**Standard:** CDISC | **Domain:** ADLB
