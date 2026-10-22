# Reject a Computed Age Described as Collected

**Goal:** build the demographics domain with each subject's age (`AGE`), and
the metadata a data definition describes it by.

**Input:** collected demographics with the year of birth (`BRTHYR`) and the
year of the reference start date (`RFYEAR`), one record per subject.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `AGE` would be the reference year minus the birth year.

**Note:** the age is computed by subtracting one year from another, but it
is described as collected by the investigator. A value computed here was not
collected as it stands, so the description contradicts how the value is
made, and the run is rejected before any data is read; no artifact is
accepted.

## How to fix

Describe the age as derived by the sponsor, with the method a reviewer needs:

```yaml
submission:
  core: Exp
  role: Record Qualifier
  length: 3
  origins:
    - type: Derived
      source: Sponsor
  method: Year of the reference start date minus the year of birth
```

If the case report form collected the age itself, read that collected value
instead of computing it, and keep the collected origin.
