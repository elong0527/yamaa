# ADaM ADAE: impute partial dates

This example uses sample AE and ADSL data and a `yamaa` specification to derive
one row per adverse event:

- `AETERM` is the reported term for the event, and `AESTDTC` its start date as
  collected, which may carry only a year, or a year and month, or nothing
  usable at all;
- `TRTSDT` is the subject's treatment start date;
- `ASTDT` is the analysis start date. A date collected in full is used as it
  stands, and one missing only its day is placed on the 15th. A year-only
  source remains without an analysis date rather than supplying both month and
  day. A collected value that is not a date and an uncollected value also give
  no analysis date;
- a completed date is never placed before `TRTSDT`. Where the 15th would fall
  earlier, the event is moved forward to the treatment start, which the
  collected month still allows. Where the collected month ends before the
  treatment start, no day it allows can satisfy that, so the event is left
  without an analysis date rather than moved into a month nobody recorded. A
  date collected in full is left exactly as collected even when it falls
  earlier, because there is nothing about it to choose;
- `ASTDTC` is the same analysis date written as text;
- `ASTDTF` is `D` when the day was supplied. It is empty when the date was
  collected in full and when no analysis date could be formed. It is read from
  `ASTDT` itself rather than from the collected text, so the flag and the date
  it describes cannot disagree;
- `TRTEMFL` marks an event as treatment-emergent when it starts on or after
  `TRTSDT`. A supplied day decides this exactly as a collected one would, so
  `ASTDTF` is what tells a reader which of these events rested on one.

The two subjects differ in what their treatment start allows. For
`CATH-UCSD-0001` the 15th of a collected month is already on or after
treatment start, so every completed date keeps it. For `CATH-UCSD-0002`
treatment starts on 20 March: the March event moves from the 15th to the 20th,
and the February event is left without an analysis date because February ends
first.
