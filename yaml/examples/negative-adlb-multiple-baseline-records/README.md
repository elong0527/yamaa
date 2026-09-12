# ADaM ADLB: reject a subject with two baseline records for one parameter

This example uses a pre-derived analysis slice to prepare one record per
subject, parameter, and analysis date:

- `AVAL` is the analysis value of the result;
- `ABLFL` is `Y` on the record that serves as the subject's baseline for the
  parameter, and is empty on every other record.

A parameter has one baseline for a subject, because every change and percentage
change is measured from it. Two flagged records leave that comparison
undefined, and neither the earlier nor the later one can be preferred without
inventing a rule the study did not state. The disagreement is reported against
the subject and parameter rather than corrected, and no artifact is accepted
from this input. The expected output records the completed rows presented to
that check.

## How to fix

Correct the flag in the incoming slice so that one record carries it, choosing
the record the study's baseline definition selects -- ordinarily the latest
result on or before the first exposure. When the flag should be derived here
instead of trusted from the slice, derive it and let a tie be reported where it
arises:

```yaml
- name: ABLFL
  type: str
  label: Baseline Record Flag
  derivation:
    baseline_flag:
      group_by: [STUDYID, USUBJID, PARAMCD]
      date: ADT
      reference_date: TRTSDT
```

Do not widen the count to accept two records; a second baseline is a defect in
the data rather than a policy the analysis can adopt.
