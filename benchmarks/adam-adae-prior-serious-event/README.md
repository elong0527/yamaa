# Prior Serious Event

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-prior-serious-event.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** for each non-serious adverse event, name the subject's most
recent earlier serious event and the subject's first serious event.

**Input:** adverse event records with study, subject, and sequence
(`STUDYID`, `USUBJID`, `AESEQ`), the dictionary term (`AEDECOD`),
and whether the event was serious (`AESER`).

**Variables:**

- `PRIOR_SAEFL`: `Y` on a non-serious event when the subject had a
  serious event with a smaller sequence number, empty otherwise.
- `PRIOR_SAEDECOD` / `PRIOR_SAESEQ`: the term and sequence number of
  that most recent earlier serious event, empty when there is none.
- `FIRST_SAEDECOD` / `FIRST_SAESEQ`: the term and sequence number of
  the subject's first serious event, empty when the subject had none.

**Note:** the serious events are recorded first, and the non-serious
events then look back at those completed records. A serious event's
own prior and first serious event fields stay empty, because the
first pass cannot read the records it is still writing. One subject
with no serious events shows that the look-back stays empty rather
than failing.

**Standard:** ADaM | **Domain:** ADAE
