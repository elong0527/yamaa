# Reject a Country Submitted Without a Length

**Goal:** build the demographics domain with each subject's country
(`COUNTRY`), and the metadata a data definition describes it by.

**Input:** collected demographics, one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `COUNTRY` would be the country of the subject's site, as a three-letter
  code.

**Note:** the country is submitted as text, and text is described with its
longest length. Neither a declared length nor a checked limit states it, and
the data definition requests no transport file that would measure it from the
published values, so the data definition would lack the length. The run
builds the dataset, and the data definition is refused; no document is
written.

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

A `max_length` check of 3 on the column would state the same length. A data
definition that requests a transport file, such as `transport: dataset_json`,
takes the longest published value instead.
