# Flag QTcF Values and Increases Beyond Categorical Limits

**Goal:** derive the criteria `CRIT1` to `CRIT5` and their flags: whether an
on-treatment Fridericia-corrected QT interval (QTcF) lies above 450, 480 or
500 milliseconds (ms), and whether its increase from baseline lies above 30
or 60 ms.

**Input:** electrocardiogram (ECG) analysis records, one per subject,
parameter and analysis visit, with the value `AVAL` in ms, the baseline flag
`ABLFL`, the on-treatment flag `ONTRTFL`, and the baseline value `BASE` and
change from baseline `CHG` already in place.

**Variables:**

- `CRIT1`, `CRIT2`, `CRIT3`: `QTcF > 450 ms`, `QTcF > 480 ms` and
  `QTcF > 500 ms`; empty when the record is not assessed or `AVAL` is
  missing.
- `CRIT1FL`, `CRIT2FL`, `CRIT3FL`: `Y` when `AVAL` is above the limit and
  `N` otherwise, so a value exactly at a limit reads `N`.
- `CRIT4`, `CRIT5`: `QTcF increase from baseline > 30 ms` and
  `QTcF increase from baseline > 60 ms`; empty when the record is not
  assessed or has no `CHG`, as without a baseline value, while its absolute
  criteria are still assessed.
- `CRIT4FL`, `CRIT5FL`: `Y` when `CHG` is above the limit and `N`
  otherwise: an increase of exactly 30 reads `N`, and a decrease never
  meets them, however large.

**Note:** only on-treatment QTcF records (`ONTRTFL` is `Y`) are assessed;
baseline records, records after the on-treatment window and other
parameters, such as the uncorrected QT interval, carry no criterion. Values
are compared unrounded, and each flag is empty exactly where its criterion
is. A baseline record marked on treatment, or a change without a baseline
value, stops the run.
