# ADaM ADSL geography normalization

This fixture answers one question: how is collected country text normalized and
grouped into a region?

`COUNTRY` uppercases the source and substitutes `UNKNOWN` when it is missing.
`REGION1` maps supported country codes and sends the non-missing `UNKNOWN`
value to `Rest of World`. The rows cover lower-, upper-, and mixed-case input
plus one missing country.

Expected handler counts are one `COUNTRY.str_upper.missing` and one
`REGION1.mapping.unmapped`. Rows remain in DM order; the key is
`[STUDYID, USUBJID]`; exactly four rows are expected.
