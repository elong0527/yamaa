# Original predicate diagnostic truth

The malformed window filter `ID >` was independently authored and captured from the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. Its complete failed report pins REQ-0188, the authored filter path, character position 4 and one captured/ingested `ID,V\n1,2\n` source. Rust and both installed hosts preserve this report after dropping preparation, without recapture or successful save.

The shared core diagnostic also represents the existing named-selection causes and predicate parser causes, with typed context and canonical registry mappings. Existing independent lookup reports and locked predicate syntax fixtures retain their exact observations. No old expectation is regenerated.

Two additional original window filters now have independently captured complete
failed reports in `predicate_context_diagnostics.tsv`. They were captured on
2026-10-11 from the unchanged Python reference at
`976c38b27c6fd7dbb33e6f03bbb219a9596ed21c`, using the same admitted document and
`ID,V\n1,2\n` source as the earlier malformed filter. Only report identity fields
(`backend`, `runtime_version`, `engine_version`) select the native fixture
identity; diagnostics, capture history, source tables and failed node outcome
come from reference execution. No existing expectation was regenerated.

Core now projects original-document grammar findings as the reference
`predicate` and character `position` context. Rich regex and temporal facts stay
on the retained immutable parser cause and in standalone syntax responses. The
Rust complete-report tests retain both results after dropping preparation,
repeat failed-save gates and observe exactly one source capture. This new
fixture does not claim a fresh installed Python/R host qualification.

The three previously documented forms now have these statuses:

| Filter | Unchanged reference observation | Native status |
| --- | --- | --- |
| `ID LIKE '%' ESCAPE 'xx'` | REQ-0188, position 19; predicate/position context | Approved REQ-0191 under #1973 item 6; position 19 and predicate/position context are preserved. The historical reference citation remains recorded. |
| `STR_CONTAINS(ID, '(')` | REQ-1244, position 17; predicate/position context | Matches the new complete original report; standalone syntax retains regex byte/reason |
| `ID = DATE '2026-02-30'` | REQ-0188, position 10; predicate/position context | Matches the new complete original report; standalone syntax retains temporal kind/error |

REQ-0191 owns invalid ESCAPE literals in `rules/operations/predicates.md`, and the
locked predicate syntax fixture already preserves that native mapping. The
unchanged Python reference was rerun on 2026-10-11 and still reports REQ-0188.
The delegated decision in #1973 item 6 selects REQ-0191 for new shared execution.
This disposes of the citation difference without rewriting either locked
observation. REQ-0192 now explicitly describes parse-time `invalid_predicate`
under REQ-0188 and retained temporal cause/standalone context, preserving the
original rejected date's complete report. Installed qualification remains a
separate gate; this record does not turn a pending tuple into a pass.
