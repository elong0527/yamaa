# Reject a First Visit Window Taken From Records Not Yet Built

**Goal:** carry each collected blood pressure measurement and add one
follow-up record per subject, with the study day in `VSDY`, the record's
category in `VSCAT`, the systolic blood pressure in `VSSTRESN`, and the visit
window in `VISIT`.

**Input:** collected measurements carrying the study day (`VSDY`) and the
systolic blood pressure (`SYSBP`), and a table of each subject's visit windows
from a lowest (`LO`) to a highest (`HI`) study day.

**Variables:**

- `VSDY` would be the measurement's study day, and on a follow-up record the
  subject's latest study day.
- `VSCAT` would be `COLLECTED` on a measurement and `FOLLOW-UP` on the
  subject's follow-up record.
- `VSSTRESN` would be the systolic blood pressure, blank on a follow-up
  record.
- `VISIT` would name the window, both ends inclusive, that holds the study
  day of the subject's latest record already built.

**Note:** a record's window is found from the subject's latest record built
before it. The collected measurements are the first records built, so for
them no earlier record exists, and the run is rejected before any data is
read and no artifact is accepted.

## How to fix

Give the collected measurements the window of their own study day, and keep
the latest-record window for the follow-up records, which are built after
them:

```yaml
intermediates:
  - id: OWNWIN
    dataset: WINDOWS
    key: [USUBJID]
    between: {value: VSDY, lower: LO, upper: HI}
```

```yaml
- id: collected
  dataset: VITALS
  derivations:
    VISIT: OWNWIN.NAME
```
