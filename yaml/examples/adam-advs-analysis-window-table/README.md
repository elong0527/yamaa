# Assign records to analysis windows from a window table

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-advs-analysis-window-table.html)

**Goal:** place each systolic blood pressure (`SYSBP`) measurement
into its analysis window, assigning the visit order number
(`AVISITN`), the window target day (`AWTARGET`), and the distance
from target (`AWTDIFF`), and marking `ANL01FL` on the record that
represents its window.

**Input:** pre-derived measurement records carrying `ADT`
(analysis date), `ADY` (relative study day), and `AVAL` (measured
value) alongside the visit as collected, plus one study-wide
window table shared by all parameters that gives, for each
analysis visit, its order number, its first and last study day,
and its target day.

**Variables:**

- `AVISITN` is the order number of the analysis visit whose
  window holds the record: the one window, matched within the
  same study, whose range from its first through its last study
  day, both ends inclusive, contains the record's study day. It
  is missing when no window contains the day.
- `AWTARGET` is the target day stated by the matched window;
  missing when no window matched.
- `AWTDIFF` is the record's study day minus its target day,
  negative before the target and positive after; missing when
  there is no window or no study day.
- `ANL01FL` is `Y` on the record nearest its window target among
  records for the same study, subject, parameter, and analysis
  visit, with the lower sequence number breaking a tie; blank on
  every other record, including any record with no window.

**Note:** an assigned window brings its order number and target
day together, so a record with no window has none of the three. A
record has no window when it has no study day, when its day falls
in a gap between stated ranges, or when its day sits on or past
the first day of a window whose last day was never stated: an
absent bound is not an open-ended one. Window tables are
study-specific.

**Standard:** ADaM | **Domain:** ADVS
