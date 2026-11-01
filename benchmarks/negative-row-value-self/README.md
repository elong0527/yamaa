# Reject Future-Filled Weight

**Goal:** fill a missing weight from the most recent earlier
weight, carrying `ADT`, `AVAL`, and `AVALF` for each measurement.

**Input:** collected weight records carrying the collection date
(`VSDTC`), the test code (`VSTESTCD`), and the collected result
(`VSSTRESN`).

**Variables:**

- `ADT` would be the analysis date taken from the collection
  date.
- `AVAL` would be the collected weight taken from the collected
  result, and missing when no measurement was taken.
- `AVALF` would be the current weight or, when it is missing, the
  filled weight of a later measurement in date order. That later
  measurement's filled value is itself being calculated.

**Note:** the requested fill depends on a future calculated value rather
than an earlier completed result. The run is rejected before any data is
read and no artifact is accepted.

## How to fix

Carry from the collected series rather than from the filled output. Search the
earlier collected values, then take the first available of `AVAL` and
`PRIOR`:

```yaml
- name: PRIOR
  type: float
  derivation:
    locf:
      source: AVAL
      include_current: false
      window:
        group_by: [STUDYID, USUBJID, PARAMCD]
        order_by: [ADT, VSSEQ]

- name: AVALF
  type: float
  derivation:
    first_available:
      sources: [AVAL, PRIOR]
```

Keep `PRIOR` internal by omitting it from `output.columns`.
