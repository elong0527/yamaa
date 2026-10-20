# Reject Two Releases of SDTM Terminology in One Data Definition

**Goal:** submit DM with its data-definition document, coding the age unit
(`AGEU`) and sex (`SEX`) with SDTM controlled terminology.

**Input:** one row per subject carrying the age and the recorded sex code.

**Variables:**

- `AGE` and `SEX` are as collected; `SEX` is one of the codes the sex
  terminology lists: `F`, `M` or `U`.
- `AGEU` is `YEARS` for every subject, since age is collected in whole
  years.

**Note:** the data-definition document identifies the SDTM terminology it
cites by that name alone, without its release, so one document cites one
release of it. The sex codes are taken from the December 2023 release and
the age unit from the March 2024 release, so the submission stops before any
file is written.

## How to fix

Choose the one release of SDTM terminology the study follows, check each
list's codes against it, and declare that release once for both lists:

```yaml
codelists:
- standard:
    name: CDISC/NCI
    publishing_set: SDTM
    version: '2024-03-29'
  codelists:
  - id: SEX
    name: Sex
    alias: C66731
    items:
    - {value: F, decode: Female, alias: C16576}
    - {value: M, decode: Male, alias: C20197}
    - {value: U, decode: Unknown, alias: C17998}
  - id: AGEU
    name: Age Unit
    alias: C66781
    items:
    - {value: YEARS, alias: C29848}
```

Lists may stay in separate sources when every source names the same
release.
