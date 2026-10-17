# Baseline Strictly Before First Dose

**Goal:** flag each subject's baseline result (`ABLFL`) as the latest
result dated before the day treatment starts.

**Input:** one `spec.yaml` over vital-sign results carrying subject,
sequence number (`VSSEQ`), assessment date (`ADT`), result (`AVAL`),
and the subject's treatment start date (`TRTSDT`), all carried through
unchanged.

**Variables:**

- `ABLFL` is `Y` on the latest result dated strictly before treatment
  start and blank on every other result. A result taken on the day
  treatment starts is never the baseline, so a subject whose earliest
  result falls on that day has no baseline.
