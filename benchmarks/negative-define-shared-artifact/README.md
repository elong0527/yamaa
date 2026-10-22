# Refuse a Data Definition That Describes One File as Two Datasets

**Goal:** submit the demographics domain with its data definition.

**Input:** collected demographics, one record per subject.
`spec_dmv2.yaml` is a renamed copy of the demographics specification, kept
for review.

**Variables:**

- `DOMAIN` is `DM` on every record.

**Note:** the renamed copy was listed in the section beside the original,
and both write `dm.csv`. A data definition would then describe the one file
as two datasets, which a reviewer could not tell apart, so the submission is
refused before any document is written; the dataset itself still builds.

## How to fix

List only the specification whose dataset is submitted:

```yaml
sdtm:
  specs:
    - spec_dm.yaml
```

Keep the copy under review out of the section until it replaces the
original.
