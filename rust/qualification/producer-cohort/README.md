# Original shared-producer cohort

`shared-producer-parquet` copies the three authored inputs byte for byte from
`python/tests/planning/test_workflow.py::test_shared_producer_executes_once_and_shares_generated_snapshot`.
The original test independently requires one producer execution, shared generated
snapshots for aliases A/B, and the exact root value `{"ID": "01"}`.

`reference.json` and `expected/` retain the complete Python `execute_example`
reference run and its published artifacts. They were captured before the native
resource-read classification correction. The corresponding pre-fix native run
matched all nodes, tables, artifacts, checks and callbacks, but incorrectly
reported three resource requests instead of the reference's one `seed.csv`
request. Generated snapshots remain native execution evidence without entering
the resource-request ledger.

Installed Python and R public Build/Save owners run the unchanged input files.
Their complete raw reports, original case identity, package provenance and saved
bytes are retained separately. The M1 comparator projects only report labels and
native metadata that the M1 schema does not represent; it compares every M1
observation and checks both publication attempts. Parquet comparison is logical
under REQ-0742; CSV comparison and repeated held-byte publication are exact.

This cohort does not supply missing function-reference truth, qualify graph
sidecars, or close the broader #1741 acceptance criteria.
