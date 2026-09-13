# Reject a cell total larger than the counter can hold

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-compute-integer-overflow.html)

**Goal:** compute `CELLTOT`, the whole number of cells in the
sample, as `CELLCNT * VOLUML` for each subject.

**Input:** laboratory records carrying the collected cell
concentration (`CELLCNT`) and the sample volume (`VOLUML`).

**Variables:**

- `CELLTOT` would contain the whole number of cells, `CELLCNT`
  multiplied by `VOLUML`, but the product exceeds the largest
  whole number the result can hold, so the run fails and no
  artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Verify the transcription and correct `CELLCNT` in the source
before running the calculation. If the verified count genuinely
cannot fit in a signed 64-bit integer, the specification cannot
represent it exactly as `int`; change the measurement unit or
representation upstream rather than allowing the multiplication
to wrap or converting the count to an approximate value.
