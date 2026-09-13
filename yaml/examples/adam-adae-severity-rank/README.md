# Rank adverse events by severity

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-severity-rank.html)

**Goal:** rank each subject's adverse events (AEs) from worst to
mildest: the numeric severity `ASEVN` orders events into a rank
`SEVRANK` and a dense rank `SEVLVL` in an Analysis Data Model (ADaM)
adverse event analysis dataset.

**Input:** collected Study Data Tabulation Model (SDTM) adverse
event records with the reported term and severity; the reported
severity is carried through as `ASEV` (empty when severity was not
reported).

**Variables:**

- `ASEVN` is the numeric form of `ASEV`: 1 for MILD, 2 for
  MODERATE, and 3 for SEVERE; empty when `ASEV` is empty.
- `SEVRANK` is the rank of the event among the subject's events
  ordered by `ASEVN`, worst first. Events with equal severity share
  a rank and the ranks they would otherwise fill are skipped, so a
  rank of 1 means the subject reported nothing worse.
- `SEVLVL` is the dense rank of the event: it numbers the distinct
  severities the subject reported, worst first with each severity
  counted once, so its largest value is how many different
  severities the subject reported.

**Note:** an event with no reported severity sorts after every event
with a reported severity, and events with no reported severity share
one rank with each other because nothing tells them apart.

**Standard:** ADaM | **Domain:** ADAE
