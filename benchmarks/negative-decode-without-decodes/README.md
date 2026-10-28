# Reject a Parameter Name From a Terminology That Lists No Names

**Goal:** derive the parameter name (`PARAM`) of each laboratory result
from its parameter code (`PARAMCD`), as the study's parameter terminology
names the code.

**Input:** collected laboratory results with the study and subject
identifiers and the test code, which is the parameter code.

**Variables:**

- `PARAM` would be the name the study's parameter terminology gives the
  code.

**Note:** the study's parameter terminology lists the codes alone, with no
name beside any of them, so no code has a name to read. The request is
rejected before any record is read and no artifact is accepted.

## How to fix

Give every code of the terminology the name it stands for, in the project
file's codelist:

```yaml
- id: PARAMCD
  name: Parameter Code
  items:
    - {value: ALT, decode: Alanine Aminotransferase (U/L)}
    - {value: AST, decode: Aspartate Aminotransferase (U/L)}
```

A terminology whose codes are already the words a reader needs has no
names to add; carry the code itself instead.
