# ADaM ADLB BDS baseline and change

The `datasets` paths provide LB and ADSL, and LB is the base that establishes
candidate analysis records. Each row definition constructs exactly one
parameter: ALT and AST map source results directly, while ALTSI derives a new
SI-unit ALT parameter. ADSL contributes treatment variables by `STUDYID` and
`USUBJID`; this enrichment must not change the number of rows.

Using one row definition per parameter is a BDS modeling convention demonstrated
by this fixture, not a general execution-engine rule.

Portable expression semantics used by this example:

- `baseline_flag` uses `date = ADT` and `reference_date = TRTSDT` to select the
  latest non-missing `ADT` on or before `TRTSDT` within `STUDYID`, `USUBJID`,
  and `PARAMCD`. Exactly one row is flagged `Y`; ties are errors.
- `baseline_value` uses `value = AVAL` and `flag = ABLFL` to copy the flagged
  baseline value to every record in its group.
- `CHG` is `compute` with `AVAL - BASE`.
- `PCHG` is `compute` with `100 * (AVAL - BASE) / NULLIF(BASE, 0)`. This is the
  formula R007 gives for the retired `percent_change` keyword, with the
  zero-baseline rule made visible: `NULLIF` is what turns a zero baseline into
  a missing percentage, rather than the rule hiding inside a keyword. Subject
  `003` has a zero baseline and shows it.
- `AVAL` for the derived ALTSI parameter is `compute` with
  `LB.LBSTRESN * 0.0167`, converting ALT from `U/L` to `ukat/L`. This is the
  only fixture where `compute` runs during row construction, so its identifier
  is qualified against the row driver exactly as `row.filter` qualifies one.
- `row_number` partitions by `group_by` and sorts ascending by `order_by`.

The expected output demonstrates one-parameter-at-a-time row construction,
creation of an additional parameter from the same source records, grouping
independently by subject and parameter, treatment enrichment, baseline
propagation, and deterministic analysis sequence.

Expected rows remain in row-template order: all `alt` rows, then `alt_si`, then
`ast`. Window calculations assign sequence values but do not reorder rows.

Subject `003` has a zero ALT baseline, so `CHG` remains defined while `PCHG` is
missing by rule. A later ALT record has a missing numeric result and is removed
by both ALT row filters before it can generate either ALT or ALTSI output.
