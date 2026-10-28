# Reject a Parameter Name Read From an Undeclared Terminology

**Goal:** derive the parameter name (`PARAM`) of each laboratory result
from its parameter code (`PARAMCD`), as the study's parameter terminology
names the code.

**Input:** collected laboratory results with the study and subject
identifiers and the test code, which is the parameter code.

**Variables:**

- `PARAM` would be the name the study's parameter terminology gives the
  code, such as `Alanine Aminotransferase (U/L)` for `ALT`.

**Note:** the request names a terminology the study does not declare. The
study's parameter terminology lists each code beside its name, under the
code list's own name, so the request is rejected before any record is
read and no artifact is accepted.

## How to fix

Read the names from the codelist the project file declares, whose items
pair each code with its name:

```yaml
derivation:
  decode:
    source: PARAMCD
    codelist: PARAMCD
```

Declare a separate codelist only when the study's terminology truly lists
the names somewhere else.
