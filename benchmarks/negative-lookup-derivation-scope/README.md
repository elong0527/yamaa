# Reject a Visit Window Found From the Clinic Record for Home Rows

**Goal:** combine clinic and home blood pressure measurements, with the
study day in `VSDY`, where the measurement was taken in `SOURCE`, the
systolic blood pressure in `VSSTRESN`, and the visit window that holds the
study day in `VISIT`.

**Input:** clinic and home measurements, each carrying the study day
(`VSDY`) and systolic blood pressure (`SYSBP`), and a table of each subject's
visit windows from a lowest (`LO`) to a highest (`HI`) study day.

**Variables:**

- `SOURCE` would be `CLINIC` or `HOME`.
- `VSSTRESN` would be the systolic blood pressure.
- `VISIT` would name the visit window, both ends inclusive, that holds the
  measurement's study day, and be blank when none does.

**Note:** the window is found from a study day taken from the clinic record.
A home measurement's row is built from a home record and has no clinic record
of its own, so the run is rejected before any data is read and no artifact is
accepted.

## How to fix

Find the window from the row's own study day, which both kinds of record
supply:

```yaml
intermediates:
  - id: VISITWIN
    dataset: WINDOWS
    key: [USUBJID]
    between: {value: VSDY, lower: LO, upper: HI}
```
