# Derive Investigator Overall Response Records

**Goal:** carry each investigator-scored overall response assessment
into the analysis dataset as one record under the `OVR` (Overall
Response by Investigator) parameter, adding the completed date
(`ADT`) with its imputation flag (`ADTF`), the study day (`ADY`), the
response rank (`AVAL`), and the per-date flag (`ANL01FL`).

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
- `ADTF` is `D` when the day of `ADT` was completed and `M` when the
  month and day were; empty for a fully collected date.
- `ADY` is the study day of the assessment, counting the treatment
  start as day one.
- `AVALC` is the collected response: complete response (`CR`),
  partial response (`PR`), stable disease (`SD`), neither complete
  response nor progressive disease (`NON-CR/NON-PD`), progressive
  disease (`PD`), or not evaluable (`NE`).
- `AVAL` codes the response as `1` (`CR`), `2` (`PR`), `3` (`SD`),
  `4` (`NON-CR/NON-PD`), `5` (`PD`), or `6` (`NE`).
- `ANL01FL` is `Y` for one record at each assessment date: the worst
  evaluable response that day (the largest `AVAL` other than `NE`),
  with the lowest `RSSEQ` breaking ties when a day carries the same
  worst response twice. `NE` is flagged only on a date with no
  evaluable response. It is empty otherwise.

**Note:** the study's statistical analysis plan (SAP) sets these rules:
this parameter holds the investigator's overall response alone (other
evaluators and tests would be parameters of their own), a partial date
completes to its first possible day and is flagged, and the worst
evaluable response of a date is the one analysed. A completed date
counts exactly as a collected one in every comparison that follows, so
grouping by date and measuring from the treatment start never depend on
how much of the date was collected. An assessment dated exactly on the
treatment start lands on study day one.