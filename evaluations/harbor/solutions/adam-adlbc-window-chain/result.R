# Reference solution for the yamaa benchmark adam-adlbc-window-chain (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

lb <- read_csv(
  "/app/input/lb.csv",
  col_types = cols(
    LBSTRESN = col_double(),
    VISITNUM = col_integer(),
    .default = col_character()
  )
)

# Albumin results with a numeric value; a record without one gets no row,
# so the lag compares against the latest earlier visit with a result.
alb <- lb |>
  filter(LBTESTCD %in% "ALB", !is.na(LBSTRESN)) |>
  mutate(PARAMCD = "_ALB", AVISITN = VISITNUM, AVAL = LBSTRESN) |>
  arrange(STUDYID, USUBJID, AVISITN)

# The change since the previous visit with a result, and its lag: the
# previous visit's change. A zero change is a real value.
adlbc <- alb |>
  group_by(STUDYID, USUBJID) |>
  mutate(
    PREV_AVAL = lag(AVAL),
    CHG = AVAL - PREV_AVAL,
    PREV2 = lag(CHG)
  ) |>
  ungroup() |>
  select(STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, PREV_AVAL, CHG, PREV2) |>
  arrange(STUDYID, USUBJID, AVISITN)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlbc, "/app/output/adlbc.csv", na = "")
