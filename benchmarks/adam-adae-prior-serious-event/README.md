# Name Each Event's Prior Serious Event

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-prior-serious-event.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** for each adverse event, name the subject's most recent
earlier serious event (`PRIOR_SAEFL`, `PRIOR_SAEDECOD`,
`PRIOR_SAESEQ`), the subject's first serious event
(`FIRST_SAEDECOD`, `FIRST_SAESEQ`), and the subject's immediately
preceding event (`PREV_AEDECOD`).

**Input:** adverse event records with study, subject, and sequence
(`STUDYID`, `USUBJID`, `AESEQ`), the dictionary term (`AEDECOD`),
and whether the event was serious (`AESER`).

**Variables:**

- `PRIOR_SAEFL`: `Y` on a non-serious event when the subject had a
  serious event with a smaller sequence number, empty otherwise.
- `PRIOR_SAEDECOD` / `PRIOR_SAESEQ`: the term and sequence number of
  that most recent earlier serious event, empty when there is none.
- `FIRST_SAEDECOD` / `FIRST_SAESEQ`: the term and sequence number of
  the subject's first serious event, empty when the subject had no
  serious event.
- `PREV_AEDECOD`: the term of the subject's event with the next
  smaller sequence number, serious or not; empty on the subject's
  first event, and empty when the preceding event has no coded term.

**Note:** a serious event with no coded term still counts: the prior
flag is `Y` and its sequence number is named, while the term fields
stay empty. The serious events are laid down first, so a serious
event's own prior and first serious event fields stay empty, because
they cannot look back at records that are still being written. The
preceding event is named only after every event is in place, so it
can name an event of either kind.

**Standard:** ADaM | **Domain:** ADAE
