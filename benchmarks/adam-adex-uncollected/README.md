# Uncollected Exposure

**Goal:** summarize each subject's exposure, carrying `TRT01P` (planned
treatment) through and adding `DOSECUM` (cumulative dose), `NDOSREC`
(exposure-record count), and `NDOSVAL` (recorded-dose count).

**Input:** the subject-level analysis dataset (ADSL) carrying each
subject's planned treatment, and the SDTM exposure (EX) records holding
administered doses (`EXDOSE`) and exposure sequence numbers (`EXSEQ`).

**Variables:**

- `TRT01P` is the subject's planned treatment, so a subject who was never
  dosed still has a record.
- `DOSECUM` is the total administered dose across the subject's exposure
  records; empty when no dose was ever recorded, while a recorded zero
  total is kept so a measured zero never reads as missing.
- `NDOSREC` is the number of the subject's exposure records; empty when
  the subject has no exposure record at all.
- `NDOSVAL` is the number of exposure records carrying a recorded dose;
  an explicitly recorded zero counts as a dose, and the count is zero (not
  empty) when records exist but every dose was left blank.

**Note:** a subject whose doses were all left blank stays distinguishable
from one who was never dosed: the first has exposure records with a zero
`NDOSVAL`, while the second has nothing to count and leaves all three
measures empty.
