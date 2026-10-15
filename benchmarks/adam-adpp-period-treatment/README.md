# Link each PK parameter to the period and treatment of its dose

**Goal:** give every pharmacokinetic (PK) parameter record of a two-period
crossover the analysis period (`APERIOD`), actual treatment (`TRTA`) and
actual dose (`DOSEA`, unit `DOSEU`) of the dose its profile follows.

**Input:** PK parameters (PP), one record per subject, analyte (`PPCAT`),
parameter and profile, with the date and time of the profile's reference
dose (`PPRFTDTC`); and exposure (EX), one record per dose with its start
date and time, treatment, dose, unit and epoch (`TREATMENT 1` or
`TREATMENT 2`).

**Variables:**

- `PARCAT1`, `PARAMCD`, `PARAM` and `PPRFTDTC` carry the analyte, the
  parameter and the reference date and time from PP.
- `APERIOD` is the number in the epoch of the subject's dose that starts
  exactly at the parameter's reference date and time.
- `TRTA`, `DOSEA` and `DOSEU` are that dose's treatment, dose and unit, as
  recorded.
- `AVAL` and `AVALU` are the parameter's standard result and unit.

**Note:** the full date and time must agree, so a dose earlier or later on
the same day, the subject's most recent earlier dose and another subject's
dose at that time are not the parameter's dose. A parameter with no such
dose keeps its record with all four values empty. Two doses of one subject
with one start, or a dose in an epoch other than `TREATMENT` followed by a
number, stops the run.
