# ADaM ADSL: reject lowercasing a numeric site code

This example uses collected demographics to record one row per subject:

- `SITEID` is the site number, counting sites, not naming them.
- `SITEFOLD` folds the site code to lowercase for grouping.

Lowercasing reads letters, and a number has none: no implementation may
apply the operation to a numeric value, so the specification is rejected
before any data is read and no artifact is accepted.

## How to fix

Decide what the study groups by, then state it in text. When the site
code is collected as a number, carry it as a number and group by that:

```yaml
- name: SITEID
  type: int
  derivation:
    source: DM.SITENO
```

When grouping needs text, collect the code as text so the case
operation has letters to read.
