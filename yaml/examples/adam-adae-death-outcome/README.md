# ADaM ADAE: carry each subject's death onto every event

This example uses collected adverse events and demographics with a `yamaa`
specification to derive one row per adverse event:

- `AEDECOD` is the coded event and `ASTDT` the date it started;
- `DTHFL` marks every event for a subject whose death is supported by a fatal
  event or a death date in demographics. A subject with neither is unmarked;
- `DTHCAUS` names the fatal event: the coded term of the subject's event whose
  outcome is death. A subject with no such event has none;
- `DTHDT` is the date the subject died: the day the fatal event started, or,
  when no event was reported fatal, the death date recorded in demographics. A
  subject with neither has none.

A subject's cause and event date come from one fatal event. When no event
carries the death, demographics supplies the date but the cause stays empty: a
death never collected as an event has no event to name.
