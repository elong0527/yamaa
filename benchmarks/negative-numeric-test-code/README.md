# Reject Per-Test Lengths Over a Numeric Test Code

**Goal:** turn each collected sample's glucose and creatinine results into
one laboratory record per test, identified by the test code in `LBTESTCD`
and carrying the collected result in `LBORRES`.

**Input:** collected laboratory results with one record per subject sample,
carrying the glucose (`GLUC`) and creatinine (`CREAT`) results as collected.

**Variables:**

- `LBTESTCD` would be the laboratory's code for the test: `1001` for
  glucose and `1002` for creatinine.
- `LBORRES` would be the result as collected, at most four characters long
  for glucose and three for creatinine.

**Note:** each test states its own maximum result length, which applies to
the records carrying that test's code. A test code is text, but the test
code here is declared as an integer, so no record can carry the code a
length names and the lengths would check nothing. The run is rejected
before any data is read and no artifact is accepted.

## How to fix

Declare the test code as text, even though the laboratory's codes are
digits:

```yaml
- name: LBTESTCD
  type: str
  label: Lab Test or Examination Short Name
```
