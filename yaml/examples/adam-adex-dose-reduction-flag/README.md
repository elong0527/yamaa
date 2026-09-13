# Flag a dose reduction from the previous administration

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adex-dose-reduction-flag.html)

**Goal:** flag each exposure administration whose dose was reduced
from the previous administration, adding `DOSREDFL`.

**Input:** exposure (EX) records carrying sequence number (`EXSEQ`),
treatment start (`EXSTDTM`), and collected dose (`EXDOSE`).

**Variables:**

- `DOSREDFL`: `Y` when the current dose is lower than the
  immediately preceding dose in time for the same subject and both
  doses are positive; blank otherwise.

**Note:** the comparison runs in chronological treatment-start
order within each subject, so the first administration has no
predecessor and stays blank; a pause in dosing (a zero dose)
neither flags a reduction nor counts as a reduced-from dose.

**Standard:** ADaM | **Domain:** ADEX
