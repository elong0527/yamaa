# Count Treatment Days From a Saved Demographics File

**Goal:** derive each subject's first and last treatment dates (`TRTSDT`,
`TRTEDT`) and the number of days on treatment (`TRTDURD`) in ADSL, from the
demographics (DM) file that an earlier step builds and saves.

**Input:** two spec files, run in order:

- `spec_dm.yaml` builds DM from one comma-separated record per subject and
  saves it as `dm.csv`, carrying the first (`RFXSTDT`) and last (`RFXENDT`)
  treatment dates as dates;
- `spec_adsl.yaml` is the entry: it reads the saved `dm.csv`, whose fields
  hold what the spec that wrote it declares, so its dates are dates without
  being declared again.

**Variables:**

- `TRTSDT` and `TRTEDT` are DM's first and last treatment dates;
- `TRTDURD` counts the days from `TRTSDT` to `TRTEDT`, both included, so a
  single treatment day gives one; it stays empty when either date is
  missing.

**Note:** a comma-separated file stores every value as text. The dates in
`dm.csv` are still dates when the entry reads it back, because the spec
that wrote it declares them so, and the day count works on them directly.
