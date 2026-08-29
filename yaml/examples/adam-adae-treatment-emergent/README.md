# ADaM ADAE: classify an event as treatment-emergent

This example uses sample AE and ADSL data and a `yamaa` specification to derive
one row per adverse event:

- `TRTA`, `TRTSDT`, and `TRTEDT` are the subject's treatment and the dates it
  ran between, carried across from ADSL. A subject with no ADSL record keeps
  their events and leaves all three empty;
- `ASTDT` is the event start date;
- `TRTEMFL` marks an event as treatment-emergent when its start date falls
  within the treatment period, counting both the first and the last day. An
  event before treatment started, one after it ended, and one belonging to a
  subject with no treatment dates are all left unflagged.

Treating both boundaries as inside the period is this study's rule rather than
a universal one.
