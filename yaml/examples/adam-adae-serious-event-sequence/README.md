# Number serious events in onset order

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-serious-event-sequence.html)

**Goal:** number each subject's serious (`AESER` is `Y`) events in
onset order as `SERSEQ`: earliest onset first, same-day onsets by
`AESEQ`.

**Input:** collected adverse events, carrying the serious flag
`AESER` and the onset date `ASTDT`.

**Variables:**

- `SERSEQ`: the subject's serious-event number, starting at 1 for
  the earliest onset; empty for an event that is not serious.

**Note:** numbering starts at each subject's first serious event, so
earlier non-serious events stay unnumbered, and a subject with no
serious event keeps every record unnumbered.

**Standard:** ADaM | **Domain:** ADAE
