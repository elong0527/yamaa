# Reject a Value Stated as Both a Total and a Single Dose

**Goal:** keep one record per study drug administration, with the dose in
`AVAL`.

**Input:** collected exposure records, one per administration, with the
dose given (`EXDOSE`).

**Variables:**

- `EXSEQ` is the collected sequence number of the administration.
- `AVAL` would be the analysis value of the record.

**Note:** the value of `AVAL` is stated twice: once as each subject's total
dose over all administrations, and once, for each administration record, as
the dose given on that record. A total is computed from all of a subject's
records, so it cannot be replaced record by record, and the run is rejected
before any data is read; no artifact is accepted.

## How to fix

Decide what `AVAL` holds on an administration record. When it is the dose
given, state only that, in the column itself:

```yaml
- name: AVAL
  type: float
  label: Analysis Value
  derivation: EX.EXDOSE
```

When the subject's total dose is needed as well, give it a column of its own,
such as a cumulative dose, rather than replacing it record by record.
