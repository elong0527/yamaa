# Reject One Summary Over Exposure and Vital Signs Records Together

**Goal:** give each subject's total dose per kilogram of body weight
(`DOSEKG`) on the subject-level analysis dataset.

**Input:** subjects from the collected demographics, collected exposure
records with the dose given (`EXDOSE`), and collected vital signs with the
body weight (`WEIGHT`), several records per subject in each.

**Variables:**

- `DOSEKG` would be the subject's total dose divided by the heaviest weight
  recorded for the subject.

**Note:** the total dose and the heaviest weight are summarized in one
expression that reads the exposure records and the vital signs records
together. Two sets of records have no single set of rows to summarize, so
the run is rejected before any data is read and no artifact is accepted.

## How to fix

Summarize each set of records into a column of its own, then combine the two
columns:

```yaml
- name: TOTDOSE
  type: float
  label: Total Dose
  derivation:
    aggregate: "SUM(EX.EXDOSE)"
- name: MAXWT
  type: float
  label: Maximum Weight (kg)
  derivation:
    aggregate: "MAX(VS.WEIGHT)"
- name: DOSEKG
  type: float
  label: Total Dose per kg Body Weight
  derivation: "TOTDOSE / MAXWT"
```

Keep the weight the statistical analysis plan names: the heaviest weight
here, a baseline weight in many studies.
