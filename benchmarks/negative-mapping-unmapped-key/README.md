# Reject Unmapped Key

**Goal:** carry sex and the collected result into the output and
attach the upper limit of normal (`ANRHI`) chosen by test and sex.

**Input:** collected laboratory results carrying test code
(`LBTESTCD`), sex (`SEX`), and numeric result (`LBSTRESN`), plus a
reference table carrying test code, sex, and upper limit
(`ANRHI`).

**Variables:**

- `ANRHI` would be the upper limit of normal for that test and
  sex, read from the reference table where the collected test code
  and sex match a table entry. Every result is required to find an
  entry: leaving the limit missing would hide that the result was
  never checked against a range, so a collected test and sex with
  no table entry rejects the run with no artifact accepted.

## How to fix

Add the governed `AST/M` reference range to the reference table when one
exists. If the analysis intentionally leaves the limit missing when a complete
key is not in the table, drop `require_match: true` so the lookup answers with
missing:

```yaml
intermediates:
  - id: LBREF_LOOKUP
    dataset: LBREF
    key: {LBTESTCD: PARAMCD, SEX: SEX}
```

The same answer also covers a result whose test code or sex is blank: both
find no entry (LOOKUP-24), and the run counts each under `no_match`.
