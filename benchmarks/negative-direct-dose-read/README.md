# Reject Uncomputed Dose

**Goal:** double the administered exposure dose in `DOSEDBL` for
each subject.

**Input:** a subject-treatment inventory carrying `EXTRT` with
matching exposure records carrying the exposure sequence number
and `EXDOSE`.

**Variables:**

- `DOSEDBL` would contain twice the exposure dose (`EXDOSE * 2`)
  for the subject. A subject can have several exposure records, so
  a formula naming `EX.EXDOSE` does not say which record it means,
  and choosing one without a stated rule, or totalling them, would
  each give a different result. A formula computes only from values
  the output row already carries, and a value taken from another
  source is bound to one of those first, so the run is rejected
  before any data is read and no artifact is accepted.

## How to fix

Choose the exposure record explicitly, bind its dose to an output column,
and then compute from the unqualified column. For example, to use the
earliest administration:

```yaml
- name: DOSE
  type: float
  derivation:
    source:
      variable: EX.EXDOSE
      order_by: [EX.EXSEQ]
      keep: first

- name: DOSEDBL
  type: float
  derivation: "DOSE * 2"
```

Keep `DOSE` internal by omitting it from `output.columns`.

If the intended value is cumulative dose instead, use a qualified
`aggregate` rather than selecting one record.
