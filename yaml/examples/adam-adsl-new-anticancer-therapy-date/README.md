# ADaM ADSL: date the subject started new anti-cancer therapy

This example uses a subject list with the concomitant medications and
procedures collected beside it to derive one record per subject:

- `TRTSDT` is the day study treatment started;
- `NACTDT` is the earliest start of an anti-cancer therapy given during the
  study, whether it was recorded as a medication or as a procedure. Therapy
  the subject received before the study does not count, and neither does a
  procedure recorded for a reason other than the cancer;
- `NACTDY` is the study day of that start, counting the first day of treatment
  as day one;
- `NACTFL` is `Y` for a subject who started such a therapy and is empty
  otherwise.

A subject whose only anti-cancer therapy predates the study looks the same as
one who never received any, because both leave the study without a date to
censor at.
