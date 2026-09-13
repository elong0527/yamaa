# Reject a doubled dose read straight from exposure

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-compute-qualified-identifier.html)

**Goal:** double the administered exposure dose in `DOSEDBL` for
each subject.

**Input:** a subject-treatment inventory carrying `EXTRT` with
matching exposure records carrying the exposure sequence number
and `EXDOSE`.

**Variables:**

- `DOSEDBL` would contain twice the exposure dose (`EXDOSE * 2`)
  for the subject, but no row is produced.

A subject can have several exposure records, so a formula naming
`EX.EXDOSE` does not say which record it means. Choosing one
without a stated rule, or totalling them, would each give a
different result from the same request, so the run is rejected
before any data is read and no artifact is accepted. A formula
computes from values the record already carries, and a value
taken from another source is bound to one of those first.

**Standard:** ADaM | **Domain:** ADEX

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
      multiple_matches:
        order_by: [EX.EXSEQ]
        keep: first

- name: DOSEDBL
  type: float
  derivation:
    compute:
      expr: "DOSE * 2"
```

Keep `DOSE` internal by omitting it from `output.columns`.

If the intended value is cumulative dose instead, use a qualified
`aggregate` rather than selecting one record.
