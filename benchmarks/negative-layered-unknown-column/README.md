# Reject a Change From Baseline That Reads an Undeclared Value

**Goal:** derive each visit's change from baseline (`CHG`) for the vital
signs analysis dataset, which reuses the study's shared identifier columns.

**Input:** pre-analysis vital signs, one record per subject, parameter and
visit, carrying the analysis value (`AVAL`) and the baseline value (`BASE`);
and `layers/study.yaml`, the study's shared identifier columns.

**Variables:**

- `AVAL` would be the analysis value as collected.
- `BASE` would be the subject's baseline value for the parameter.
- `CHG` would be `AVAL` minus the baseline value; missing when either is
  missing.

**Note:** the change from baseline subtracts a value named `BASEVAL`, which
neither file declares: the baseline is held in `BASE`. No value can be read
for the subtraction, so the run is rejected before any data is read and no
artifact is accepted.

## How to fix

Subtract the baseline column the dataset declares:

```yaml
- name: CHG
  type: float
  label: Change from Baseline
  derivation: "AVAL - BASE"
```
