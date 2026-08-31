# ADaM ADSL: derive a dose adjustment flag from multiple sources

This example demonstrates deriving a subject-level dose adjustment flag
by checking multiple domain datasets (EX, EC, and FA) for any record
indicating a dose adjustment:

- `DOSADJFL` is `Y` if the subject has a dose adjustment reported in `EX`,
  `EC`, or `FA`. If the subject appears in at least one of these datasets
  but no dose adjustment is reported, the value is `N`. If the subject is
  absent from all sources, the value is missing.

## Robustness

The derivation provides these guarantees:
- **Condition precedence**: A qualifying record yields `Y` even if the same subject has non-qualifying or missing records in the same or other sources.
- **Negative completion**: A subject with source records that all lack a qualifying condition receives `N`.
- **Null safety**: A subject absent from all sources receives a missing value.
- **Composite-key isolation**: Matches require both `STUDYID` and `USUBJID`.
- **Orphan rejection**: Qualifying source records without a matching parent row create no output.

## Provenance

- Upstream repository: `pharmaverse/admiral`
- Source path: `R/derive_var_merged_ef_msrc.R`
- Immutable commit SHA: `e32e5689d7fd03e224ddbcfc369c332c5df837d9`
