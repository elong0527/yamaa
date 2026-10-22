# Reject a Submitted Variable That States No Origin

**Goal:** build the demographics domain with each subject's sex (`SEX`), and
the metadata a data definition describes it by.

**Input:** collected demographics, one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `SEX` would be the sex as collected.

**Note:** the sex carries submission metadata, but its list of origins is
empty. A data definition states where every submitted value came from, and
an empty list states nothing, so the run is rejected before any data is read
and no artifact is accepted.

## How to fix

State where the value came from. The sex is collected by the investigator on
the case report form:

```yaml
origins:
  - type: Collected
    source: Investigator
```
