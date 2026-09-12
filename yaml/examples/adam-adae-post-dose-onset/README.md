# ADaM ADAE: classify an event by the moment it started

This example uses sample AE and ADSL data and a `yamaa` specification to derive
one row per adverse event:

- `AETERM` is the reported event and `ASTDTM` is the moment it started, held as
  a date and a time of day rather than a day alone. A start collected to the
  minute is written back to the second;
- `TRTSDTM` is the moment the subject's first dose was given, carried across
  from ADSL. A subject with no ADSL record leaves it empty;
- `TRTEMFL` marks an event as treatment-emergent when it started at or after
  that moment. An event that started earlier is not marked, and an event whose
  own start or whose first dose was never collected is left unmarked;
- `AOCCFL` marks the subject's earliest treatment-emergent event. Earliest
  means by the moment of onset, and the lower sequence number settles two
  events recorded at the same second.

Deciding emergence at the moment rather than at the day is the point of the
example: an event that started earlier on the day of the first dose and one
that started later the same day fall on opposite sides of the rule, and a start
date alone cannot tell them apart.
