# Reject a weight carried from its own filled value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-row-value-self-reference.html)

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
- `AVALF` would be the current weight, or the earlier filled
  value from the most recent earlier measurement when the current
  weight is missing. When a gap follows a filled gap, the value
  would have to come from a record that was itself filled in, so
  the filled value is stated in terms of its own earlier value.

**Note:** filling one gap and stopping, or resolving the records
in an order nothing states, would each give a different answer
from the same data, so the run is rejected before any data is
read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADVS

## How to fix

Carry from the collected series rather than from the filled output. Search the
earlier collected values, then coalesce that result with the current value:

```yaml
- name: PRIOR
  type: float
  derivation:
    previous_non_missing:
      source: AVAL
      group_by: [STUDYID, USUBJID, PARAMCD]
      order_by: [ADT, VSSEQ]
- name: AVALF
  type: float
  derivation:
    coalesce:
      sources: [AVAL, PRIOR]
```

Keep `PRIOR` internal by omitting it from `output.columns`.
