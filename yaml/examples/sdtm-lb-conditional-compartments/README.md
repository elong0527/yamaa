# SDTM LB: tell an inapplicable compartment from an uncollected sample

This example uses collected skin-biopsy data and a `yamaa` specification to
derive one record per compartment a subject actually has:

- `LBSPEC` and `LBLOC` say which compartment the record is for, lesional or
  non-lesional;
- `LBTESTCD`, `LBTEST`, `LBORRES`, and `LBORRESU` identify the test and hold
  the collected result and its unit;
- `LBSTRESN` is the numeric form of that result;
- `LBSTAT` reads `NOT DONE` when the sample was expected but never analysed,
  and is empty otherwise;
- `LBSEQ` numbers a subject's records once they all exist.

The two cases the domain has to keep apart look different in the output.
A subject with no lesion has no lesional compartment at all, so no lesional
record is produced for them. A subject who has one whose sample was not
analysed still gets a record, with an empty result and `LBSTAT` marking why.
An empty result therefore always means a sample that was expected, and a
missing record means a compartment that never existed.
