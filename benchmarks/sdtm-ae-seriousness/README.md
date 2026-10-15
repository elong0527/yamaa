# Seriousness Criteria

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ae-seriousness.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `AESER` for each adverse event (AE) from its collected
seriousness criteria while retaining the reported term and criterion flags.

**Input:** collected adverse event terms (`AETERM`) with `Y` or `N`
flags for death (`AESDTH`), life threat (`AESLIFE`), required or
prolonged hospitalization (`AESHOSP`), persistent or significant
disability or incapacity (`AESDISAB`), congenital anomaly or birth
defect (`AESCONG`), and another medically important serious event
(`AESMIE`); any flag may be empty.

**Variables:**

- `AESER` is `Y` when any seriousness criterion is `Y`; it is `N` when no
  criterion is `Y`, including when every criterion is empty.

**Note:** the term and the six criterion flags are kept as collected, so
an empty flag stays empty rather than becoming `N`.

**Standard:** SDTM | **Domain:** AE
