# Reject a File Path With an Empty Folder Name

**Goal:** carry each laboratory result (`AVAL`) onto the laboratory analysis
dataset, one record per subject and parameter.

**Input:** pre-analysis laboratory results named by the path
`input//lb.csv`.

**Variables:**

- `PARAMCD` would be the parameter code as collected.
- `AVAL` would be the result as collected.

**Note:** the path to the results joins the folder and the file name with
two separators, which leaves an empty folder name between them. Systems
disagree on what such a path names, so the same study could read different
files in different places; the run is rejected before any data is read and
no artifact is accepted.

## How to fix

Write the path with one separator between each folder and the next name:

```yaml
input:
  LB: {path: input/lb.csv, types: {AVAL: float}}
```
