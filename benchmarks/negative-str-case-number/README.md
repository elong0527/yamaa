# Reject Numeric Lowercase

**Goal:** derive the site name (`SITE`) for analysis use, in lower case.

**Input:** collected demographics with study and subject identifiers
and the numeric site number (`SITENUM`).

**Variables:**

- `SITE` would be the site name in lower case, but it is taken from the
  site number (`SITENUM`), and a number has no lower-case form.

The request is rejected before any data is read, and no artifact is
accepted.

## How to fix

Fold the collected site name rather than its number, once `SITENM` is
present in the demographics extract:

```yaml
str_case:
  source: DM.SITENM
  to: lower
```
