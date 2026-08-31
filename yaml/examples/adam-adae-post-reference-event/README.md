# ADaM ADAE: flag an event after a specific reference event

Derives one analysis record per AE event from AE, retaining `AEDECOD` and
`ASTDT`:

- `AFTCOVFL` is `Y` for an event ordered strictly after the subject's first
  COVID-19 event; otherwise it is missing, including when the subject has no
  COVID-19 event

## Provenance

- Upstream repository: `pharmaverse/admiral`
- Path: `R/derive_var_relative_flag.R`
- Commit: `e32e5689d7fd03e224ddbcfc369c332c5df837d9`
