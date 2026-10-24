# Reject a standard deviation of text results

**Goal:** keep each replicate systolic blood pressure reading with its
result as text (`AVALC`), and add `AVALSD`, the standard deviation of the
subject's replicate readings.

**Input:** vital signs with one record per replicate reading, the result
collected as text (`VSORRES`) and as a number in standard units
(`VSSTRESN`).

**Variables:**

- `AVALSD` would be the sample standard deviation of the subject's replicate
  readings, the same on each of them. The request computes it from the text
  results, and a standard deviation needs numbers, so the run is rejected
  before any data is read.

## How to fix

Compute the deviation from the numeric result, carried in its own column:

```yaml
- name: AVAL
  type: float
  derivation: VS.VSSTRESN

- name: AVALSD
  type: float
  derivation:
    aggregate:
      group_by: [STUDYID, USUBJID, PARAMCD]
      expr: "STDDEV(AVAL)"
```

Converting the text results instead would read every collected spelling as
a number; use the standard numeric result the study already provides.
