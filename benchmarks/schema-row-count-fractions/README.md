# Row Count Bounds

**Goal:** bound the rows a filter matches, by count and by share: upper
and lower count bounds, upper and lower share bounds, per-group share
bounds with an inclusive edge, and a deliberately violated warning.

**Input:** eight subjects in two arms of four. Each has an `ARM`, a
`FLAG` marking it for review, and a collected `VAL`. Three are flagged
(one in arm A, two in arm B); seven carry a value and one does not.

**Checks:**

- Flagged subjects stay at or under one half: 3 of 8 do, so the check holds.
- Subjects with a collected value stay above three quarters: 7 of 8 do,
  so the check holds.
- Per arm, flagged subjects stay at or under one half: arm A has 1 of 4,
  arm B has 2 of 4, and the exact half counts as held.
- Flagged subjects number at most three: exactly 3 do, so the check
  holds and pins the inclusive count boundary.
- Subjects with a collected value number at least seven: exactly 7 do,
  so the check holds and pins the inclusive count boundary.
- Unflagged subjects stay at or under one half: 5 of 8 do not, so the
  check is violated. It is a warning, so the run still succeeds and the
  run report carries the broken check.

The run report (`adsl-checks.csv`) has one row per item the run handled
(`KIND`, `SPEC_PATH`, `NAME`): the run and its outcome (`OUTCOME`), the
subjects read and built, each output column with its missing values and
longest text, then one row per check in the order the run evaluated them.
Every row names the dataset (`ARTIFACT`). A check row carries its label
(`ID`), the rule it applies (`RULE`), its `SEVERITY`, and how many units
failed out of how many it evaluated (`COUNT`, `TOTAL`): the group check
evaluates two groups and the others the whole dataset as one unit, so no
check names a single column (`TARGET`). A held check leaves `CONDITION`
and `KEYS` empty, reports zero failures and has empty details (`{}`). The
violated warning names its condition, reports how many rows matched out of
how many in `DETAILS`, and lists one empty key (`[{}]`) because a check
over the whole dataset has no offending rows.
