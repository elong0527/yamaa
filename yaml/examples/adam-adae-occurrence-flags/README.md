# ADaM ADAE: flag the first occurrence at three levels

This example uses a pre-classified ADAE slice and a `yamaa` specification to
derive one row per adverse event:

- `AEBODSYS` and `AEDECOD` are the body system and preferred term the event was
  coded to, `ASTDT` is its start date, and `TRTEMFL` says whether it is
  treatment-emergent;
- `AOCCFL`, `AOCCSFL`, and `AOCCPFL` each mark a subject's earliest
  treatment-emergent event: the first overall, the first within each body
  system, and the first within each preferred term. Earliest means by start
  date, and the lower sequence number settles two events on the same day.

Only treatment-emergent events are eligible, so an event that is not one is
never marked at any level, even when it is the subject's earliest event.

The three levels nest: an event marked as the subject's first is necessarily
also the first in its body system and its preferred term, while a term
occurring later carries only the preferred-term flag.
