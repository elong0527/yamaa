# ADaM ADSL: select the final subject disposition from DS

This example uses sample DM and DS data and a `yamaa` specification to derive
one row per subject:

- `EOSDT` is the subject's latest disposition event date. Only disposition
  events count; protocol milestones do not;
- `EOSDECOD` and `EOSREAS` are the decoded term and the reason recorded on the
  subject's last disposition event, taken from the same set of records as
  `EOSDT` so the three cannot describe different events. When two events fall
  on the same day the higher sequence number is the later one;
- `EOSSTT` is `COMPLETED` when the last event says so, `DISCONTINUED` when
  there is any other event, and `ONGOING` when the subject has no disposition
  event at all;
- `DCSREAS` repeats the reason only for a discontinued subject, and is empty
  for one who completed or is ongoing.
