# ADaM ADAE: number a subject's serious events in onset order

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-serious-event-sequence.html)

This example uses collected adverse events to derive one record per event:

- `AESER` marks an event the investigator reported as serious;
- `ASTDT` is the date the event started;
- `SERSEQ` numbers a subject's serious events from one in onset order, and is
  empty for an event that is not serious.

A subject who reported no serious event keeps every record and receives no
number on any of them. Numbering the earliest of their events instead would
present an ordinary event as the first serious one.
