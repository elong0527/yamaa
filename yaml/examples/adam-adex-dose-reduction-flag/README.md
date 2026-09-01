# ADaM ADEX: derive a dose reduction flag

This example reads exposure records and returns one row per exposure record:

- `EXSEQ` identifies the collected exposure record;
- `EXSTDTM` and `EXDOSE` retain its treatment start and collected dose;
- `DOSREDFL` is `Y` when the current and immediately preceding
  chronological doses are positive and the current dose is lower; otherwise,
  it is missing.

## Provenance

- Upstream repository: `pharmaverse/admiral`
- Source path: `R/derive_var_joined_exist_flag.R`
- Immutable commit SHA: `e32e5689d7fd03e224ddbcfc369c332c5df837d9`
