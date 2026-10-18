# Reject a Sequence Number Ordered by the Clinic Visit Day

**Goal:** combine blood pressure measured at clinic visits with blood pressure
read at home, with the study day in `VSDY`, where the blood pressure was
measured in `VSCAT`, the systolic blood pressure in `VSSTRESN`, and each
subject's records numbered in order of study day in `VSSEQ`.

**Input:** clinic measurements, one per visit, and home readings, one per day.
Both carry the study day (`VSDY`) and the systolic blood pressure (`SYSBP`).

**Variables:**

- `VSCAT` would be `CLINIC` for a clinic measurement and `HOME` for a home
  reading.
- `VSSTRESN` would be the systolic blood pressure of the measurement or the
  reading.
- `VSSEQ` would number each subject's records from 1, the earliest study day
  first, whether a record is a clinic measurement or a home reading.

**Note:** the numbering takes each record's day from the clinic measurement it
was built from. A record built from a home reading has no clinic measurement,
so that day is not a value of the record. The run is rejected before any data
is read, and no artifact is accepted.

## How to fix

Number the records by a day that every record carries. `VSDY` already holds
it, taken from the clinic measurement or the home reading that built the
record, so order by that column:

```yaml
VSSEQ:
  row_number:
    window:
      group_by: [USUBJID]
      order_by: [VSDY]
```

When the order needs a value that only one source holds, such as the time of
a clinic visit, first bring it onto every record as its own column, filled
from each record's own source, and order by that column.
