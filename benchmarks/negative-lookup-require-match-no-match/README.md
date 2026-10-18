# Reject a Reference Limit That Is Both Required and Optional

**Goal:** attach the reference upper limit (`LBSTNRHI`) to each collected
result by test and sex.

**Input:** collected results carrying test code (`LBTESTCD`) and sex
(`SEX`), plus a reference table carrying an upper limit (`NRHI`) per test
code and sex.

**Variables:**

- `LBSTNRHI` would be the upper limit read from the reference table matched
  on test code and sex.

**Note:** every result is required to find its reference entry, and a result
without one is also given an answer. The two contradict each other, so the
run is rejected before any record is read, with no artifact accepted.

## How to fix

Decide whether a result without a reference entry is an error or has an
answer. If every result must be checked against a range, keep only
`require_match: true`:

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
    require_match: true
```

If a missing limit is acceptable, keep only the answer, or omit both: an
intermediate without `no_match` answers with a missing limit, and the run
counts each such result. When only some results must find a limit, keep
`require_match: true` and read the limit under a condition that selects those
results; a result that never reads the limit never fails:

```yaml
derivation:
  case:
    - when: "LBTESTCD = 'ALT'"
      then: REFRANGE.NRHI
```
