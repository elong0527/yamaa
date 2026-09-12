# ADaM ADTTE: derive overall survival

This example uses randomization dates, last known alive dates, and deaths to
derive one overall-survival record per subject:

- `STARTDT` is the randomization date. `ADT` is the qualifying death date,
  clamped to `STARTDT` when it predates randomization. With no death, `ADT` is
  the later of last known alive and randomization; a tie uses randomization.
- `AVAL` is the inclusive number of days from randomization through `ADT`.
- `CNSR` is zero for a death event and one for censoring, while `EVNTDESC`
  and `CNSDTDSC` state whether the record represents death or censoring.
- `SRCDOM`, `SRCVAR`, and `SRCSEQ` identify the selected event or censor
  source.
