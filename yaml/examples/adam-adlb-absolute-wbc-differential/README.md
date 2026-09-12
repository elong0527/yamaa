# ADaM ADLB: derive absolute WBC differentials

Collected analysis records produce absolute lymphocyte results at the subject
and visit grain:

- `AVAL` retains each collected value. A new `LYMPH` record contains WBC
  multiplied by the `LYMLE` fraction from the same visit. No record is added
  when either contributing result is absent or missing, or when `LYMPH`
  already exists.
- `DTYPE` is `CALCULATION` on a derived record and missing on a collected one.

Each contributing parameter must occur at most once within a subject and visit;
an ambiguous source group is rejected rather than selected by value or order.
