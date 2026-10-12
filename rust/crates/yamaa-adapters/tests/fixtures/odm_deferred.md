# Deferred ODM defaults

Eight independently authored reports and CSV artifacts retain REQ-1265–1278
semantics when a column default depends on a completed row-number window.
They cover record occurrence scope, missing items, group hierarchy scope,
selectors followed by source-only predicates, an unselected CASE branch,
a nested string operand, and complete ambiguity context with the authored
row-template identity. Record occurrences use the completed sequence key.

Expected reports and CSV bytes were authored independently of the evaluator.
Each replay compares the complete report and source ledger and publishes a
successful held artifact repeatedly. Existing fourteen ODM fixtures remain
unchanged. The same fixture is staged for installed Python and standalone R.
