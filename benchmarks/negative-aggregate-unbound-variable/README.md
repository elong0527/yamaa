# Reject a Total That Sums an Unnamed Per-Record Value

**Goal:** give each subject's total dose in milligrams (`TOTDOSE`) on the
subject-level analysis dataset.

**Input:** subjects from the collected demographics, and collected exposure
records with the dose given (`EXDOSE`) stored as text, several per subject.

**Variables:**

- `TOTDOSE` would be the sum of the subject's doses, each read as a number.

**Note:** each record's dose is read as a number under the name `DOSEMG`,
but the total sums `DOSE_MG`, a name given to no per-record value. Nothing
says what that name holds, so the run is rejected before any data is read
and no artifact is accepted.

## How to fix

Sum the per-record value under the name it was given:

```yaml
aggregate:
  key: [STUDYID, USUBJID]
  derivations:
    DOSEMG:
      type: float
      derivation: EX.EXDOSE
  expr: "SUM(DOSEMG)"
```
