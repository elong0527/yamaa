# ADaM ADAE: carry each subject's death onto every event

This example uses collected adverse events and demographics with a `yamaa`
specification to derive one row per adverse event:

- `AEDECOD` is the coded event and `ASTDT` the date it started;
- `DTHFL` marks the event whose collected outcome is death. Every other
  outcome is left unmarked;
- `DTHCAUS` names the fatal event: the coded term of the subject's event whose
  outcome is death. A subject with no such event has none;
- `DTHDT` is the date the subject died: the day the fatal event started, or,
  when no event was reported fatal, the death date recorded in demographics. A
  subject with neither has none.

A subject's cause and date come from one fatal event, so an event and its own
outcome always agree. When no event carries the death, demographics supplies
the date but the cause stays empty: a death never collected as an event has no
event to name.
