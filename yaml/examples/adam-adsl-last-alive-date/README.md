# ADaM ADSL: derive the last known alive date from multiple sources

This example uses ADSL, ADAE, and ADVS inputs to derive one row per subject:

- `TRTEDT` is the subject's treatment end date, already available in `ADSL`.
- `LSTALVDT` is the subject's last known alive date. It is derived as the
  latest date among the treatment end date, the subject's latest adverse event
  end date (`AENDT`), and the subject's latest vital signs date (`ADATE`).

A subject whose dates are all missing has no last known alive date. When a
subject has dates in some sources but not others, the latest of the available
dates is retained.
