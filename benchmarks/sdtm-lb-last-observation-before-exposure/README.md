# Last Observation Before Exposure

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-last-observation-before-exposure.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag the last laboratory record collected on or before
first exposure for each subject, test, and specimen: `LBLOBXFL`.

**Input:** long-form Operational Data Model (ODM) data with one row
per collected item: a laboratory result, its collection date, or
its completion status. A small test dictionary (`lb_mapping.csv`)
translates result items into test codes and specimens. One
demographics row per subject carries the reference start date
(first exposure).

**Variables:**

- `LBTESTCD` is the test code from the item dictionary; together
  with the specimen it defines the series the flag is assigned within.
- `LBSPEC` is the specimen from the item dictionary; a test measured
  in more than one specimen is flagged separately per specimen.
- `LBORRES` is the result as collected; missing on records that were
  not done.
- `LBDTC` is the collection date as collected.
- `LBSTAT` is the completion status as collected; `NOT DONE` on
  records without a result.
- `LBLOBXFL` is `Y` on the latest record with a result whose
  collection date falls on or before the subject's reference start
  date; blank on every other record.

**Note:** records without a result never carry the flag, even when
they carry the latest date. A subject with no reference start date
has the flag on the latest record with a result, since no record
falls after exposure. Dates compare at day precision, so a record
collected on the day of first exposure still qualifies; of two results
collected on the same date, the one with the higher sequence number
(`LBSEQ`) is flagged.

**Standard:** SDTM | **Domain:** LB
