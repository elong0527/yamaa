# Prepare investigator overall response records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adrs-overall-response-records.html)

**Goal:** derive overall-response records for the `OVR` (Overall
Response by Investigator) parameter, carrying the collected date
`RSDTC` and result `AVALC` with the completed date `ADT`, study
day `ADY`, response rank `AVAL`, and per-date flag `ANL01FL`.

**Input:** one record per collected response assessment carrying
the test code `RSTESTCD`, the evaluator `RSEVAL`, the assessment
date `RSDTC`, and the result `RSSTRESC`, plus the treatment start
`TRTSDT` from the subject-level dataset. Only assessments the
investigator (`INVESTIGATOR`) scored as overall response
(`OVRLRESP`) leave a record; a target-lesion assessment
(`TRGRESP`) and an independent assessor (`INDEPENDENT ASSESSOR`)
assessment leave none.

**Variables:**

- `ADT` is the completed analysis date. A fully collected date is
  used as it stands; a year and month without a day is completed to
  the first of the month.
- `ADY` is the study day of the assessment, counting the treatment
  start as day one.
- `AVAL` is the rank of the response, from best to worst: `1` for a
  complete response (`CR`), `2` for a partial response (`PR`), `3`
  for stable disease (`SD`), `4` for neither complete response nor
  progressive disease (`NON-CR/NON-PD`), `5` for progressive disease
  (`PD`), and `6` for not evaluable (`NE`).
- `ANL01FL` is `Y` for one record at each assessment date: the worst
  (largest `AVAL`) response that day, with the lowest `RSSEQ`
  breaking ties when a day carries the same worst response twice. It
  is missing otherwise.

**Note:** a completed date counts exactly as a collected one in every
comparison that follows, so grouping by date and measuring from the
treatment start never depend on how much of the date was collected.

**Standard:** ADaM | **Domain:** ADRS
