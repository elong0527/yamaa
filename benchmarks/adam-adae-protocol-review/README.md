# Protocol Review Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-protocol-review.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one row per adverse event (AE), flagging the events a
protocol reviewer needs to look at (`REVIEWFL`).

**Input:** adverse event records carrying the reported term
(`AETERM`), a protocol review score (`SCORE`), the collected start
date (`AESTDTC`, empty when never collected), and the collected
start datetime (`AESTDTM`, empty when never collected).

**Variables:**

- `ASTDT` is the analysis start date, taken from the collected start
  date as it stands. It is empty when no start date was collected.
- `ASTDT2` is the calendar date of the collected start datetime. It
  is empty when no start datetime was collected.
- `REVIEWFL` is `Y` when the event clears all three review checks;
  anything else is `N`.

**Note:** the three checks are a review window, a term pattern, and
a score floor. The window is either the collected start date falling
in January 2025, or the collected start datetime falling at or after
09:30 on 1 February 2025. The reported term must begin with the
literal text `INF_` (the underscore is a literal character, so
`INFXREACTION` does not match). The protocol review score must be
-1.5 or higher. An event with neither a start date nor a start
datetime known cannot fall in a window, so it is `N`.

**Standard:** ADaM | **Domain:** ADAE
