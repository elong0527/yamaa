# Give Each Analysis Period an End-of-Period Status

**Goal:** give each subject an end-of-period status and a discontinuation
reason for both periods of a two-period study: `EOP01STT`, `DCP01RS`,
`EOP02STT`, and `DCP02RS`.

**Input:** demographics rows identifying each subject, exposure records
(`EX`) carrying the epoch (`EPOCH`) of each dose, and disposition records
(`DS`) with category (`DSCAT`), epoch, reported term (`DSTERM`), coded term
(`DSDECOD`), and date (`DSSTDTC`). Period 1 is the `TREATMENT 1` epoch and
period 2 the `TREATMENT 2` epoch.

**Variables:**

- `EOP01STT` reads the subject's period 1 disposition event, the record
  whose category is `DISPOSITION EVENT` and whose epoch is `TREATMENT 1`:
  `COMPLETED` when its coded term is `COMPLETED`, `DISCONTINUED` for any
  other coded term, `ONGOING` when the subject received a dose in period 1
  but has no such event yet, and empty when the subject never received a
  dose in period 1.
- `DCP01RS` is the coded term of that event, such as `LOST TO FOLLOW-UP`,
  when `EOP01STT` is `DISCONTINUED`; never the reported text; empty
  otherwise.
- `EOP02STT` and `DCP02RS` are the same for period 2, read from the
  `TREATMENT 2` epoch.

**Note:** the epoch alone places an event in a period; its date, complete
or partial, plays no part. Records in other categories, such as a site
transfer, and events in other epochs, such as a withdrawal during the
washout or the end-of-study event, never set a period's status. Two
disposition events for one period, or one for a period the subject never
received a dose in, stop the run.
