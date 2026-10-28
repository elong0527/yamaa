# Replicate ECG Results at Planned Time Points

**Goal:** give each electrocardiogram (ECG) reading collected at a planned
time point one record per test, the QT interval as a number and the
overall interpretation as text, with the replicate number (`EGREPNUM`),
the planned time point (`EGTPT`, `EGTPTNUM`) and the result in standard
character form (`EGSTRESC`) on every record.

**Input:** long-form Operational Data Model (ODM) rows, one row per
collected item. Each planned time point has its own ECG form, and each
replicate ECG taken there is one repeat of the ECG item group, numbered in
the item-group repeat key. A repeat holds the ECG's date and time, its QT
interval in milliseconds, and the reader's interpretation.

**Variables:**

- `EGSEQ` numbers every record of a subject in one sequence: by time
  point number, then replicate number, then test code (`INTP` before
  `QTAG`).
- `EGTESTCD` and `EGTEST` name the test: `QTAG`, "QT Interval,
  Aggregate", for the interval and `INTP`, "Interpretation", for the
  interpretation.
- `EGORRES` is the result as collected: the interval in milliseconds, or
  the interpretation, `NORMAL` or `ABNORMAL`.
- `EGORRESU` is `msec` for the interval and empty for the interpretation.
- `EGSTRESC` is the result in standard form, always as text, for both
  tests: the collected result, since the interval was collected in
  standard units and the interpretation in standard terms.
- `EGSTRESN` is the interval as a number; empty for the interpretation,
  which has no numeric result.
- `EGSTRESU` is `msec` for the interval and empty for the interpretation.
- `EGREPNUM` is the item-group repeat key: it numbers the replicate ECGs
  of one time point, starting again at each time point.
- `EGDTC` is the date and time of the ECG, shared by both of its records;
  empty when the repeat holds none.
- `EGTPT` is the planned time point of the form: `PRE-DOSE` or
  `2 H POST-DOSE`.
- `EGTPTNUM` is the planned time point number: 1 before the dose and 2
  after it.

**Note:** a result left blank on the form gives no record, while the other
result of the same ECG keeps its record.
