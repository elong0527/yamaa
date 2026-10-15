# Reject a Missing Required Argument

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

## How to fix

Supply the required argument declared by the environment:

```yaml
function:
  name: project_value
  args: {x: SOURCE.VALUE}
```
