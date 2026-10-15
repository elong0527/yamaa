# Reject Nested Expression

**Goal:** carry `COUNTRY` on one record per subject, holding the
subject's collected country in capitals, or the site country when
the subject's own entry is blank.

**Input:** collected demographics records carrying the collected
country (`COUNTRY`) and the site country (`SITECNTY`).

**Variables:**

- `COUNTRY` would be the subject's own country in capitals, or the
  site country in capitals when the subject's own entry is blank.

**Note:** the uppercasing step is also asked to choose between the
two entries, but it can only read one named value. Choosing the
entry is a step of its own whose result must be named first, so the
run is rejected before any data is read and no artifact is
accepted.

## How to fix

Derive the selected source in a helper column, then pass that value to the
uppercasing step:

```yaml
- name: COUNTRYSRC
  type: str
  derivation:
    first_available:
      sources: [DM.COUNTRY, DM.SITECNTY]

- name: COUNTRY
  type: str
  derivation:
    str_case:
      source: COUNTRYSRC
      to: upper
```

Keep the helper column out of the artifact by leaving it off the artifact
column list. A subject with both entries blank then stops the run; if that can
happen, add `missing: null` beside `source: COUNTRYSRC` to leave that
subject's country blank instead.
