# ADaM ADLB: reject duplicate WBC inputs for an absolute differential

Collected analysis records attempt to produce one absolute lymphocyte result
per subject and visit:

- `AVAL` would contain WBC multiplied by the `LYMLE` fraction, but two WBC
  records in one visit leave no unique value to use. The run fails rather than
  choosing one record or adding both values.
- `DTYPE` would be `CALCULATION` on a derived record.

## How to fix

Resolve the duplicate WBC records according to the study's data conventions
before calculating the absolute differential. Do not replace `ONLY` with
`MIN`, `MAX`, or file-order selection unless that choice is a documented
clinical rule; those alternatives answer a different question.
