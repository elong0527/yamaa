# Reject an Empty Name for the Shared File

**Goal:** carry each subject's age (`AGE`) onto the subject-level analysis
dataset.

**Input:** collected demographics, one record per subject.

**Variables:**

- `AGE` would be the age in years as collected.

**Note:** the file was copied from a template that builds on a shared file,
and the name of that shared file was left empty. An empty name names no
file, and it is not the same as naming none, so the run is rejected before
any data is read and no artifact is accepted.

## How to fix

When the dataset builds on the study's shared file, name it:

```yaml
parent: layers/study.yaml
```

When it builds on no shared file, remove the `parent` line altogether.
