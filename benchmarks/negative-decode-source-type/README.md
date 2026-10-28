# Reject a Visit Name Read From Text Codes by a Visit Number

**Goal:** derive the analysis visit (`AVISIT`) of each laboratory result
from its visit number (`AVISITN`), as the study's visit terminology names
the visit.

**Input:** collected laboratory results with the study and subject
identifiers, the test code and the visit number.

**Variables:**

- `AVISITN` is the analysis visit number, the collected visit number.
- `AVISIT` would be the name the study's visit terminology gives the visit
  number, such as `Week 2` for `2`.

**Note:** the visit terminology writes its codes as text, while the visit
number is a number, and a number never equals text, so no visit could be
named. The request is rejected before any record is read and no artifact
is accepted.

## How to fix

Declare the visit codes as the integers they are, in the project file's
codelist:

```yaml
- id: VISIT
  name: Analysis Visit
  data_type: integer
  items:
    - {value: 1, decode: Baseline}
    - {value: 2, decode: Week 2}
```

Keep text codes only when the visit is identified by text, and then read
it from a text column.
