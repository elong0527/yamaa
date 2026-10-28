# Reject a Parameter Code the Study's Terminology Does Not List

**Goal:** derive the parameter name (`PARAM`) of each laboratory result
from its parameter code (`PARAMCD`), as the study's parameter terminology
names the code.

**Input:** collected laboratory results with the study and subject
identifiers and the test code, which is the parameter code.

**Variables:**

- `PARAM` would be the name the study's parameter terminology gives the
  code, such as `Alanine Aminotransferase (U/L)` for `ALT`.

**Note:** a result carries a test code the terminology does not list, so
it has no name, and nothing states what such a result's name should be.
The run stops when it reaches that result, and no artifact is accepted.

## How to fix

Decide whether the test belongs to the study. When it does, add its code
and name to the project file's codelist:

```yaml
- {value: GGT, decode: Gamma Glutamyl Transferase (U/L)}
```

When an unlisted test is expected and should carry a fixed name instead,
declare that answer locally with `unmapped: Other`.
