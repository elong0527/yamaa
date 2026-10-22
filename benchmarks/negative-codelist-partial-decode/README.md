# Reject a Codelist That Decodes Only Some of Its Values

**Goal:** build the demographics domain with each subject's sex (`SEX`),
restricted to the values of the study's sex codelist.

**Input:** collected demographics, one record per subject, and the study's
codelists in the project file.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `SEX` would be the sex as collected, one of the codelist's values.

**Note:** the sex codelist gives the text that `F` and `M` stand for, but
not the text for `U`, which was added later. A list whose values carry their
meaning and one whose values are their own meaning are different kinds of
list, and this one is both at once, so the run is rejected before any data
is read; no artifact is accepted.

## How to fix

Give every value its text:

```yaml
items:
  - value: F
    decode: Female
  - value: M
    decode: Male
  - value: U
    decode: Unknown
```
