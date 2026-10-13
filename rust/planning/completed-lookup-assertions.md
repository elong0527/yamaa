# Completed-row selected assertions

This closes the remaining selected-field site in #2005. A specification that
constructs `VALUE: P.X` can already read the declared kept record. Before this
change, changing an otherwise valid assertion from `VALUE = 2` to `P.X = 2`
produces `unknown_field` / REQ-0405. REQ-0183 and REQ-0185 permit that declared
lookup for the completed row.

The compiler holds declared assertion lookup names until
the existing source-schema binding step. It binds their names to completed
columns or admitted selected fields. Unknown ordinary names keep their original
finding; a missing declared lookup field produces the same cause once its schema
is available. A valid `when` retains its type checkpoint before a later invalid
`require`, and earlier verification records retain their order and failures.

Verification borrows the original completed candidates, including their template,
feeding members and selection identity, and the attempt's existing intermediate
Run. A kept record or derived relation is selected/materialized by the same native
context already used for case operands. The owned output table is formed after
verification. No table or host error is cloned, and no new evaluator or endpoint
is introduced.

Type admission uses compiled column and selected-field types with nonmissing
representatives. It has no study-cell, selector or function-call authority. Actual
rows evaluate `when` followed by `require`, including exempt rows, as REQ-0383
requires. Existing AND/OR operand evaluation remains eager. Existing function
missing-input short circuiting is unchanged. Predicate selected reads preserve
the existing policy that their inherited recovery does not add handler counts.

The shared operand context requires an assignment-shaped path carrier. Assertion
admission excludes raw source, collection and joined bindings; the carrier's
column/expression do not participate in selected resolution. Original opaque
selected-cell failures retain `intermediates[0]` and their original source
coordinates/payload. No-match failures retain their completed output keys. Neither
invents a destination column identity.

The independent fixture `completed_lookup_assertions.json` contains 17 complete
Check/Build findings and ordered verification records, typed output cells,
capture/handler ledgers and exact output CSV bytes. It covers both predicate
sites, shared selected fields, record/group/mixed templates, absent selection,
eager WHEN/REQUIRE and logical operands, missing fields, ordered declaration/type
failures, zero-output admission and warning severity with its required warning
log. Both installed hosts consume the same authored truth through portable ordinal
directories. No existing fixture or benchmark expectation is changed.

Six independently authored typed engine controls additionally pin actual cell
traces, fresh attempts, non-cloneable payload identity and failure order, zero
donor cells on empty-output admission, and derived donor callback counts. One
materialization executes two callbacks per attempt; subsequent WHEN/REQUIRE reads
execute none. Empty output executes none.

The older #1980 wording `source.condition` is outside the current source grammar.
`condition` belongs to `flag_class`; it remains an unknown source field. The
grouped-filter historical private fixture disposition and every earlier source
and numerical qualification head remain unchanged by this assertion work.
