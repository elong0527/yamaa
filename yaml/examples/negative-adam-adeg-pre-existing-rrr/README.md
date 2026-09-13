# Reject a collected RR interval

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adam-adeg-pre-existing-rrr.html)

**Goal:** keep each collected heart rate (HR) record and add a
rederived RR duration (time between successive R waves) record
under the code `RRR` for each subject and analysis visit with a
non-missing, non-zero HR result, labeled `RR Duration Rederived
(ms)` and carrying the new result in `AVAL` and its unit in
`AVALU`.

**Input:** collected electrocardiogram (ECG) records with heart
rate results under the code `HR` per subject and visit, carrying
the record label, result, and result unit; one input record
already carries the code `RRR`.

**Variables:**

- `AVAL`: the collected result on a kept record, or 60000 divided
  by the heart rate (in beats/min) on an added `RRR` record. A
  missing or zero heart rate adds no `RRR` record.
- `AVALU`: the collected unit on a kept record, or milliseconds
  (`ms`) on an added `RRR` record. A heart rate record that
  contributes a result must use `beats/min`.

Every `RRR` record must be produced from a heart rate by the
calculation above rather than accepted as collected, so the
collected `RRR` record breaks the stated requirement. The
completed dataset is presented to that check and rejected, and no
artifact is accepted from this input. The expected file records
the completed rows presented to the check.

**Standard:** ADaM | **Domain:** ADEG

## How to fix

First confirm the RR duration should come from the heart rate rather
than from the collected record: the collected `RRR` record is the
defect, not the calculation. Delete the collected `RRR` input record;
keep each contributing `HR` record with `beats/min` as the unit. The
run then calculates the RR interval itself instead of accepting one
as collected.
