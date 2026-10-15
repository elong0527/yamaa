# Apply the stated policy to qualified laboratory results

**Goal:** retain the reported result in `AVALC`, produce numeric `AVAL`, and
identify substituted results with `DTYPE`.

**Input:** Study Data Tabulation Model (SDTM) laboratory results for human
immunodeficiency virus ribonucleic acid (HIV RNA), reported as text in
copies/mL, including inequalities and missing results.

**Variables:**

- `PARAMCD` identifies the laboratory test, carried from `LBTESTCD`.
- `AVALC` retains the reported text; blank when no result was reported.
- `AVAL` follows the study's one-unit policy: subtract 1 from a strict
  `<` limit and add 1 to a strict `>` limit. Inclusive `<=` and `>=` limits
  use the boundary itself. An ordinary number or `=` result retains its
  numeric value; a missing result stays missing, distinct from zero.
- `AVALU` carries the standard unit, copies/mL.
- `DTYPE` is `IMPUTATION` for an inequality result; blank otherwise.

**Note:** the one-unit offset is an authored study convention. It does not
claim to recover the unmeasured result. Only nonnegative decimal numbers,
optionally preceded by `<`, `>`, `<=`, `>=`, or `=`, are accepted; other
reported text stops the run.
