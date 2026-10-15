# Exposure Adjusted Adverse Event Incidence

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-exposure-adjusted-incidence.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** report event-specific time at risk and exposure adjusted incidence
per actual treatment, with subject-level risk attribution alongside it.

**Input:** treated subjects with first and last dose dates, and adverse event
analysis records with term and complete onset date.

**Variables:**

- `TRT01A` and `AEDECOD` identify actual treatment and the prespecified terms
  DIARRHOEA and HEADACHE, including terms a subject never experiences.
- In ADRISK, `FIRSTDT` is the first onset on or after first dose and on or
  before 30 days after last dose; empty if none qualifies. `EVENTN` is 1
  when such an onset exists and 0 otherwise. `RISKDAYS` counts both first
  dose and first eligible onset, or both first dose and collection-window
  end when no event qualifies.
- In ADINC, `NSUBJ` counts subjects with an eligible event. `PERSONDAYS`
  totals their term-specific risk days, including event-free subjects.
  `PERSONYRS` is that total divided by 365.25; `EAIR100` is `NSUBJ` per 100
  person-years. Neither numeric result is rounded.

**Note:** repeated events contribute only their first eligible onset. An
onset before first dose or after the collection window contributes no event
and does not shorten time at risk. Different terms have separate denominators.

**Standard:** ADaM | **Domain:** ADAE
