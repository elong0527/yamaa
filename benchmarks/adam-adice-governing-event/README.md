# Governing Intercurrent Event

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adice-governing-event.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** identify the single governing intercurrent event in each subject's
completed event history for an Analysis Data Model (ADaM) occurrence dataset.

**Input:** subject identifiers and collected intercurrent event records with
event sequence, type and complete event date.

**Variables:**

- `ASEQ`, `ADECOD1` and `ASTDT` identify the source sequence, type and date.
- `ICEPRIO` is 1 for ICE04 and ICE05, 2 for ICE01, 3 for ICE02 and 4 for
  ICE03: death or kidney replacement, prohibited medication, start of
  other protocol-prohibited therapy, and treatment discontinuation,
  respectively.
- `ICERANK` numbers each subject's events from 1 by increasing priority,
  then earliest date, then lowest source sequence.
- `GOVFL` is Y on position 1 and empty elsewhere; `SUPERFL` is Y on every
  other event and empty on position 1.
- `GOVTYPE`, `GOVDT` and `GOVSEQ` are the governing type, date and source
  sequence on every event of the subject.

**Note:** priority outranks chronology across event types. Equal priority
uses the earliest date, then lowest sequence. A subject without events has
no record. This is a completed-history summary; it does not choose the event
applicable at an earlier efficacy assessment.

**Standard:** ADaM | **Domain:** ADICE
