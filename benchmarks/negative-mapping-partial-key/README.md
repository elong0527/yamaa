# Reject Partial Key

**Goal:** carry sex and the collected result into the output and
attach the upper limit of normal (`ANRHI`) chosen by test and sex.

**Input:** laboratory records carrying test code (`LBTESTCD`), sex
(`SEX`), and collected result (`LBSTRESN`), plus a limit table
carrying test code, sex, and upper limit (`ANRHI`).

**Variables:**

- `ANRHI` would be the upper limit of normal taken from the limit
  table where the test matches the collected test and the sex
  matches the carried sex.

**Note:** when the carried sex is blank, the pair that chooses a
limit is incomplete and no entry can be looked for, so the run is
rejected with no artifact accepted. Falling back to either sex, or
to a combined limit, would answer with a range the study never
stated.

## How to fix

Recover and correct the missing sex when possible. If the intended result is a
missing reference limit whenever any lookup input is missing, drop
`require_match: true` so the lookup answers with missing:

```yaml
intermediates:
  - id: LBREF_LOOKUP
    dataset: LBREF
    key: {LBTESTCD: PARAMCD, SEX: SEX}
```

That one answer also covers a complete test and sex that the reference table
does not list: both find no entry (LOOKUP-24), and the run counts each under
`absent`.
