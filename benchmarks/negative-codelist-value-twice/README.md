# Reject a Codelist That Lists a Value Twice

**Goal:** build the demographics domain with each subject's sex (`SEX`),
restricted to the values of the study's sex codelist.

**Input:** collected demographics, one record per subject, and the study's
codelists in the project file.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `SEX` would be the sex as collected, one of the codelist's values.

**Note:** the sex codelist lists `F` twice. A codelist is a set of distinct
values, each with one place in the list, so a repeated value makes it
unclear which entry a reader should take, and the run is rejected before any
data is read; no artifact is accepted.

## How to fix

List each value once:

```yaml
- id: SEX
  name: Sex
  items:
    - value: F
    - value: M
```
