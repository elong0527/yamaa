# Original-document binding findings

`binding_diagnostics.tsv` contains ten independently authored original documents
and complete failed reports, with eleven findings. Python's unchanged reference
`execute_example` produced these new expectations on 2026-10-07. Only runtime,
engine version and backend identity were normalized, consistently with the
existing original-document fixtures. No candidate output is expected truth.

The documents exercise stored-field and output-name failures, qualified numeric
references, grouped-row field access, invalid aggregate driver scope, and
forward, key and cyclic output dependencies. Each uses one declared source with
the exact bytes `ID,V\n1,2\n`. The other-driver aggregate deliberately names
`SECOND.ID` without declaring another source; the reference rejects its driver
scope after ingesting the one declared input. Valid grouped rows with multiple
declared sources remain Unsupported and are not admitted by this diagnostic move.

Rust and both installed hosts compare the complete reports, including source
tables, capture accounting, authored paths, node failures and empty publication
ledgers. Failed results retain their observations after compiled handles are
released. Repeated failed saves neither publish nor recapture.

Eleven binding causes now use the shared core registry. The core guard reaches
grouping and dependency causes through actual scope/dependency services and
the other causes through original-document compilation and binding. Column-group
scope and a missing key derivation retain component coverage; these tests do not
claim new original-document compiler scope. Existing numeric/aggregate grammar
projection is delegated to the core parser diagnostics. Window and named-selection
findings still have their earlier separate core-owned definition projection.
