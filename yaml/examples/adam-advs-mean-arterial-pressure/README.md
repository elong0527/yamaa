# ADaM ADVS: derive mean arterial pressure

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-advs-mean-arterial-pressure.html)

Collected ADVS blood-pressure records produce one output row per collected
record plus one MAP row per complete subject and visit:

- `PARAMCD` and `PARAM` retain each collected parameter and identify a new MAP
  record as mean arterial pressure;
- `AVAL` retains each collected value. For MAP, it is two-thirds of diastolic
  pressure plus one-third of systolic pressure. No MAP row is added when either
  contributor is absent or missing;
- `DTYPE` is `CALCULATION` on a derived MAP row and missing on a collected row.

Input containing a MAP parameter is rejected. Each contributing parameter must
occur at most once within a subject and visit; an ambiguous group is rejected.
