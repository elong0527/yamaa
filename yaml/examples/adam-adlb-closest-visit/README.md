# ADaM ADLB: select the record closest to a window's target day

This example uses a pre-derived ADLB slice and a `yamaa` specification to
derive one row per record:

- `ADT`, `ADY`, and `AVAL` are the record's analysis date, its study day, and
  the value measured;
- `AVISIT` is the analysis window the record falls in, and `AWTARGET` the day
  that window aims at. A record outside every window has neither;
- `ADIST` is how far the record's study day is from that target, in days and
  without direction;
- `ANL01FL` marks the record that represents its subject in the window: the one
  closest to the target, and the later of two equally close. A record outside
  every window is never marked.

Publishing `AWTARGET` and `ADIST` alongside the flag lets a reader see why a
record was chosen rather than take the flag on trust.
