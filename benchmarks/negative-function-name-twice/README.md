# Reject Two Project Routines With One Name

**Goal:** carry the source identifier through as `ID` and return the
project routine's result as `RESULT`.

**Input:** a source file with `ID` and `VALUE`, where `VALUE` holds the
numeric value passed to the routine.

**Variables:**

- `ID` is the source identifier, carried through unchanged.
- `RESULT` would be the result of passing the source `VALUE` to the
  `project_value` routine.

**Note:** the project lists two routine files, and both define a routine
named `project_value`: the second is a working copy of the first. The name
no longer says which routine a call runs, so the run is rejected before any
project code or data is read, and no artifact is accepted.

## How to fix

Keep one file per routine name. Remove the working copy from the project's
list, or give its routine a name of its own if both are meant to be used:

```yaml
functions:
- functions/project_value.yaml
```
