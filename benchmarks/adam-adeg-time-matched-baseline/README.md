# Time-Matched ECG Baselines by Period

**Goal:** give each electrocardiogram (ECG) time-point record of a QT study
its time-matched baseline: flag the baseline records (`ABLFL`), carry the
baseline value (`BASE`), compute the change from baseline (`CHG`), and name
the baseline each record is compared with (`BASETYPE`).

**Input:** one record per subject, parameter, analysis period (`APERIOD`),
nominal day within the period (`APERDAY`) and nominal time point (`ATPT`,
with its number of hours after the reference dose time in `ATPTN`), each
holding the average of the replicate ECGs taken at that time (`AVAL`, unit
`AVALU`).

**Variables:**

- `ABLFL`: `Y` on every record of day -1 of a period, that period's
  baseline day; no value on every other day.
- `BASE`: the value recorded on the same period's baseline day at the same
  nominal time point for the same subject and parameter; on the baseline
  day, the record's own value. No value when that baseline day did not
  record the time point or the period has no baseline day; nothing stands
  in for it, neither another time point, day or period nor the dosing
  day's pre-dose value.
- `CHG`: `AVAL` minus `BASE`, not rounded; no value on the baseline day
  and wherever `BASE` has no value.
- `BASETYPE`: "Period " followed by the period number, " Baseline Day "
  and the time point, for example "Period 2 Baseline Day 2 h", on every
  record, including those with no baseline.

**Note:** every period has its own baseline day, so a subject has one
baseline record per period and time point, and each of them has a baseline
type of its own.
