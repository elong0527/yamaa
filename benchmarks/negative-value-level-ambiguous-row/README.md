# Reject a Record Two Value Descriptions Select

**Goal:** carry the vital-sign result (`AVAL`) for each subject, parameter and
timepoint, with descriptions of the values for the data-definition document.

**Input:** standardized vital signs, one record per subject, test and
timepoint, with the numeric result.

**Variables:**

- `PARAMCD` is the vital-sign test code.
- `ATPT` is the timepoint, lying or standing.
- `AVAL` would be the numeric result.

**Note:** each record may be described by at most one of a variable's value
descriptions. One description here is chosen by parameter and another by
timepoint, which nothing rules out until the data arrive: the standing
systolic pressure matches both, so the run stops once the results are
complete and no artifact is accepted.

## How to fix

Decide which description the standing systolic pressure should carry, and
make the conditions exclusive, for example by naming the parameter in the
standing description as well:

```yaml
value_level:
  - id: SYSBP
    where: {PARAMCD: [SYSBP], ATPT: [AFTER 5 MIN LYING]}
  - id: STANDING
    where: {PARAMCD: [SYSBP, DIABP], ATPT: [AFTER 3 MIN STANDING]}
```
