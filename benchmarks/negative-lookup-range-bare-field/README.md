# Reject an Analysis Window Placed by a Day the Dataset Does Not Carry

**Goal:** give each laboratory result its analysis visit (`AVISIT`,
`AVISITN`) from the window that holds its study day (`ADY`), next to its
parameter (`PARAMCD`) and value (`AVAL`).

**Input:** collected laboratory results with their study day, and a table of
analysis windows, each from a first to a last study day.

**Variables:**

- `ADY` would be the study day the result was collected on.
- `AVISITN` and `AVISIT` would be the number and name of the window whose
  first and last days, both inclusive, hold that day, and blank when no
  window does.

**Note:** a window is chosen by a value of the row being built. The day it
names is a field of the collected record that the dataset does not carry, so
the run is rejected before any data is read; the field is never read in its
place, and no artifact is accepted.

## How to fix

Choose the window by the analysis day the dataset carries:

```yaml
between: {value: ADY, lower: LO, upper: HI}
```

Or read the collected day from the record each row is built from, naming
the dataset:

```yaml
between: {value: LB.LBDY, lower: LO, upper: HI}
```
