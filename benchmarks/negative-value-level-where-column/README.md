# Reject a Value Description Selected by a Variable the Dataset Lacks

**Goal:** carry the vital-sign result (`AVAL`) for each subject and
parameter, with a description of the systolic pressure values for the
data-definition document.

**Input:** standardized vital signs, one record per subject and test, with
the numeric result.

**Variables:**

- `PARAMCD` is the vital-sign test code.
- `AVAL` would be the numeric result.

**Note:** a value description selects its records by variables of the
dataset it describes, since a reviewer evaluates its condition on that
dataset. Here the systolic description names the collected test code
`VSTESTCD`, which the analysis dataset does not carry, so the run is refused
before any data is read.

## How to fix

Select the records by the analysis dataset's own variable, the parameter code
that carries the test code:

```yaml
value_level:
  - id: SYSBP
    where: {PARAMCD: [SYSBP]}
    label: Systolic Blood Pressure (mmHg)
```

A where column must be a declared text column the output carries, other than
the described variable itself.
