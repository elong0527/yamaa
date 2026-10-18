# Reject a Systolic Record Without a Result

**Goal:** carry the vital-sign result (`AVAL`) for each subject and
parameter, where a systolic pressure must always have a result.

**Input:** standardized vital signs, one record per subject and test, with
the numeric result when one was obtained.

**Variables:**

- `PARAMCD` is the vital-sign test code.
- `AVAL` would be the numeric result; it may be missing for any parameter but
  systolic pressure.

**Note:** the requirement applies to the systolic records alone: a missing
diastolic result is accepted, while a systolic record without a result stops
the run once the results are complete, and no artifact is accepted.

## How to fix

Lead with the data: query the site for the missing systolic measurement and
correct the source record. When a missing systolic value is acceptable for
the study, say so in the systolic description instead:

```yaml
- id: SYSBP
  where: {PARAMCD: [SYSBP]}
  mandatory: false
```
