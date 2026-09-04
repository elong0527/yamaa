# ADaM ADTTE: derive the time to first adverse event

This example uses treatment start dates, end of study dates, and adverse
events to derive one time-to-first-adverse-event record per subject:

- `STARTDT` is the treatment start date. `ADT` is the selected adverse event
  date, or the end of study date when no event exists. The selected date is
  clamped to `STARTDT` when it predates treatment start, and a subject with
  neither source date produces no output row.
- `AVAL` is the inclusive number of days from treatment start through `ADT`.
- `CNSR` is zero for an adverse event and one for censoring, while `EVNTDESC`
  states whether the record represents an adverse event or end-of-study
  censoring.
- `SRCDOM`, `SRCVAR`, and `SRCSEQ` identify the selected event or censor
  source.

The time-to-event precedence, earliest event, and tie-breaking behavior follow
[`pharmaverse/admiral`](https://github.com/pharmaverse/admiral) commit
`01669e09c5a49064826ab1c1f470835b71c1c27f`, `R/derive_param_tte.R`.
