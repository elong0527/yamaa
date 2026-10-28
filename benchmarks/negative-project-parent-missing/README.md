# Reject a Package Whose Study Project Is Not There

**Goal:** carry the planned treatment (`TRT01P`) into ADSL for one
submission package, coded with the study's planned treatments.

**Input:** one pre-analysis row per subject, and the package's project file,
which inherits the study's coded values from the study's project, expected
in `study/project.yaml`.

**Variables:**

- `TRT01P` would be the planned treatment, one of the study's coded values.

**Note:** the study's project was never copied into this package's folder.
Without it the package has none of the study's coded values, so the run is
rejected before any data is read and no artifact is accepted.

## How to fix

Copy the study's project into place, so that `study/project.yaml` exists
beside the package's project file, or point `parent` at the file where the
study keeps it:

```yaml
parent: ../study/project.yaml
```

Do not remove `parent` to make the run pass: the package would then lose
the study's coded values.
