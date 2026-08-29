# SDTM LB: build one record per collected lab result

This example uses collected long-form lab data and a `yamaa` specification to
derive one record per calcium and creatinine result:

- `LBTESTCD` and `LBTEST` identify the test, and `LBORRES` and `LBORRESU` keep
  the collected result and its unit exactly as reported;
- `LBSTRESN` and `LBSTRESU` are the result in standard form. A result reported
  as text rather than a number, such as `NOT DONE`, keeps its text in
  `LBORRES` and leaves `LBSTRESN` empty rather than failing;
- `LBDTC` is the collection date for the visit the result belongs to;
- `LBSEQ` numbers a subject's records in date and test order, and is assigned
  once all records exist so that it is unique within the subject.

A test with no collected value produces no record at all, so a subject's
records are the results actually reported rather than one per scheduled test.
