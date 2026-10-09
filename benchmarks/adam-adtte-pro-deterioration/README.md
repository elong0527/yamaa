# Time to Deterioration for a PRO Endpoint

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adtte-pro-deterioration.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag each visit-level patient-reported outcome (PRO) score as a
deterioration or not, then build one time-to-deterioration record per
subject with parameter code `TTDGHS` (`Time to Deterioration in Global
Health Status`), carrying `STARTDT`, `ADT`, `AVAL`, `CNSR`,
`EVNTDESC`, `CNSDTDSC`, `SRCDOM`, `SRCVAR`, and `SRCSEQ`.

**Input:** subject-level randomization and death dates (`RANDDT`,
`DTHDT`), visit-level PRO scores on a 0 to 100 scale, each with a
sequence number (`QSSEQ`), an assessment date (`ADT`), and a score
(`AVAL`), and disposition records naming disease progression, study
discontinuation, or withdrawal of consent (`DSDECOD`, `DSDTC`).

**Variables:**

- `DETERFL` is `Y` on an assessment dated after the subject's earliest
  assessment whose score has fallen at least 10 points below that
  earliest (baseline) score; blank otherwise, including on the baseline
  assessment itself.
- `STARTDT` is the randomization date, the origin from which the time
  to deterioration is counted.
- `ADT` is the analysis date: the deterioration date when a flagged
  assessment exists, else the death date when the subject died. A
  deterioration and a death on the same date count as deterioration.
  With neither, `ADT` is the last assessment dated on or before the
  first censoring reason, or randomization when no assessment qualifies.
- `AVAL` is the number of whole calendar months from `STARTDT` to
  `ADT`.
- `CNSR` is `0` when the subject has a deterioration or death event
  and `1` otherwise.
- `EVNTDESC` is `PRO DETERIORATION` for a deterioration event, `DEATH`
  for a death event, and `CENSORED` otherwise.
- `CNSDTDSC` is blank for an event, holds the censoring reason
  (`DISEASE PROGRESSION`, `STUDY DISCONTINUATION`, or `WITHDRAWAL OF
  CONSENT`) when one is available, and is `STUDY COMPLETION` otherwise.
- `SRCDOM` is `QS` for a deterioration or a censored record with a
  supporting assessment, and `ADSL` for a death or a record censored at
  randomization.
- `SRCVAR` is `ADT` for a deterioration or a censored record with a
  supporting assessment, `DTHDT` for a death, and `RANDDT` for a record
  censored at randomization.
- `SRCSEQ` is the sequence number from the deterioration or censoring
  assessment, and blank when the analysis date comes from a
  subject-level date.

**Note:** progression, discontinuation, and withdrawal are censoring
reasons, never events: a subject who progresses before any
deterioration is censored at the last assessment on or before the
progression date, and a deterioration dated the same day as the
progression still counts as the event.

**Standard:** ADaM | **Domain:** ADTTE
