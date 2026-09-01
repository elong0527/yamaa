# ADaM ADEG: derive an RR interval

This example uses ADEG HR records to preserve collected records and add one RRR
parameter record per subject and analysis visit:

- `PARAMCD` is the source parameter code or `RRR` for the added record;
- `PARAM` is the source parameter name or the rederived RR duration name;
- `AVAL` is the source result or 60000 divided by HR for `RRR`. A missing or
  zero HR adds no `RRR` record;
- `AVALU` is the source unit or `ms` for `RRR`.

The formula and parameter identity follow
[`pharmaverse/admiral`](https://github.com/pharmaverse/admiral) commit
`e32e5689d7fd03e224ddbcfc369c332c5df837d9`, `R/derive_param_rr.R`.
