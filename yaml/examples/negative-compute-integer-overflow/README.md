# ADaM ADLB: reject a cell total larger than the counter can hold

This example uses collected cell concentrations and sample volumes to attempt
one record per subject:

- `CELLTOT` is the whole number of cells in the sample, the concentration
  multiplied by the volume.

One concentration was transcribed with far more digits than the instrument
reports, and its total exceeds the largest whole number a result can hold.
Wrapping to a negative total or drifting to the nearest representable one would
each report a count nobody measured, so the run must fail and no artifact is
accepted.

## How to fix

Verify the transcription and correct `CELLCNT` in the source before running the
derivation. If the verified count genuinely cannot fit in a signed 64-bit
integer, the specification cannot represent it exactly as `int`; change the
measurement unit or representation upstream rather than allowing the
multiplication to wrap or silently converting the count to an approximate
value.
