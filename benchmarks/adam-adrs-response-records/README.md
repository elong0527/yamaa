# Derive Investigator Overall Response Records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adrs-response-records.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each investigator-scored overall response assessment
into the analysis dataset as one record under the `OVR` (Overall
Response by Investigator) parameter, adding the completed date
(`ADT`), the study day (`ADY`), the response rank (`AVAL`), and the
per-date flag (`ANL01FL`).

**Input:** one record per collected response assessment carrying the
test code `RSTESTCD`, the evaluator `RSEVAL`, the assessment date
`RSDTC`, and the result `RSSTRESC`, plus the treatment start `TRTSDT`
from the subject-level dataset. Only assessments the investigator
(`INVESTIGATOR`) scored as overall response (`OVRLRESP`) leave a
record; a target-lesion assessment (`TRGRESP`) and an independent
assessor (`INDEPENDENT ASSESSOR`) assessment leave none.

**Variables:**

- `RSSEQ` numbers the assessment within the subject; `PARAMCD` and
  `PARAM` carry the fixed parameter code and description.
- `RSDTC` carries the collected assessment date as it was recorded.
- `ADT` is the completed analysis date. A fully collected date is
  used as it stands; a year and month without a day complete to the
  first of the month, and a year alone to 1 January. An assessment
  with no readable date stops the run.
- `ADY` is the study day of the assessment, counting the treatment
  start as day one.
- `AVALC` is the collected response: complete response (`CR`),
  partial response (`PR`), stable disease (`SD`), neither complete
  response nor progressive disease (`NON-CR/NON-PD`), progressive
  disease (`PD`), or not evaluable (`NE`).
- `AVAL` ranks the response from best to worst as `1` (`CR`), `2`
  (`PR`), `3` (`SD`), `4` (`NON-CR/NON-PD`), `5` (`PD`), or `6`
  (`NE`).
- `ANL01FL` is `Y` for one record at each assessment date: the worst
  (largest `AVAL`) response that day, with the lowest `RSSEQ`
  breaking ties when a day carries the same worst response twice. It
  is empty otherwise.

**Note:** a completed date counts exactly as a collected one in every
comparison that follows, so grouping by date and measuring from the
treatment start never depend on how much of the date was collected. An
assessment dated exactly on the treatment start lands on study day
one.

**Standard:** ADaM | **Domain:** ADRS
