# ADaM ADEX: reject a dose intensity measured against a per-record plan

This example uses a subject-treatment inventory with its component exposure
records to attempt one record per subject and treatment:

- `EXDOSU` is the dose unit the component is administered in;
- `DOSECUM` is the total dose administered across the component's records;
- `RDI` is meant to be that total as a percentage of the dose planned for the
  component.

The planned dose is recorded on each administration record rather than once
for the treatment, so a total taken across those records has no single planned
dose to measure against. Which record supplied it would decide the answer, so
the rule is rejected whether or not the recorded values happen to agree, and
no artifact is accepted.

## How to fix

First decide what the denominator means. If each exposure row carries its own
planned administration, total both actual and planned dose:

```yaml
derivation:
  aggregate:
    expr: >-
      100 * SUM(EX.EXDOSE) / NULLIF(SUM(EX.EXPLDOS), 0)
```

If the denominator is one treatment-level plan, store it once in the subject
treatment source, bind it to a numeric column, and compute `RDI` from that
column and `DOSECUM`. Do not select one exposure row arbitrarily.
