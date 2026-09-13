# ADaM ADAE: carry each event's severity from its supplemental record

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-event-severity.html)

This example uses collected adverse events and per-event severity records
with a `yamaa` specification to derive one row per adverse event:

- `AEDECOD` is the coded event collected for the subject's event;
- `AESEV` is the severity recorded for that same event: the supplemental
  record carrying the subject's identifiers and the event sequence number.
  An event with no supplemental record has none.

Severity joins on the subject identifiers and the event sequence number
together: two events of one subject carry different severities because the
sequence number keeps their supplemental records apart.
