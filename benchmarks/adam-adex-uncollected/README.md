# Uncollected Exposure

**Goal:** summarize each subject-treatment, carrying `EXTRT`
(treatment name) through and adding `DOSECUM` (cumulative dose),
`NDOSREC` (exposure-record count), and `NDOSVAL` (recorded-dose
count).

**Input:** a subject-treatment inventory, plus exposure records
holding administered doses (`EXDOSE`) and exposure sequence
numbers (`EXSEQ`).

**Variables:**

- `DOSECUM` is the total administered dose across the treatment's
  exposure records; empty when no dose was ever recorded, while a
  recorded zero total is kept so a measured zero never reads as
  missing.
- `NDOSREC` is the number of exposure records for the treatment;
  empty when the treatment has no exposure record at all.
- `NDOSVAL` is the number of exposure records carrying a recorded
  dose; an explicitly recorded zero counts as a dose, and the
  count is zero (not empty) when records exist but every dose was
  left blank.

**Note:** a treatment whose doses were all left blank stays
distinguishable from one that was never given: the first has
exposure records with a zero `NDOSVAL`, while the second has
nothing to count and leaves all three measures empty.
