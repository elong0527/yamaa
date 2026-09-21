# Window on a Window-Derived Value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-row-window-on-window-result.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** reject a window expression whose source depends on a value
that only exists because of another window, even inside the same row
template.

**Input:** laboratory (LB) records carrying a test code (`LBTESTCD`),
a numeric result (`LBSTRESN`), and a visit number (`VISITNUM`).

**Variables:**

- `PREV_AVAL` would lag `AVAL` across visits within each subject.
- `CHG` would be derived after the window pass as `AVAL - PREV_AVAL`.
- `PREV2` would lag `CHG`, a value computed from a window result.

A window reads the rows its template constructed, in one pass with no
declared evaluation order between window expressions. A second window
depending on the first window's result — directly, or through a value
derived from one like `CHG` — would read values whose computation order
is unspecified, so the run is rejected before any data is read and no
artifact is accepted.

**Standard:** ADaM | **Domain:** ADLBC

## How to fix

Restructure the derivation so each window reads only values the row
template constructed directly, and each scalar reads window results
after the window pass. Here, drop `PREV2`: `CHG` already carries the
change from the previous visit, and no second window is needed.
