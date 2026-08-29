# SDTM LB: apply external reference ranges

This example uses collected laboratory results and a test-by-sex reference
dictionary to derive one record per result:

- `LBTESTCD`, `SEX`, and `LBSTRESN` identify the result that is evaluated;
- `LBORRESU`, `LBSTNRLO`, and `LBSTNRHI` are the unit, lower limit, and upper
  limit read depends on the `SEX`;
- `LBNRIND` is `LOW`, `NORMAL`, or `HIGH` according to the result's relation
  to those limits, and is empty when the result itself is missing.

Units and ranges are not present in the collected laboratory source. The
test-and-sex combination must have one and only one reference entry.
