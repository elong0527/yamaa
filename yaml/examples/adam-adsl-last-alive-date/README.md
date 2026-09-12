# ADaM ADSL: derive the last known alive date from multiple sources

This example uses ADSL, ADAE, and ADVS inputs to derive one row per subject:

- `TRTEDT` is the subject's treatment end date, already available in `ADSL`.
- `LSTCNTDC` is the date of last contact as collected, which may carry only a
  year, or a year and month, or nothing at all.
- `LSTCNTDT` is that contact date completed to a day. Where a component was
  not collected it is placed as late as the collected text still allows: a
  year and month keep their month and take its final day, and a year alone
  takes the final day of December. A last known alive date is a claim that the
  subject was alive at least until then, so the latest day the collected text
  admits is the reading that claims no more than the text supports.
- `LSTALVDT` is the subject's last known alive date. It is derived as the
  latest date among the treatment end date, the completed contact date, the
  subject's latest adverse event end date (`AENDT`), and the subject's latest
  vital signs date (`ADATE`).

A completed date competes on the day it names. For `S1` the contact date was
collected as February 2025 and its final day, the 28th, is later than every
date collected in full, so it is the last known alive date. `S3` shows the
same for a year collected alone. For `S2` the contact date was collected in
full and an adverse event ends one day later, so the event date is retained
instead.

A subject whose dates are all missing has no last known alive date. When a
subject has dates in some sources but not others, the latest of the available
dates is retained.
