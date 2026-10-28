# Reject a Package That Names Two Parent Projects

**Goal:** carry the planned treatment (`TRT01P`) into ADSL for one
submission package, coded with the study's planned treatments.

**Input:** one pre-analysis row per subject, the package's project file, the
study's project in `study/project.yaml`, which holds the study's coded
values, and the program's in `program/project.yaml`, which holds the study's
description.

**Variables:**

- `TRT01P` would be the planned treatment, one of the study's coded values.

**Note:** the package names both projects as its parents. A project inherits
from one parent, so which project's metadata comes first would be unstated,
and the run is rejected before any data is read.

## How to fix

Chain the projects so that each names at most one parent: the package names
the study's project, and the study's project names the program's.

```yaml
# project.yaml
parent: study/project.yaml
# study/project.yaml
parent: ../program/project.yaml
```
