# Relate adverse events and medications sharing a link

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-relrec-many-to-many.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** emit one related-records row per relationship
participation, carrying `IDVARVAL`, `RELTYPE`, and `RELID`.

**Input:** collected adverse events (AE) each carrying two link
identifiers, plus collected concomitant medications (CM) each
carrying two link identifiers.

**Variables:**

- `IDVARVAL` is the sequence number of the related record as text;
  always present.
- `RELTYPE` is blank throughout, because each row points at one
  record rather than a whole dataset.
- `RELID` names the relationship the row takes part in; rows
  sharing a value are related to one another, and every row
  carries one.

**Note:** a record with no link identifier contributes no row,
while a record naming two link identifiers contributes one row per
identifier; carrying a third relationship would need another link
field on the collected record.

**Standard:** SDTM | **Domain:** RELREC
