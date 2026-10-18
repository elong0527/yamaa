# Adverse Events Take the Treatment Period Holding Their Start Date

**Goal:** give each adverse event the number of the subject's treatment
period (`APERIOD`) whose days hold the event's start date (`ASTDT`).

**Input:** one `spec.yaml` over adverse events, each carrying a reported term
(`AETERM`) and a start date, and a period table giving each subject's
treatment periods, each from a first day (`APERSDT`) to a last day
(`APEREDT`).

**Variables:**

- `APERIOD` is the number of the subject's period whose first and last days,
  both inclusive, hold the event's start date. It is blank when the event has
  no start date, when none of the subject's periods holds the date, or when
  the period that would hold it has no last day.

**Note:** the period is chosen among the subject's own periods by the start
date of the event itself, so a subject with several periods still gives each
event at most one period. An event outside every period is kept with the
period blank, and each such blank is counted so the run reports how often it
happened.
