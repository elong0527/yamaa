# Mark serious events from collected seriousness criteria

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ae-seriousness-criteria.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `AESER` for each adverse event (AE) from its collected
seriousness criteria while retaining the reported term and criterion flags.

**Input:** collected adverse event terms with flags for death, life threat,
hospitalization, disability, congenital anomaly, and medical importance.

**Variables:**

- `AETERM` is the adverse event term reported by the site.
- `AESER` is `Y` when any seriousness criterion is `Y`; it is `N` when no
  criterion is `Y`, including when every criterion is empty.
- `AESDTH` is `Y` when the event resulted in death; otherwise it retains the
  collected `N` or empty value.
- `AESLIFE` is `Y` when the event was life threatening; otherwise it retains
  the collected `N` or empty value.
- `AESHOSP` is `Y` when the event required or prolonged hospitalization;
  otherwise it retains the collected `N` or empty value.
- `AESDISAB` is `Y` when the event caused persistent or significant disability
  or incapacity; otherwise it retains the collected `N` or empty value.
- `AESCONG` is `Y` when the event caused a congenital anomaly or birth defect;
  otherwise it retains the collected `N` or empty value.
- `AESMIE` is `Y` for another medically important serious event; otherwise it
  retains the collected `N` or empty value.

**Standard:** SDTM | **Domain:** AE
