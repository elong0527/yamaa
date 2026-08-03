# ADaM ADLB BDS baseline and change

The `datasets` paths provide LB and ADSL, and LB is the base that establishes
candidate analysis records. Each row definition constructs exactly one
parameter: ALT and AST map source results directly, while ALTSI derives a new
SI-unit ALT parameter. ADSL contributes treatment variables by `STUDYID` and
`USUBJID`; this enrichment must not change the number of rows.

Using one row definition per parameter is a BDS modeling convention demonstrated
by this fixture, not a general execution-engine rule.

Portable operation semantics used by this example:

- `baseline_flag` uses `date = ADT` and `reference_date = TRTSDT` to select the
  latest non-missing `ADT` on or before `TRTSDT` within `STUDYID`, `USUBJID`,
  and `PARAMCD`. Exactly one row is flagged `Y`; ties are errors.
- `baseline_value` uses `value = AVAL` and `flag = ABLFL` to copy the flagged
  baseline value to every record in its group.
- `subtract` returns `minuend - subtrahend`.
- `percent_change` returns `100 * (value - base) / base`; a zero or missing
  baseline returns missing.
- `PCHG` carries `where: "ABLFL IS NULL"`, so it is derived only on
  post-baseline records and is missing on the baseline record itself. Percent
  change at baseline is always zero and carries no information. This is the
  restricted-derivation construct in R001, and the declarative equivalent of
  wrapping the derivation in a filter.
- `multiply` consumes the pipeline value seeded by `LB.LBSTRESN` and uses
  `factor = 0.0167` to convert ALT from `U/L` to `ukat/L` for the derived ALTSI
  parameter.
- `row_number` partitions by `group_by` and sorts ascending by `order_by`.

The expected output demonstrates one-parameter-at-a-time row construction,
creation of an additional parameter from the same source records, grouping
independently by subject and parameter, treatment enrichment, baseline
propagation, and deterministic analysis sequence.
