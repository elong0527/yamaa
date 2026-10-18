# Reference solution for the yamaa benchmark adam-adlb-metadata (R track).
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
  GLUC = "Glucose (mg/dL)",
  CREAT = "Creatinine (umol/L)",
  HCG = "Choriogonadotropin Beta"
)
adlb <- lb |>
  transmute(
    STUDYID,
    USUBJID,
    PARAMCD = LBTESTCD,
    PARAM = unname(param[LBTESTCD]),
    AVAL = as.numeric(LBSTRESN),
    AVALC = LBSTRESC,
    AVALU = LBSTRESU
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
