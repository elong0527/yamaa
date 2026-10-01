# Reference solution for the yamaa benchmark adam-adlbc-row-window (R track).
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

# Build the change parameters from albumin and bilirubin; any other test
# produces no rows. A record with no numeric result gets no row.
adbc <- lb |>
  filter(LBTESTCD %in% c("ALB", "BILI"), !is.na(LBSTRESN), !is.na(VISITNUM)) |>
  mutate(
    PARAMCD = case_when(
      LBTESTCD == "ALB" ~ "_ALB",
      LBTESTCD == "BILI" ~ "_BILI"
    ),
    AVISITN = VISITNUM,
    AVAL = LBSTRESN
  ) |>
  arrange(USUBJID, PARAMCD, AVISITN) |>
  group_by(USUBJID, PARAMCD) |>
  mutate(
    PREV_AVAL = lag(AVAL),
    CHG = AVAL - PREV_AVAL
  ) |>
  ungroup() |>
  select(STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, PREV_AVAL, CHG)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adbc, "/app/output/adlbc.csv", na = "")
