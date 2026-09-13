# ADaM ADSL: exposure duration in weeks and months

This example derives whole-week and whole-month durations from a
start and an end date, one row per subject:

- `STDT` and `ENDT` are carried through as given: the start date and
  the end date of the exposure.
- `DURW` is the number of whole seven-day blocks between them, or
  missing when either date is absent.
- `DURM` is the number of monthly anniversaries of the start date
  falling on or before the end date, or missing when either date is
  absent.

A month anniversary keeps the start day except where the month is too
short, in which case it is the last day of the month: the anniversary
of January 31 is February 28, and of February 29 is February 28 in a
common year. An end date before the start date gives the negated count
with the dates exchanged.

[Rendered view](https://elong0527.github.io/yamaa/examples/adam-adsl-duration-weeks-months.html)
