# Reject Two Versions of One Implementation Guide in One Data Definition

**Goal:** build the demographics dataset, one record per subject, and submit
it with its data-definition document.

**Input:** the study's subjects, one row each.

**Variables:**

- `USUBJID` identifies each subject across the study.

**Note:** a dataset names the implementation guide it follows by the guide's
name alone, so one data definition cites each guide once. The study cites
two versions of the general tabulation guide, so the submission stops before
any file is written; the dataset itself still builds.

## How to fix

Cite the one version of the guide the study follows:

```yaml
sdtm:
  standards:
    - {name: SDTMIG, version: "3.4", status: Final}
```

Datasets prepared under an earlier version are brought to that version
before the study submits them together.
