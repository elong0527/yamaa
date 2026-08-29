# ADaM ADSL: derive a chain of population flags

This example uses sample DM and EX data and a `yamaa` specification to derive
one row per subject:

- `TRTSDT` is the subject's first exposure date and `RANDDT` their
  randomization date;
- `RANDFL` is `Y` for a subject who was randomized, and `ITTFL` follows it;
- `SAFFL` is `Y` for a subject who has a treatment start date;
- `POPFL` is `Y` only for a subject who is in both the safety and the
  intent-to-treat populations;
- `AGEGR1` is the age band, under 65 or 65 and over, and `AGERNK` ranks
  subjects within the study by age.

Each flag rests on the one before it, so the chain runs from a collected date
through to the combined population flag.

The variables are written down in the reverse of that order, with `POPFL`
first and the subject identifier last. The order they are written in has no
effect on the result, which is what this example is here to show; a real ADSL
would not be laid out this way. `RANDFL` is used along the way but is not part
of the output.
