# Reference solution for the yamaa benchmark sdtm-lb-grading (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/lb_raw.csv",
  col_types = cols(
    LBSEQ = col_integer(),
    LBSTRESN = col_double(),
    .default = col_character()
  )
)

toxgrade <- function(test, sex, value) {
  if (is.na(value)) return(NA_character_)
  if (test == "ANC") {
    if (value < 0.5) return("4")
    if (value < 1.0) return("3")
    if (value < 1.5) return("2")
    if (value < 1.8) return("1")
    return("0")
  }
  if (test == "HGB") {
    if (!sex %in% c("M", "F")) return(NA_character_)
    if (value < 8.0) return("3")
    if (value < 10.0) return("2")
    limit <- if (sex == "M") 13.5 else 12.0
    if (value < limit) return("1")
    return("0")
  }
  NA_character_
}

lb <- raw |>
  filter(LBTESTCD %in% c("ANC", "HGB"), !(LBTESTCD == "HGB" & !SEX %in% c("M", "F"))) |>
  rowwise() |>
  mutate(LBTOXGR = toxgrade(LBTESTCD, SEX, LBSTRESN)) |>
  ungroup() |>
  mutate(DOMAIN = "LB") |>
  select(DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBTOXGR) |>
  arrange(STUDYID, USUBJID, LBSEQ)

dir.create("/app/output", showWarnings = FALSE)
write_csv(lb, "/app/output/lb.csv", na = "")
