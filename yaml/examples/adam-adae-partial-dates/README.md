# ADaM ADAE: impute partial dates

This example uses sample AE and ADSL data and a `yamaa` specification to derive
one row per adverse event:

- `AETERM` is the reported term for the event, and `AESTDTC` its start date as
  collected, which may carry only a year, or a year and month, or nothing
  usable at all;
- `ASTDT` is the analysis start date. A date collected in full is used as it
  stands; one missing its day is placed on the 15th, and one missing its month
  as well is placed in June. A collected value that is not a date and an
  uncollected value both give no analysis date;
- `ASTDTC` is the same analysis date written as text;
- `TRTSDT` is the subject's treatment start date, and `TRTEMFL` marks an event
  as treatment-emergent when it starts on or after it. An imputed day decides
  this exactly as a collected one would, and nothing in the output says which
  events rested on an imputed component.
