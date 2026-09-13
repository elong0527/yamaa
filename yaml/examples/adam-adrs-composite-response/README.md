# Composite response from efficacy, safety, discontinuation

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adrs-composite-response.html)

**Goal:** decide a composite responder value (`AVALC`), its numeric
companion (`AVAL`), and the reason behind the assignment (`ARSN`)
for each subject and visit.

**Input:** a table of percent change from baseline values (`PCHG`)
under input parameter code EASI (Eczema Area and Severity Index),
matched to a subject-level file with a serious adverse event flag
(`SAEFL`) and a reason for discontinuation (`DCSREAS`). `PARAMCD`
is fixed to `RESP75` and `PARAM` to `EASI-75 Response`.

**Variables:**

- `AVALC` is the responder value: `RESPONDER`, `NON-RESPONDER`,
  or `NOT EVALUABLE`.
- `AVAL` is 1 for a responder, 0 for a non-responder, and missing
  when the subject is not evaluable.
- `ARSN` records which check assigned the value: `SAFETY OR
  DISCONTINUATION RULE`, `COMPONENT MISSING`, `THRESHOLD MET`, or
  `THRESHOLD NOT MET`.

**Note:** the checks apply in a fixed order. A subject with a
serious adverse event or any discontinuation reason is a
non-responder whatever the efficacy value; otherwise a subject
with no efficacy value is not evaluable, one with a percent change
of -75 or less (a reduction of at least 75%) is a responder, and
everyone else is a non-responder. So a subject meeting the
efficacy mark but flagged for safety is a non-responder, and a
subject with no efficacy value who discontinued is a non-responder
rather than not evaluable.

**Standard:** ADaM | **Domain:** ADRS
