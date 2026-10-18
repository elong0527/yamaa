# Reference solution for the yamaa benchmark adam-adsl-xpt (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl_raw <- read_csv(
  "/app/input/adsl_raw.csv",
  col_types = cols(.default = col_character())
)

# A collected time without seconds names second zero.
adsl <- adsl_raw |>
  mutate(
    AGE = as.integer(AGE),
    HEIGHTBL = as.numeric(HEIGHTBL),
    TRTSDTM = ifelse(nchar(TRTSDTC) == 16, paste0(TRTSDTC, ":00"), TRTSDTC),
    TRTSDT = substr(TRTSDTC, 1, 10)
  ) |>
  select(STUDYID, USUBJID, TRT01A, AGE, HEIGHTBL, TRTSDT, TRTSDTM)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
