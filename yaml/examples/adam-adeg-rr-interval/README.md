# Rederive the RR interval from heart rate

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adeg-rr-interval.html)

**Goal:** add a rederived RR duration record, coded `RRR`, at
each subject and analysis visit with a present and nonzero heart
rate (HR) result, computed as 60000 divided by that rate.

**Input:** collected electrocardiogram (ECG) records with heart
rate `HR` parameter codes per subject (`USUBJID`) and analysis
visit (`AVISIT`), carrying the record label (`PARAM`), result
(`AVAL`), and result unit (`AVALU`) in beats per minute
(`beats/min`).

**Variables:**

- `PARAMCD`: the source code, or `RRR` on the added record; the
  input must not already contain that code.
- `PARAM`: the source name, or the rederived RR duration name on
  the added record.
- `AVAL`: the source result, or 60000 divided by the heart rate
  on the added record; for example, a rate of 60 gives 1000.
- `AVALU`: the source unit, or milliseconds (`ms`) on the added
  record.

**Note:** the added record needs a heart rate result that is
present and not zero at the same subject and visit; otherwise no
record is added and the collected records stay unchanged.

**Standard:** ADaM | **Domain:** ADEG
