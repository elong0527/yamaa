# Refuse to Submit Demographics Last Saved Under an Earlier Name

**Goal:** submit the demographics domain with its data definition, from the
files its last build saved.

**Input:** collected demographics, one record per subject, and what the last
build saved: the dataset under its earlier name, `dm_v1.csv`, and the run
report beside it, `dm-report.csv`.

**Variables:**

- `DOMAIN` is `DM` on every record.
- `STUDYID` and `USUBJID` are copied from the collected record.

**Note:** the dataset is now saved as `dm.csv`, but it has not been built
since it was renamed. Its run report still describes the last build: each
row names the dataset that build saved (`ARTIFACT`), the kind of item it
describes (`KIND`), where the specification states that item (`SPEC_PATH`)
and its name (`NAME`), with the build's outcome (`OUTCOME`), and with the
collected records read, the subjects built and each column's missing values
and longest text (`COUNT`, `TOTAL`, `DETAILS`); no check ran, so its label,
column, rule, severity, failure and failing subjects (`ID`, `TARGET`,
`RULE`, `SEVERITY`, `CONDITION`, `KEYS`) stay empty. The report records
`dm_v1.csv`, not `dm.csv`, so the submission is refused before any document
is written: its data definition would describe a dataset that no build of
the current specification saved.

## How to fix

Build and save the dataset again, so that `dm.csv` and its run report
describe one build, then submit:

```python
p = yamaa.project("project.yaml")
p.spec("DM").run().save()
p.submission("sdtm")
```

Do not edit the run report or rename the saved file to match it: the report
records what a build read and wrote, and only a new build replaces it.
`spec_saved.yaml` keeps the specification as it stood at that last build.
