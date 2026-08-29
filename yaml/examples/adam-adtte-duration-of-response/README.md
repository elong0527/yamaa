# ADaM ADTTE: derive the duration of a response

This example uses the subjects who responded, their later tumour assessments,
and their deaths to derive one duration-of-response record per responder:

- `STARTDT` is the date the response began and `ADT` the date the response
  ended, whether by an event or by the subject running out of observation;
- `AVAL` is the inclusive number of days from `STARTDT` through `ADT`;
- `CNSR` is zero for an event and one for censoring, and `EVNTDESC` says
  whether the record represents progression, death, or censoring;
- `CNSDTDSC` is empty for an event, and for a censored record says whether
  observation ended at the subject's last evaluable assessment or at the start
  of a new anti-cancer therapy;
- `SRCDOM`, `SRCVAR`, and `SRCSEQ` identify where `ADT` came from. A record
  censored at a new therapy is traced to the subject-level date, which has no
  sequence number of its own.

A new anti-cancer therapy closes the observation period: progression or death
after it cannot be the event, and a subject still under observation at that
point is censored there rather than at a later assessment. An event on the day
the therapy started still counts, because that day is inside the period. A
subject who never responded has no duration to measure and no record here.
