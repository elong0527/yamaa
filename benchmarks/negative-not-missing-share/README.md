# Reject Too Many Missing Baseline Weights

**Goal:** record each subject's baseline weight (`WEIGHTBL`) from the
collected demographics, allowing at most a quarter of the subjects to lack
one.

**Input:** collected demographics carrying each subject's weight.

**Variables:**

- `WEIGHTBL` would be the subject's weight in kilograms at baseline, copied
  from the collected records, and missing where none was collected.

**Note:** when the share of subjects without a baseline weight exceeds the
accepted limit, no artifact is accepted. The failure names every affected
subject and how many subjects were checked, so the share is known exactly.

## How to fix

Decide whether the analysis can proceed with that many missing weights.
When the weights were collected, correct them at the governed source and
rerun. When the study accepts a larger share of missing weights, state the
share it accepts:

```yaml
columns:
  - name: WEIGHTBL
    verifications:
      - not_missing: {max_fraction: 0.5}
```

To keep the limit and still build the dataset, report the excess instead of
failing, which needs a run report file:

```yaml
output:
  report: adsl-checks.csv
columns:
  - name: WEIGHTBL
    verifications:
      - not_missing: {max_fraction: 0.25, severity: warning}
```

Do not fill the gaps with a placeholder weight, which reports a value nobody
collected.
