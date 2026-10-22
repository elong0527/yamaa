# Reject a Country Submitted Without a Length

**Goal:** build the demographics domain with each subject's country
(`COUNTRY`), and the metadata a data definition describes it by.

**Input:** collected demographics, one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `COUNTRY` would be the country of the subject's site, as a three-letter
  code.

**Note:** the country is submitted as text, and text is described with its
longest length. Neither a declared length nor a checked limit states it, so
the data definition would lack the length, and the run is rejected before
any data is read; no artifact is accepted.

## How to fix

State the length of the longest country code the study submits:

```yaml
submission:
  core: Req
  role: Record Qualifier
  length: 3
  origins:
    - type: Collected
      source: Investigator
```

A `max_length` check of 3 on the column would state the same length.
