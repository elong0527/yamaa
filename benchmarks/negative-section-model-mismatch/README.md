# Reject an Analysis Dataset Listed With the Tabulation Datasets

**Goal:** carry the age and sex (`AGE`, `SEX`) of each subject onto the
subject-level analysis dataset, and submit it.

**Input:** collected demographics, one record per subject.

**Variables:**

- `AGE` is the age in years as collected.
- `SEX` is the sex as collected.

**Note:** tabulation and analysis datasets are submitted with separate data
definitions, each describing the datasets of one CDISC model. The study lists
this analysis dataset with its tabulation datasets, so composing their data
definition is rejected before any document is written; the dataset itself
still builds.

## How to fix

List the dataset with the analysis datasets, which the analysis
implementation guide describes:

```yaml
adam:
  standards:
    - {name: ADaMIG, version: "1.3", status: Final}
  specs:
    - spec.yaml
  define: define.xml
```

If the dataset is meant to be a tabulation domain instead, declare
`model: SDTM` and give it the tabulation metadata that model requires.
