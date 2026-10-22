# Reject a Project Routine Whose Own Test Case Fails

**Goal:** carry the source identifier through as `ID` and return the
project routine's result as `RESULT`.

**Input:** a source file with `ID` and `VALUE`, where `VALUE` holds the
numeric value passed to the routine.

**Variables:**

- `ID` is the source identifier, carried through unchanged.
- `RESULT` would be the result of passing the source `VALUE` to the
  `project_value` routine, which returns the value it is given.

**Note:** before any data is read, the routine is run on the test cases
written beside it. Its ordinary case expects 15.0 for an input of 1.5, while
the routine returns 1.5, so either the routine or its stated expectation is
wrong. The run is rejected before any data is read, and no artifact is
accepted.

## How to fix

Decide which side is wrong. The routine is meant to return its argument
unchanged, so here the expected result was mistyped; correct the test case:

```yaml
- id: ordinary
  covers:
  - normal
  - numeric-comparison
  args:
    x: 1.5
  result: 1.5
```

When the expected result is right, fix the routine in both host packages
instead. Never delete a failing test case to make the run pass.
