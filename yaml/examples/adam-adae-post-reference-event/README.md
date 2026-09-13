# ADaM ADAE: flag an event after a specific reference event

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-post-reference-event.html)

Derives one analysis record per AE event from AE, retaining `AEDECOD` and
`ASTDT`:

- `AFTCOVFL` is `Y` for an event ordered strictly after the subject's first
  COVID-19 event; otherwise it is missing, including when the subject has no
  COVID-19 event.
