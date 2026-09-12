# ADaM ADVS: derive a body surface area parameter

Collected ADVS vital signs records produce one output row per collected record
plus one BSA row per complete subject and visit:

- `PARAMCD` and `PARAM` retain each collected parameter and identify a new BSA
  record as body surface area;
- `AVAL` retains each collected value. For BSA, it is the square root of height
  in centimetres multiplied by weight in kilograms, divided by 3600. No BSA row
  is added when either contributor is absent or missing;
- `DTYPE` is `CALCULATION` on a derived BSA row and missing on a collected row.

Input containing a BSA parameter is rejected. Each contributing parameter must
occur at most once within a subject and visit.
