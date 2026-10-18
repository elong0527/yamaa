# Reject Symlink Path

**Goal:** build analysis records for laboratory tests carrying
`SEX`, `AVAL`, and `ANRHI`, with the upper limit chosen for each
test and sex.

**Input:** collected laboratory results carrying the collected
result (`LBSTRESN`), test code (`LBTESTCD`), and sex (`SEX`),
plus a reference table of upper limits by test code and sex.

**Variables:**

- `SEX` would hold the sex the limit is chosen by, copied from
  the collected record.
- `AVAL` would hold the collected result, copied from `LBSTRESN`.
- `ANRHI` would hold the upper limit of normal for that test and
  sex, taken from the reference table for the matching test code
  and `SEX`; missing when no row matches or the sex is missing.

**Note:** the name the study reads for the reference table,
`input/lbref.csv`, is a symbolic link: a stand-in that points at
another file. Where it points can be changed without changing the
study, so the limits that were reviewed and the limits that are
read need not be the same. The run is rejected before any data is
read and no artifact is accepted.

## How to fix

Read the limits under the name that holds them:

```yaml
input:
  LBREF: input/reference/lbref.csv
```

Storing the file itself where the study reads it works equally well. Either
way one name reaches one file, and reviewing the study is reviewing what it
reads.
