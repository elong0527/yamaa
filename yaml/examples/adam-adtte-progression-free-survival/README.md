# ADaM ADTTE: derive progression-free survival

This example uses randomization dates, tumour assessments, and deaths to derive
one progression-free-survival record per subject:

- `STARTDT` is randomization and `ADT` is the earliest eligible progression or
  death date. With neither event, `ADT` is the last adequate assessment;
- `AVAL` is the inclusive number of days from randomization through `ADT`;
- `CNSR` is zero for an event and one for censoring, while `EVNTDESC` states
  whether the record represents progression, death, or censoring;
- `SRCDOM`, `SRCVAR`, and `SRCSEQ` identify the assessment or disposition
  record supplying `ADT`.

Assessments made ineligible by subsequent therapy cannot become an event or a
censoring record. The expected endpoint is calculated from the assessment and
death inputs, not copied from the simulated endpoint form.
