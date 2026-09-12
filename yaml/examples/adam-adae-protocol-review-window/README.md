# ADaM ADAE: flag adverse events for protocol review

The collected adverse events produce one analysis row per reported event:

- `ASTDT` is the separately collected calendar date. `ASTDT2` is the calendar
  date of the collected local datetime; either is missing when its own source
  is missing.
- `REVIEWFL` is `Y` when an event falls in either review window, its term
  begins with the literal `INF_` prefix, and its review score is at least
  -1.5; otherwise it is `N`, including when neither review date is known.
