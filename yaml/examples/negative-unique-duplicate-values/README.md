# ADaM ADSL: reject a site shared by two subjects

This example uses collected demographics with a `yamaa` specification to
derive one row per subject:

- `SITEID` is the site the subject enrolled at. Two subjects enrolled at
  the same site share the value, so it cannot stand alone as the row
  identity.

## How to fix

Assert uniqueness on the columns that truly identify a row. Either check
the subject identifiers:

```yaml
verifications:
  - unique:
      columns: [STUDYID, USUBJID]
```

or, when one row per site is the intent, aggregate the subjects to site
rows before asserting.
