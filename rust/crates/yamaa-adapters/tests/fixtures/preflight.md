# Original-document preflight truth

`preflight.tsv` contains five authored JSON documents (valid YAML) and sixteen
ordered findings from the independent Python reference at main
`c813b08d98f002e827c2e9fbe1832e965297d24a`. They exercise missing and duplicate
derivations, unknown row columns and keys, domain/input collisions, unavailable
drivers, conflicting row construction and empty/duplicate/foreign groups.

Truth was recorded on 2026-10-07 using `load_specification` against the shipped
schema and `preflight_execution`; each `ExecutionPlanningError.diagnostics`
record was serialized with `model_dump(mode="json")`. Only the existing
`specification/prototype` transport envelope was added. Neither native bindings
nor candidate diagnostics were used to obtain expectations. No study file was
opened. Existing benchmark expectations remain unchanged.

Both installed hosts and the original-document adapter compare each complete
envelope. The core registry test also reaches all twelve preflight causes through
actual compilation, including the existing Parquet redundant-type requirement.
Null requirements and unavailable dataset values are intentional reference truth;
empty lists and ordered repeated/listed row names must remain intact. This is
preparation evidence, not public API or `shared_run` inventory qualification.
