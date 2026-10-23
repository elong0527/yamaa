# Reject a Missing-Weight Limit Above One

**Goal:** record each subject's baseline weight (`WEIGHTBL`) from the
collected demographics, allowing only a limited share of the subjects to
lack one.

**Input:** collected demographics carrying each subject's weight.

**Variables:**

- `WEIGHTBL` would be the subject's weight in kilograms at baseline, copied
  from the collected records, and missing where none was collected.

**Note:** the share of subjects allowed to lack a weight was entered as 1.5,
more than every subject. A share lies from zero through one, so the
specification is rejected as written and no artifact is accepted.

## How to fix

State the limit the protocol or the analysis plan sets, as a share from 0
through 1; 15 percent is 0.15:

```yaml
verifications:
  - not_missing: {max_fraction: 0.15}
```

When any number of subjects may lack a weight, the column needs no presence
check at all; remove it rather than writing a share of 1.
