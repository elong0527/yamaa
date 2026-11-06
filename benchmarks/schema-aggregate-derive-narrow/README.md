# Aggregate Narrows on Its Derived Record Date

**Goal:** demonstrate a qualified aggregate whose `between` bound and
`filter` read a value one of its per-record bindings computes (#4309):
`NPRE`, `MEANPRE` and `NDATED`.

**Input:** one specification, `spec.yaml`, over demographics (`DM`), whose
reference start date (`RFSTDTC`) is each subject's anchor date, and vital
signs (`VS`), whose collection dates (`VSDTC`) are text.

**Variables:**

- `ANCHDT`: the subject's anchor date, read from `RFSTDTC`.
- `NPRE`: the number of the subject's weight records collected on or before
  the anchor date, the anchor date itself included. A weight with no
  collection date is not counted, and a subject with no such weight has a
  missing count.
- `MEANPRE`: the mean of the same weights, missing when there are none.
- `NDATED`: the number of the subject's weight records that have a
  collection date, missing for a subject with none.

**Note:** each aggregate derives the record date `MEASDT` from `VSDTC` for
every vital-signs record of the subject, then keeps the records its range
and filter hold for, reading `MEASDT` beside the stored fields, and reduces
those. Subject 001's height record is not a weight; subject 002's second
weight has no date; subject 003's weights all fall after the anchor date;
subject 004 has no vital signs.
