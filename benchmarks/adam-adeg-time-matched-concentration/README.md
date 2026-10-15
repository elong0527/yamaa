# Time-Matched Plasma Concentrations for ECG Time Points

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adeg-time-matched-concentration.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** give each electrocardiogram (ECG) time-point record of a QT study
the plasma concentration of the parent drug measured at the same nominal
time, for the concentration-QT analysis: flag the records with a matching
pharmacokinetic (PK) sample (`ECGPCFL`), carry the concentration
(`PKCONC`), and flag the records the concentration-QT analysis uses
(`CQTANLFL`).

**Input:** the ECG time-point averages with their actual treatment
(`TRTA`), time-matched baseline and change from baseline, one record per
subject, parameter, analysis period (`APERIOD`), nominal day within the
period (`APERDAY`) and nominal time point (`ATPT`, hours after the dose
time in `ATPTN`); and the plasma concentrations, one record per subject,
period, period day, time point and analyte (the parent drug `DRGX` and a
metabolite), with `AVAL` empty and `LLOQFL` `Y` for a result below the
lower limit of quantification.

**Variables:**

- `TRTA`, `AVALU`, `ABLFL`, `BASE`, `CHG`, `BASETYPE`: as in the ECG
  input.
- `ECGPCFL`: `Y` when the concentrations hold a parent-drug sample of the
  same subject, period, period day and nominal time point, whatever its
  result; no value otherwise. A sample at another time point, even the
  nearest one, and a metabolite result never match.
- `PKCONC`: that sample's parent-drug concentration in ng/mL, as recorded
  and not rounded, on records of any treatment. Nothing is imputed: no
  value for a result below the limit or when no sample matches, except on
  placebo records (`TRTA` "Placebo"), which hold 0 whenever no quantified
  result exists, the baseline day included.
- `CQTANLFL`: `Y` when the record has both a change from baseline and a
  concentration; no value otherwise.

**Note:** a PK sample without an ECG record at its time point adds no
record, and whether a record is placebo follows its own period's treatment.

**Standard:** ADaM | **Domain:** ADEG
