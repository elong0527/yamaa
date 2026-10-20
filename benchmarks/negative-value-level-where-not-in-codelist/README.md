# Reject a Value Description Selected by a Test Code the Study Never Lists

**Goal:** carry each vital-signs result as collected (`VSORRES`) for each
subject and test, with a description of each test's results for the
data-definition document.

**Input:** collected vital signs with the study and subject identifiers,
one record per subject and test, with the test code and the result as
collected.

**Variables:**

- `VSTESTCD` is the vital-signs test code, one of the codes the study's
  test-code terminology lists: `SYSBP`, `DIABP` or `PULSE`.
- `VSORRES` would be the result as collected.

**Note:** each test's results have their own description, which finds its
records by their test code. A record can carry only a code the terminology
lists, so a description that names any other code could never describe a
record. The pulse description names `HR`, a code the study's terminology
does not list, and the run is refused before any record is read.

## How to fix

Decide which test the description is for. It describes the pulse rate,
which the study records under the code `PULSE`, so name that code:

```yaml
- id: PULSE
  where: {VSTESTCD: [PULSE]}
  label: Pulse Rate
```

When the study also records heart rate as a test of its own, add `HR` to
the study's test-code list and give it its own description instead. `HR` is
a published test code, so it is an ordinary item; only a code the published
list lacks is a sponsor addition, declared with `extended: true`.
