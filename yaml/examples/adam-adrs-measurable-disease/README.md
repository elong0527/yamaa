# ADaM ADRS: derive measurable disease at baseline

Derives one measurable-disease existence flag parameter per ADSL subject from
TU tumor identification records:

- `AVALC`: `Y` when the subject has at least one target disease assessment at
  screening; otherwise `N`
- `AVAL`: `1` for `Y` and `0` for `N`

The derivation follows
[`pharmaverse/admiral`](https://github.com/pharmaverse/admiral) commit
`e32e5689d7fd03e224ddbcfc369c332c5df837d9`,
`R/derive_param_exist_flag.R`.
