# SDTM DM: derive the reference dates from EX, DS, and AE

This example uses collected DM with the EX, DS, and AE domains and a `yamaa`
specification to derive one row per enrolled subject:

- `RFICDTC` is the date the subject gave informed consent, as collected;
- `RFXSTDTC` is the subject's first exposure start date and `RFXENDTC` is
  their last exposure end date;
- `RFSTDTC` is the reference start date that study day counts run from. This
  study defines it as the first exposure, so it repeats `RFXSTDTC`; the two are
  distinct variables and their equality is a study decision rather than a
  general rule;
- `RFENDTC` is the last date the subject is known to have participated: the
  latest of their last exposure end date, their last disposition event date,
  and their last adverse event end date. A subject missing one of the three
  takes the latest of the rest, and a subject missing all three has no value.

Only disposition events count toward `RFENDTC`; other disposition records do
not. A subject who was never exposed has no exposure dates and therefore no
reference start date, which is how a screen failure appears in the output.
