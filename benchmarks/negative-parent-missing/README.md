# Reject Shared Definitions That Are Not There

**Goal:** carry each subject's age (`AGE`) onto the subject-level analysis
dataset, reusing the study's shared identifier columns.

**Input:** collected demographics, one record per subject, and the study's
shared identifier columns, expected in `layers/study.yaml`.

**Variables:**

- `AGE` would be the age in years as collected.

**Note:** the shared identifier columns are expected in `layers/study.yaml`,
but that file was never copied into this study's folder. Without it the
dataset has no study or subject identifiers, so the run is rejected before
any data is read and no artifact is accepted.

## How to fix

Copy the study's shared file into place, so that `layers/study.yaml` exists
beside the specification, or point `parent` at the file where it is kept.
Do not remove `parent` to make the run pass: the identifier columns would
then be missing from the dataset.
