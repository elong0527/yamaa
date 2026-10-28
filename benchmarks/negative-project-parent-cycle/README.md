# Reject a Study Project That Inherits From Its Own Package

**Goal:** carry the planned treatment (`TRT01P`) into ADSL for one
submission package, coded with the study's planned treatments.

**Input:** one pre-analysis row per subject, the package's project file,
which inherits from the study's project in `study/project.yaml`, and that
project, which names the package's project as its own parent.

**Variables:**

- `TRT01P` would be the planned treatment, one of the study's coded values.

**Note:** the two project files inherit from each other, so neither holds
a settled set of study metadata, and the run is rejected before any data is
read.

## How to fix

Remove `parent` from the study's project: the study's project is the root
that every package inherits from, and names no parent of its own.

```yaml
yamaa: "1.0"
type: project
codelists:
- codelists: [...]
```
