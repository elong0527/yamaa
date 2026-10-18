# Reject Two Value Descriptions That Can Describe One Record

**Goal:** carry the vital-sign result (`AVAL`) for each subject and
parameter, with a description of the blood-pressure values for the
data-definition document.

**Input:** standardized vital signs, one record per subject and test, with
the numeric result.

**Variables:**

- `PARAMCD` is the vital-sign test code.
- `AVAL` would be the numeric result.

**Note:** each record may be described by at most one of a variable's value
descriptions. Here one description covers both blood pressures and another
covers diastolic pressure, so every diastolic record would have two
descriptions, and the run is refused before any data is read.

## How to fix

Decide which description a diastolic pressure should carry, and list each
parameter in exactly one entry:

```yaml
value_level:
  - id: SYSBP
    where: {PARAMCD: [SYSBP]}
    label: Systolic Blood Pressure (mmHg)
  - id: DIABP
    where: {PARAMCD: [DIABP]}
    label: Diastolic Blood Pressure (mmHg)
```

When the two blood pressures truly share their description, keep the single
entry listing both and drop the diastolic one.
