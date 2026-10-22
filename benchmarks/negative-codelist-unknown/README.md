# Reject a Binding to a Codelist the Project Does Not Declare

**Goal:** build the demographics domain with each subject's sex (`SEX`),
restricted to the values of the study's sex codelist.

**Input:** collected demographics, one record per subject, and the study's
codelists in the project file.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `SEX` would be the sex as collected, one of the codelist's values.

**Note:** the sex is bound to a codelist named `SEXCD`, the name another
study gave its list. This project declares its sex list as `SEX`, and no
list as `SEXCD`, so nothing says which values the sex may hold, and the run
is rejected before any data is read; no artifact is accepted.

## How to fix

Bind the column to the codelist this project declares:

```yaml
submission:
  core: Req
  role: Record Qualifier
  length: 1
  codelist: SEX
  origins:
    - type: Collected
      source: Investigator
```
