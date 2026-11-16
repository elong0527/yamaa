# Derive a Chain of Population Flags

**Goal:** derive the full analysis set flag `FASFL` from the
safety flag `SAFFL` and the intent-to-treat flag `ITTFL`, plus
the dates those flags read, first exposure date `TRTSDT` and
randomization date `RANDDT`, and the collected age `AGE` with its
age group `AGEGR1`.

**Input:** one demographic record per subject carrying age and
the randomization date, and exposure records carrying the
exposure start date.

**Variables:**

- `FASFL` holds `Y` only when both `SAFFL` and `ITTFL` hold `Y`,
  and `N` otherwise, marking subjects who were randomized and
  started treatment.
- `SAFFL` holds `Y` when the subject has a first exposure date,
  and `N` otherwise, marking subjects who started treatment.
- `ITTFL` holds `Y` when the subject was randomized, and `N`
  otherwise, marking subjects in the intent-to-treat
  population.
- `TRTSDT` holds the earliest dated exposure start for the
  subject, and stays empty when no exposure record carries a
  date.
- `RANDDT` holds the collected randomization date, and stays
  empty when the subject was not randomized.
- `AGEGR1` holds `<65` when `AGE` is below 65 and `>=65` when it
  is 65 or above; a subject with no age stops the run.

**Note:** the dates are settled before the flags that read them:
the safety and intent-to-treat flags come from their dates, and
the full analysis set flag comes from both flags.
