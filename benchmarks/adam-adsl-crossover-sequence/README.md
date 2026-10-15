# Derive planned period treatments and sequence from the trial design

**Goal:** derive each subject's planned treatment for the two periods of
a crossover (`TRT01P`, `TRT02P`) and the planned sequence of treatments
(`TRTSEQP`), with one record per subject.

**Input:** demographics (DM) listing each subject with the planned arm
code (`ARMCD`); the trial arms design (TA) listing each arm's elements
in planned order (`TAETORD`) with the element code (`ETCD`) and its epoch
(`EPOCH`); and the trial elements (TE) giving each element's name
(`ELEMENT`).

**Variables:**

- `TRT01P` is the name of the first treatment element of the subject's
  planned arm, taking the arm's elements in planned order and counting
  only those in an epoch whose name begins with `TREATMENT`, so
  screening, washout and follow-up elements are skipped; empty when the
  subject has no planned arm code.
- `TRT02P` is the name of the second treatment element of the planned
  arm, chosen the same way; empty when the subject has no planned arm
  code.
- `TRTSEQP` is `TRT01P` and `TRT02P` joined by `/` in period order, such
  as `Placebo/Vitamin D3`; empty when the subject has no planned arm
  code.

**Note:** the planned values follow the planned arm code only, never the
actual arm or the arm description, so a subject given the other order
keeps the planned one, and a screen failure or a subject never assigned
to an arm has all three empty.
