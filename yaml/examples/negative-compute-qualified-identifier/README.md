# ADaM ADEX: reject a doubled dose read straight from exposure

This example uses a subject-treatment inventory with its component exposure
records to attempt one record per subject:

- `DOSEDBL` is meant to be twice the administered dose.

A subject has several exposure records, so a formula naming the exposure dose
does not say which record it means. Silently choosing one, or totalling them,
would each give a different result from the same specification, so the run must
fail and no artifact is accepted. A formula computes from values the record
already carries, and a value taken from another source becomes one of those
first.

## How to fix

Choose the exposure record explicitly, bind its dose to an output column, and
then compute from the unqualified column. For example, to use the earliest
administration:

```yaml
- name: DOSE
  type: float
  output: false
  derivation:
    source:
      variable: EX.EXDOSE
      multiple_matches:
        order_by: [EX.EXSEQ]
        keep: first

- name: DOSEDBL
  type: float
  derivation:
    compute:
      expr: "DOSE * 2"
```

If the intended value is cumulative dose instead, use a qualified `aggregate`
rather than selecting one record.
