# Reject Integer Overflow

**Goal:** compute `CELLTOT`, the whole number of cells in the
sample, as `CELLCNT * VOLUML` for each subject.

**Input:** laboratory records carrying the collected cell
concentration (`CELLCNT`) and the sample volume (`VOLUML`) for
four subjects: an ordinary count and volume, a product exactly
at the largest whole number the result can hold
(9,223,372,036,854,775,807), a missing concentration, and a
product past that largest whole number.

**Variables:**

- `CELLTOT` would contain the whole number of cells, `CELLCNT`
  multiplied by `VOLUML`, and missing when either is missing. A
  product beyond the largest whole number the result can hold
  stops the run, and no artifact is accepted.

## How to fix

Verify the transcription and correct `CELLCNT` in the source
before running the calculation. If the verified count genuinely
cannot fit in a signed 64-bit integer, the specification cannot
represent it exactly as `int`; change the measurement unit or
representation upstream rather than allowing the multiplication
to wrap or converting the count to an approximate value.
