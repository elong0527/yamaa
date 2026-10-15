# Reject a Missing Required Argument

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-function-contract.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the source identifier through as `ID` and return
the project routine's result as `RESULT`.

**Input:** a source file with `ID` and `VALUE`, where `VALUE`
holds the numeric value passed to the routine.

**Variables:**

- `ID` is the source identifier, carried through unchanged.
- `RESULT` would be the numeric result of passing the source
  `VALUE` to the `project_value` routine.

**Note:** the call omits the numeric input required by the routine.
The run is rejected before any project code or data is read, and no
artifact is accepted.

**Standard:** TEST | **Domain:** TEST

## How to fix

Supply the required argument declared by the environment:

```yaml
function:
  name: project_value
  args: {x: SOURCE.VALUE}
```
