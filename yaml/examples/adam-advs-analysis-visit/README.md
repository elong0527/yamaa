# Assign vital signs records to analysis windows

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-advs-analysis-visit.html)

**Goal:** assign each systolic blood pressure (`SYSBP`) record to
its analysis window, adding `AVISITN` and `ANL01FL`.

**Input:** vital signs records carrying the collected visit number
(`VISITNUM`), the analysis date (`ADT`), the relative analysis day
(`ADY`), and the measured value (`AVAL`).

**Variables:**

- `AVISITN` is the numeric order of the record's analysis window:
  `-1` for `SCREENING` (days before day 0), `0` for `BASELINE`
  (day 1 only, since study days skip from day `-1` to day `1`),
  `2` for `WEEK 2` (day 2 up to but not including day 22), `4`
  for `WEEK 4` (day 22 up to but not including day 43), and `99`
  for `POST-TREATMENT` (day 43 onward, with no upper bound). It
  is missing when the relative day is missing, so such a record
  belongs to no window.
- `ANL01FL` is `Y` for the record that represents its subject and
  parameter in each window: the earliest by relative day, with
  the lower sequence number breaking ties on the same day;
  missing otherwise. A record with no relative day is never
  flagged.

**Note:** windows follow the relative day rather than the
collected visit name, so a record the site left unscheduled still
belongs to whichever window its day falls in, and one recorded
past the last boundary falls in the open-ended final window.

**Standard:** ADaM | **Domain:** ADVS
