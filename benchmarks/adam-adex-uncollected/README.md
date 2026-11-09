# Uncollected Exposure

**Goal:** summarize each subject's exposure, carrying `TRT01P` (planned
treatment) through and adding `DOSECUM` (cumulative dose), `NDOSREC`
(exposure-record count), and `NDOSVAL` (recorded-dose count).

**Input:** SDTM demographics (DM) carrying each subject's planned arm
(`ARM`), and the SDTM exposure (EX) records holding
administered doses (`EXDOSE`) and exposure sequence numbers (`EXSEQ`).
Every recorded dose in this case is expressed in milligrams (`EXDOSU=mg`),
including any additional treatment records. Mixed-unit dose normalization
is outside this case's contract; doses in other units must be standardized
before supplying EX.

**Variables:**

- `TRT01P` is the subject's planned treatment from `DM.ARM`, so a subject
  who was never dosed still has a record.
- `DOSECUM` is the total administered dose in milligrams across the
  subject's exposure records; empty when no dose was ever recorded, while
  a recorded zero total is kept so a measured zero never reads as missing.
- `NDOSREC` is the number of the subject's exposure records; empty when
  the subject has no exposure record at all.
- `NDOSVAL` is the number of exposure records carrying a recorded dose;
  an explicitly recorded zero counts as a dose, and the count is zero (not
  empty) when records exist but every dose was left blank.

**Note:** a subject whose doses were all left blank stays distinguishable
from one who was never dosed: the first has exposure records with a zero
`NDOSVAL`, while the second has nothing to count and leaves all three
measures empty.
