# Reject an unavailable project-routine contract

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-function-contract-mismatch.html)

**Goal:** carry the source identifier through as `ID` and return
the project routine result as `RESULT`.

**Input:** a source file with `ID` and `VALUE`, where `VALUE`
holds the numeric value passed to the routine.

**Variables:**

- `RESULT` would be the numeric result of passing the source
  `VALUE` to the `project_value` routine under contract `2.0.0`.

The requested contract `2.0.0` does not match the provided
contract `1.0.0`, so the run is rejected before any data is read
and no artifact is accepted.

**Standard:** TEST | **Domain:** TEST

## How to fix

Request the exact logical contract the selected project provides:

```yaml
function:
  name: project_value
  contract_version: "1.0.0"
  args: {x: SOURCE.VALUE}
```
