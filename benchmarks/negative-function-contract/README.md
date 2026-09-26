# Reject Contract Mismatch

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-function-contract.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the source identifier through as `ID` and return
the project routine's result as `RESULT`.

**Input:** a source file with `ID` and `VALUE`, where `VALUE`
holds the numeric value passed to the routine.

**Variables:**

- `ID` is the source identifier, carried through unchanged.
- `RESULT` would be the numeric result of passing the source
  `VALUE` to the `project_value` routine under contract `2.0.0`.

**Note:** the project provides `project_value` only under
contract `1.0.0`, so the requested contract `2.0.0` is
unavailable. The run is rejected before any data is read, and
no artifact is accepted.

**Standard:** TEST | **Domain:** TEST

## How to fix

Request the exact logical contract the project provides:

```yaml
function:
  name: project_value
  contract_version: "1.0.0"
  args: {x: SOURCE.VALUE}
```
