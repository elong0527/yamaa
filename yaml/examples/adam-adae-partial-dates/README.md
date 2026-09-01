# ADaM ADAE: impute partial dates

This example uses sample AE and ADSL data and a `yamaa` specification to derive
one row per adverse event:

- `AETERM` is the reported term for the event, and `AESTDTC` its start date as
  collected, which may carry only a year, or a year and month, or nothing
  usable at all;
- `ASTDT` is the analysis start date. A date collected in full is used as it
  stands, and one missing only its day is placed on the 15th. A year-only
  source remains without an analysis date rather than supplying both month and
  day. A collected value that is not a date and an uncollected value also give
  no analysis date;
- `ASTDTC` is the same analysis date written as text;
- `ASTDTF` is `D` when the day was supplied. It is empty when the date was
  collected in full and when no analysis date could be formed;
- `TRTSDT` is the subject's treatment start date, and `TRTEMFL` marks an event
  as treatment-emergent when it starts on or after it. A supplied day decides
  this exactly as a collected one would, so `ASTDTF` is what tells a reader
  which of these events rested on one.
