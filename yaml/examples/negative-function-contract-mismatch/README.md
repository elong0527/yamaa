# Project function: reject an unavailable project-routine contract

The source contains one numeric value, but the requested project routine
version is not the version the selected project provides. Execution stops
before the value is processed.

## How to fix

Request the exact logical contract the selected project provides:

```yaml
function:
  name: project_value
  contract_version: "1.0.0"
  args: {x: SOURCE.VALUE}
```
