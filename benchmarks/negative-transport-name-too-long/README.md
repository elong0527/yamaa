# Ethnicity Name Too Long for Transport

**Goal:** submit DM with the collected ethnicity in a SAS transport file.

**Input:** one row per subject carrying the site, age, sex and ethnicity.

**Variables:**

- `SITEID`, `AGE` and `SEX` are as collected.
- `ETHNICITY` is the collected ethnicity.

**Note:** a SAS transport variable name has at most eight characters, so the
submission stops before any file is written; no name is shortened.

## How to fix

Use the variable name the implementation guide publishes. SDTM names the
collected ethnicity `ETHNIC`:

```yaml
  - name: ETHNIC
    type: str
    label: Ethnicity
    derivation: DM_RAW.ETHNICITY
```
