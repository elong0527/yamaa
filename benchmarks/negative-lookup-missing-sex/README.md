# Reject Missing Sex

**Goal:** look up the sex-specific reference upper limit
(`LBSTNRHI`) for each collected laboratory result by its test
code and sex.

**Input:** collected results carrying the test code (`LBTESTCD`),
sex (`SEX`), and numeric result (`LBSTRESN`), plus a limits table
carrying the upper limit (`NRHI`) by test code and sex.

**Variables:**

- `LBSTNRHI` would be the upper limit from the limits table for
  the matching test code and sex.

**Note:** a result with a blank sex has an incomplete lookup key
and cannot find its sex-specific limit. Every result is required to
find one, so such a result rejects the run with no artifact
accepted.

## How to fix

Recover the missing sex when possible. If an incomplete lookup key is
intended to leave the upper limit missing, drop `require_match: true`; a
result with a blank sex then gets a missing `LBSTNRHI`, counted under
`no_match`:

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
```

An incomplete key and a complete key the table does not contain share one
absence policy (LOOKUP-13): both yield nothing, and `require_match` or
`no_match` decides whether that fails or answers with a value.
