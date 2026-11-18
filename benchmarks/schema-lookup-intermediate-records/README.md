# Read One Dose Span From Several Lookups

**Goal:** give each dose and sample row of a population PK dataset the
datetime of the subject's first dose and of the latest dose before the row,
while each dose row carries its number of additional doses (`ADDL`) and
interdose interval (`II`), all from each dose record's span, stated once.

**Input:** one `spec.yaml` over collected exposure records (EX), each with a
dose, a dosing frequency and start and end dates that may lack a time, and
concentration records (PC), each with a sample datetime.

**Variables:**

- `EVID`: `1` on a dose row, one per exposure record; `0` on a sample row,
  one per concentration record.
- `SEQ`: the exposure record's sequence number on a dose row, the
  concentration record's on a sample row.
- `ADTM`: the dose's start, a missing time taken as the start of the day, or
  the sample's datetime.
- `AMT`, `ADDL`, `II`: on a dose row, its dose, the number of whole
  intervals between its start and its end, a missing end time taken as the
  end of the day, and the interval in hours (`QD` 24, `BID` 12); empty on a
  sample row.
- `FADTM`: the start of the subject's earliest dose record.
- `PADTM`: the start of the latest dose record that starts strictly before
  `ADTM`; empty when none does, so a dose row never takes its own record and
  a sample taken at a dose's start takes the dose before it.

**Note:** the span is stated once, on the intermediate that drives the dose
rows. The two lookups name that intermediate as their `dataset` and read its
records, its derived fields included, as they would read an input's.
