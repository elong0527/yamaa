# Derive Body Surface Area

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-bsa.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each collected vital signs record through and add
one body surface area (BSA) record for each subject and visit that
has both a height and a weight result.

**Input:** collected vital signs with height records coded
`HEIGHT` and weight records coded `WEIGHT`; height is collected
in centimeters and weight in kilograms.

**Variables:**

- `AVAL` keeps each collected result. On a body surface area
  record it is the Mosteller value: the square root of height in
  centimeters times weight in kilograms, divided by 3600. No
  record is added when the visit's height or weight is absent or
  has no result.
- `DTYPE` is `CALCULATION` on a body surface area record and
  blank on a collected record.

**Standard:** ADaM | **Domain:** ADVS
