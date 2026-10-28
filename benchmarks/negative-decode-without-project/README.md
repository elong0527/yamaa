# Reject Parameter Names Read Without the Study's Terminology

**Goal:** derive the parameter name (`PARAM`) of each laboratory result
from its parameter code (`PARAMCD`), as the study's parameter terminology
names the code.

**Input:** collected laboratory results with the study and subject
identifiers and the test code, which is the parameter code.

**Variables:**

- `PARAM` would be the name the study's parameter terminology gives the
  code.

**Note:** the study declares its parameter terminology once, beside its
list of datasets. Built on its own, the dataset has no terminology to
read the names from, so the run is refused before any record is read.

## How to fix

Build the dataset as part of its study: open it from the study's project
file, which declares the parameter terminology and lists the dataset, for
example `yamaa.project("project.yaml").spec("ADLB").run()` in Python:

```yaml
yamaa: "1.0"
type: project
codelists:
  - codelists:
      - id: PARAMCD
        name: Parameter Code
        items:
          - {value: ALT, decode: Alanine Aminotransferase (U/L)}
          - {value: AST, decode: Aspartate Aminotransferase (U/L)}
adam:
  specs: [spec.yaml]
```
