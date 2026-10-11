The `specs/schema-verification-log.json` report comes from the installed,
retained Python reference engine's `execute_example(..., backend="python")` on
the unchanged `benchmarks/schema-verification-log` document and inputs. Its three
rendered CSV files were compared byte for byte with the unchanged committed
goldens before retaining the report. Only the backend label and runtime/engine
identity placeholders were changed to the same fixture convention used by the
other reports. No candidate Rust execution supplies this expected data.

Native and installed Python/R tests compare the complete report, including
source reads, all source and derived cells, all four checks and the actual three
published artifacts in verification, warning and primary order. Installed tests
also retain their actual public reports after this comparison. These witnesses
qualify this document; they do not close the remaining public API or producer
graph sidecar support gates.
