# Reference solution for the yamaa benchmark adam-advs-locf (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

vs <- read_csv(
  "/app/input/vs.csv",
  col_types = cols(AVISITN = col_integer(), AVALCOL = col_double(), .default = col_character())
)

# The collected value, otherwise the closest earlier visit with one, in
# visit-number order; a gap before the first stays missing.
advs <- vs |>
  arrange(USUBJID, PARAMCD, AVISITN) |>
  group_by(USUBJID, PARAMCD) |>
  fill(AVALCOL, .direction = "down") |>
  mutate(AVAL = AVALCOL) |>
  ungroup() |>
  select(USUBJID, PARAMCD, AVISITN, AVAL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
