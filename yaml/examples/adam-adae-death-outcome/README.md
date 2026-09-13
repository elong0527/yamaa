# Carry each subject's death onto every event

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-death-outcome.html)

**Goal:** build one Analysis Data Model (ADaM) adverse events row per
collected adverse event (AE), carrying the coded term (`AEDECOD`) and
start date (`ASTDT`) through and adding `DTHFL`, `DTHCAUS`, and `DTHDT`
for the subject's death.

**Input:** collected adverse events with coded term, outcome, and start
date, plus demographics with death date.

**Variables:**

- `DTHFL` is `Y` on every event of a subject with a fatal event or a
  death date recorded in demographics; empty otherwise.
- `DTHCAUS` names the fatal event, using the coded term of the
  subject's event whose outcome is death; empty when no event was
  reported fatal.
- `DTHDT` is the date the subject died: the start date of the fatal
  event, or, when no event was reported fatal, the death date
  recorded in demographics; empty when neither exists.

**Note:** cause and event date come from a single fatal event, so a
death recorded only in demographics gives a date with no cause: a death
never collected as an event has no event term to name.

**Standard:** ADaM | **Domain:** ADAE
