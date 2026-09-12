# ADaM ADVS: assign records to analysis windows

This example uses a pre-derived ADVS slice and a `yamaa` specification to
derive one row per record:

- `ADT`, `ADY`, and `AVAL` are the record's analysis date, its study day, and
  the value measured, all carried through as given;
- `VISIT` and `VISITNUM` are what the site recorded, kept unchanged;
- `AVISIT` and `AVISITN` are the analysis window the record falls in, and its
  order, decided from the study day rather than from what the site called the
  visit. Windows run from the lower bound up to but not including the next, so
  every day belongs to exactly one. A visit the site left unscheduled still
  lands in whichever window its day falls in, and one recorded past the last
  boundary falls in the open-ended final window;
- `ANL01FL` marks the record that represents its subject in that window: the
  earliest by study day, and the lower sequence number if two share a day. A
  record with no analysis date belongs to no window and is never marked.

Because study day has no day zero, the baseline window can only hold day one.
