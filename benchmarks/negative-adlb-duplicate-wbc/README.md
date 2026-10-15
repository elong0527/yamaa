# Reject Duplicate WBC

**Goal:** add an absolute lymphocyte (`LYMPH`) record carrying
`AVAL` and `DTYPE`.

**Input:** collected laboratory records carrying the collected
result in `AVAL`, identified by subject, visit, and parameter
code, including white blood cell (WBC) counts and lymphocyte
fractions (`LYMLE`).

**Variables:**

- `AVAL`: the collected result on collected records. On the new `LYMPH`
  record it is the `WBC` count multiplied by the `LYMLE` fraction from the
  same subject and visit; that record is added only when both values are
  present and no `LYMPH` result was collected at the visit.
- `DTYPE`: `CALCULATION` on the new `LYMPH` record, empty on collected
  records.

**Note:** each contributing parameter may occur at most once within a
subject and visit. A repeated `WBC` or `LYMLE` result leaves no single value
to multiply, so the run stops with no artifact rather than choosing one
record by amount or position or adding both values.

## How to fix

Resolve the duplicate WBC records according to the study's data conventions
before calculating the absolute differential. The run then sees one WBC
record per subject and visit, for example:

```csv
STUDYID,USUBJID,PARAMCD,AVAL,PARAM,VISIT
YAMAA-01,YAMAA-01-101,WBC,34,Leukocyte Count (10^9/L),CYCLE 1 DAY 1
YAMAA-01,YAMAA-01-101,LYMLE,0.90,Lymphocytes (fraction of 1),CYCLE 1 DAY 1
```

Do not replace `ONLY` with `MIN`, `MAX`, or file-order selection unless that
choice is a documented clinical rule; those alternatives answer a different
question.
