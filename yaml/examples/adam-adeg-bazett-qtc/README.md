# ADaM ADEG: derive a Bazett-corrected QT parameter

This example uses an input ADEG dataset to derive a Bazett-corrected QT
parameter per subject and visit:

- `PARAMCD` is the source code or `QTCBR`, which fails if already present;
- `PARAM` is the source name or Bazett's rederived QTcB name;
- `AVAL` is the source result or QT divided by the square root of RR in
  seconds, suppressed if either contributor is missing or absent;
- `AVALU` is exactly `ms` for `QTCBR`, and invalid QT or RR units fail.

The formula and parameter identity follow
[`pharmaverse/admiral`](https://github.com/pharmaverse/admiral) commit
`a221ff02e368cb3e9638417678e19c0838dbe368`,
`R/derive_param_qtc.R`.
