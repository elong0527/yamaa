# Body Surface Area

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-bsa.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** carry each collected vital signs record through and add
one body surface area (BSA) record per subject and visit, holding
the computed value in `AVAL` and marking the new record with
`DTYPE`.

**Input:** collected vital signs with height records coded
`HEIGHT` and weight records coded `WEIGHT` per subject and visit,
with height measured in centimeters and weight in kilograms; the
new record is coded `BSA` and labeled `Body Surface Area (m^2)`.

**Variables:**

- `AVAL` retains each collected result. For body surface area, it
  is the square root of (height in centimeters multiplied by
  weight in kilograms, divided by 3600); no new record is added
  when either contributor is absent or missing.
- `DTYPE` is `CALCULATION` on a derived body surface area record
  and blank on a collected record.

**Standard:** ADaM | **Domain:** ADVS
