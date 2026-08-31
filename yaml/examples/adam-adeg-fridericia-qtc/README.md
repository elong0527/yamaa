# ADaM ADEG: derive a Fridericia-corrected QT parameter

This example uses QT and RR records to add a Fridericia-corrected QT parameter
at the same subject and visit:

- `PARAMCD` is the source parameter code or `QTCFR` for the added record;
- `PARAM` is the source parameter name or Fridericia's rederived QTcF name;
- `AVAL` is the source result or QT divided by the cube root of RR in seconds
  for `QTCFR`;
- `AVALU` is the source unit or `ms` for `QTCFR`.

The formula and parameter identity follow
[`pharmaverse/admiral`](https://github.com/pharmaverse/admiral) commit
`e32e5689d7fd03e224ddbcfc369c332c5df837d9`,
`R/derive_param_qtc.R`.
