# ADaM ADVS: carry forward a once-measured characteristic

This example uses long-form vital signs with subject treatment dates and a
`yamaa` specification to derive one record per collected measurement:

- `PARAMCD`, `ADT`, and `AVAL` identify the measurement, its analysis date,
  and its value;
- `TRTSDT` is the subject's treatment start date;
- `HEIGHTBL` is the latest height on or before treatment. It is repeated on
  both height and weight records so later weights retain the once-measured
  subject characteristic, and is empty on every record for a subject without
  a pre-treatment height.

Weight is collected repeatedly, while height is collected only at screening.
The two parameters share the subject-level height without treating a weight as
a candidate height.
