# Flag Any Reported Dose Adjustment

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-dose-adjustment.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one record per subject flagging any reported dose
adjustment (`DOSADJFL`).

**Input:** subject list plus exposure (EX) records carrying the
reason for a dose adjustment (`EXADJ`), exposure as collected
(EC) records carrying the same reason (`ECADJ`), and
findings-about (FA) records carrying a test code (`FATESTCD`),
the object of the finding (`FAOBJ`), and its result
(`FASTRESC`).

**Variables:**

- `DOSADJFL` holds `Y` when any source holds a qualifying
  record: a filled `EXADJ` value, a filled `ECADJ` value, or a
  findings-about row with `FATESTCD` of `OCCUR`, `FAOBJ` of
  `DOSE ADJUSTMENT`, and `FASTRESC` of `Y`. A pre-specified
  (`PRESP`) or non-dose-adjustment finding does not qualify. It
  holds `N` when the subject has at least one record in any of
  the three sources but no qualifying record. It stays empty
  when the subject is absent from all three sources.

**Note:** a record belongs to a subject only when both the study
and subject identifiers match, and a source record for a subject
not in the subject list adds no record.

**Standard:** ADaM | **Domain:** ADSL
