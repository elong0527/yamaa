# Reference solution for the yamaa benchmark adam-adlb-parameter-names (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

lb <- read_csv(
  "/app/input/lb.csv",
  col_types = cols(.default = col_character())
)

param <- c(
  ALT = "Alanine Aminotransferase (U/L)",
  AST = "Aspartate Aminotransferase (U/L)"
)
adlb <- lb |>
  transmute(
    STUDYID,
    USUBJID,
    PARAMCD = LBTESTCD,
    PARAM = unname(param[LBTESTCD]),
    AVISITN = as.integer(VISITNUM),
    AVISIT = case_when(
      AVISITN == 1 ~ "Baseline",
      AVISITN == 2 ~ "Week 2",
      .default = "Unscheduled"
    ),
    AVAL = as.numeric(LBSTRESN)
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
