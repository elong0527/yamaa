# Flag adverse events for protocol review

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-protocol-review-window.html)

**Goal:** flag adverse events needing protocol review.

**Input:** collected adverse events with reported term, protocol
review score, start date, and start datetime.

**Variables:**

- `ASTDT`: analysis start date as collected; missing when the start
  date is missing.
- `ASTDT2`: calendar date of the collected start datetime; missing
  when that datetime is missing.
- `REVIEWFL`: `Y` when the event falls in either review window (the
  start date within 1 January through 31 January 2025, or the start
  datetime at or after 09:30 on 1 February 2025), its reported term
  starts with the text `INF_` (the underscore is a literal character,
  so `INFXREACTION` does not match), and its protocol review score is
  at least -1.5; otherwise `N`.

**Note:** a missing start date leaves `ASTDT` missing; a missing
start datetime leaves `ASTDT2` missing. An event with neither review
date known is `N` in `REVIEWFL`.

**Standard:** ADaM | **Domain:** ADAE
