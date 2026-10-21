# Refuse to Submit Demographics Last Saved Under an Earlier Name

**Goal:** submit the demographics domain with its data definition, from the
files its last build saved.

**Input:** collected demographics, one record per subject, and what the last
build saved: the dataset under its earlier name, `dm_v1.csv`, and the run
report beside it, `dm-report.csv`.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `STUDYID` is the collected study identifier. A record without it is refused.
- `USUBJID` is the collected unique subject identifier. A record without it is
  refused.

**Note:** the last build saved `dm_v1.csv`, but the current submission names
`dm.csv`. The dataset has not been rebuilt since that name changed. Its run
report still records the earlier filename, so the submission is refused
before any document is written: its data definition would describe a dataset
that no build of the current specification saved.

## How to fix

Build and save the dataset again, so that `dm.csv` and its run report
describe one build, then submit:

```python
p = yamaa.project("project.yaml")
p.spec("DM").run().save()
p.submission("sdtm")
```

The rebuilt report names its saved file (`ARTIFACT`), item kind and location
(`KIND`, `SPEC_PATH`, `NAME`), outcome (`OUTCOME`, `CONDITION`), counts
(`COUNT`, `TOTAL`) and measurements (`DETAILS`). Check labels and targets
(`ID`, `TARGET`), rules and severity (`RULE`, `SEVERITY`), and failing subject
keys (`KEYS`) describe any checks that ran.

Do not edit the run report or rename the saved file to match it: the report
records what a build read and wrote, and only a new build replaces it.
`spec_saved.yaml` keeps the specification as it stood at that last build.
