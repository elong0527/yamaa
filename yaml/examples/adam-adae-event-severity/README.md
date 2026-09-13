# Carry each event's severity into ADAE

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-event-severity.html)

**Goal:** one Analysis Dataset for Adverse Events (ADAE) row per
collected adverse event (AE), carrying `AEDECOD` and `AESEV`.

**Input:** collected adverse events with their coded terms, plus
supplemental records carrying the severity recorded for each event.

**Variables:**

- `AEDECOD` is the dictionary-derived term collected for the event;
  always present from the collected record.
- `AESEV` is the severity recorded for that same event; empty when
  the event has no supplemental record.

**Note:** severity is matched on the subject identifiers together with
the event sequence number, so two events for one subject keep their
own severities apart.

**Standard:** ADaM | **Domain:** ADAE
