# Original predicate diagnostic truth

The malformed window filter `ID >` was independently authored and captured from the unchanged Python reference on 2026-10-07, based on merged `c6bbdc1ae315c6b9c86ff8688cc56ed765f4093c`. Its complete failed report pins REQ-0188, the authored filter path, character position 4 and one captured/ingested `ID,V\n1,2\n` source. Rust and both installed hosts preserve this report after dropping preparation, without recapture or successful save.

The shared core diagnostic also represents the existing named-selection causes and predicate parser causes, with typed context and canonical registry mappings. Existing independent lookup reports and locked predicate syntax fixtures retain their exact observations. No old expectation is regenerated.

Three additional independently observed original window filters expose existing reference/native gaps and are **not qualified by this fixture**:

| Filter | Unchanged reference observation | Existing native observation |
| --- | --- | --- |
| `ID LIKE '%' ESCAPE 'xx'` | REQ-0188, position 19; predicate/position context | REQ-0191, position 19; predicate/position context |
| `STR_CONTAINS(ID, '(')` | REQ-1244, position 17; predicate/position context | REQ-1244, position 17; additional regex byte/reason context |
| `ID = DATE '2026-02-30'` | REQ-0188, position 10; predicate/position context | REQ-0188, position 10; additional temporal kind/error context |

REQ-0191 owns invalid ESCAPE literals in `rules/operations/predicates.md`, and the locked predicate syntax fixture already preserves that native mapping. REQ-0192's temporal wording remains a separately recorded grammar gap. This projection change preserves those existing mappings and richer parser context; it does not rewrite locked syntax truth to force original-report parity. These gaps require reconciliation before the affected original forms can be qualified or a public frontend can cut over.
