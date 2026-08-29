# SDTM LB: consolidate four collection forms into one dataset

This example uses collected serum, skin-biopsy, saliva, and tape-strip data and
a `yamaa` specification to derive one record per result actually reported:

- `LBTESTCD`, `LBTEST`, and `LBCAT` identify the analyte and its category, and
  `LBSPEC` and `LBLOC` say which specimen it came from and, where the form
  distinguishes them, whether the site was lesional or non-lesional. The same
  analyte collected on two forms is told apart by these rather than by the test
  code;
- `LBORRES` and `LBORRESU` hold the result and unit as collected;
- `LBSTRESC`, `LBSTRESN`, and `LBSTRESU` hold it in standard form. A result
  reported as text rather than a number, such as `NOT DONE`, keeps its text and
  leaves the numeric form empty;
- `LBSTAT` reads `NOT DONE` for such a result;
- `LBDTC` is the collection date recorded on the form the result came from, so
  results collected on the same day through different forms keep their own
  dates. A form collected more than once at the same visit keeps each
  occurrence's own date;
- `VISIT`, `VISITNUM`, and `LBSEQ` place the record: the visit it belongs to
  and its order within the subject, numbered once all records exist.

A result that was not reported produces no record, whether the form was never
applicable to that subject or the field was simply left blank. A numeric zero is
a real result and is kept.
