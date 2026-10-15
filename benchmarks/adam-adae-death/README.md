# Death Carryforward

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

**Note:** when a subject has more than one fatal event, the latest by
start date supplies the cause and date, and a tie on start date goes to
the highest `AESEQ`. A death recorded only in demographics gives a date
but no cause, because there is no event term to name.
