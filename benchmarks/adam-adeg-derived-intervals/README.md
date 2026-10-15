# Derived ECG Intervals

**Goal:** add one new record per subject (`USUBJID`) and analysis visit
(`AVISIT`) in study `CATH-01` for each of three ECG measures: the
Bazett-corrected QT interval (`QTCBR`), the Fridericia-corrected QT
interval (`QTCFR`), and the RR duration recomputed from heart rate
(`RRR`).

**Input:** collected electrocardiogram records with parameter codes `QT`
(QT interval duration), `RR` (RR interval duration), and `HR` (heart
rate), each carrying its label (`PARAM`), result (`AVAL`), and unit
(`AVALU`). QT and RR results are in milliseconds (`ms` or `msec`);
heart rate is in beats per minute (`beats/min`).

**Variables:**

- `PARAMCD`: `QTCBR`, `QTCFR`, or `RRR` on the new records; `QT`, `RR`,
  or `HR` on the collected ones. The input must not already contain
  `QTCBR`, `QTCFR`, or `RRR`.
- `PARAM`: "QTcB - Bazett's Correction Formula Rederived (ms)",
  "QTcF - Fridericia's Correction Formula Rederived (ms)", or
  "RR Duration Rederived (ms)" on the new records.
- `AVISIT`: the analysis visit the record belongs to, for example
  `BASELINE` or `WEEK 4`.
- `AVAL`: `QT / SQRT(RR / 1000)` for `QTCBR`,
  `QT / POWER(RR / 1000, 1 / 3)` for `QTCFR`, and `60000` divided by
  the heart rate for `RRR`; for example, a heart rate of 60 gives 1000.
- `AVALU`: `ms` on the new records.

**Note:** a QTc record is added only when both the QT and the RR
results are present at that visit; an `RRR` record is added only when
the heart rate is present and not zero. Collected QT and RR records in
a unit other than `ms` or `msec` fail the run, and each visit is
expected to carry at most one record per parameter code.
