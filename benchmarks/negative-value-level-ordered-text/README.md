# Reject Ordering Visits by Their Names

**Goal:** carry each subject's glucose result, `AVAL`, with `PARAMCD` and the
visit `AVISIT`, describing the results from week 4 on in the data-definition
document.

**Input:** the subjects' glucose results, one row per subject per visit.

**Variables:**

- `PARAMCD` is `GLUC`, glucose.
- `AVISIT` is the name of the visit, such as `WEEK 2`.
- `AVAL` is the glucose result in mg/dL.

**Note:** the later results are described as those whose visit name comes at
or after `WEEK 4`. A visit name is text, and text in alphabetical order does
not follow the calendar: `WEEK 10` sorts before `WEEK 4`. Only a number or a
date can be ordered, so the description is rejected before any data is read.

## How to fix

Select the later visits by the visit number, which orders as the calendar
does:

```yaml
- name: AVISITN
  type: int
  label: Analysis Visit (N)
# ...
where:
  PARAMCD: [GLUC]
  AVISITN: {comparator: GE, values: ['4']}
```

When the dataset carries no visit number, list the later visits by name
instead, such as `AVISIT: [WEEK 4, WEEK 6]`.
