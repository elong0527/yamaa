# Count Adverse Events From Each Subject's Own First Dose

**Goal:** count each subject's treatment-emergent adverse events
(`TEAENUM`), the events that start on or after the first dose.

**Input:** one `spec.yaml` over subject-level records carrying the date of
first dose (`TRTSDT`), and adverse events carrying a sequence number and a
start date.

**Variables:**

- `TEAENUM` is the number of the subject's adverse events that start on or
  after the subject's first dose date, that day included. It is blank when
  no event does, and when the subject has no first dose date.

**Note:** the first dose date comes from the subject-level record that
built the row; the output carries no copy of it.
