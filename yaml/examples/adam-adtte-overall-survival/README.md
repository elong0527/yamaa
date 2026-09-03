# ADaM ADTTE: derive overall survival

This example uses randomization dates, last known alive dates, and deaths to
derive one overall-survival record per subject:

- `STARTDT` is the randomization date and `ADT` is the death date. With no
  qualifying death, `ADT` falls back to the last known alive date, and then to
  the randomization date.
- `AVAL` is the inclusive number of days from randomization through `ADT`.
- `CNSR` is zero for a death event and one for censoring, while `EVNTDESC`
  and `CNSDTDSC` state whether the record represents death or censoring.
- `SRCDOM`, `SRCVAR`, and `SRCSEQ` identify the death or demographic record
  supplying `ADT`.
