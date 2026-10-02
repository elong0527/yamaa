# R submission validation

`load_study_document()` reads YAML 1.2, validates the canonical submission
schemas, loads dataset specifications, and validates metadata and terminology
bindings. The installed schema copies are compared byte for byte with the
repository's canonical files by the package tests.

`validate_submission_metadata()` and `validate_study_terminology()` consume
resolved specification lists and return portable diagnostics. An engine with
inheritance resolution supplies its resolving function as
`specification_loader` to `load_study_document()`. The default loader reads
standalone specifications and rejects unresolved parents explicitly.
The default reader currently uses R's native integer representation. Integer
declarations outside that range fail validation; full 64-bit scalar loading and
automatic engine integration remain work for the replacement R engine.

At the completed-column verification boundary, call
`check_submission_column(data, column, specification, document)`. It checks
non-extensible item lists, applies value-level codelist overrides, and reports
the column, row keys, and offending values. Missing values pass, and external
and extensible lists add no constraint. Inventory vocabularies are checked
against the study's declared dataset identifiers.
