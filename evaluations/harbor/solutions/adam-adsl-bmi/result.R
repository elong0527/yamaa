# Reference solution for the yamaa benchmark adam-adsl-bmi (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

src <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(
    HEIGHTBL = col_double(),
    WEIGHTBL = col_double(),
    .default = col_character()
  )
)

adsl <- src |>
  mutate(
    BMIBL = if_else(
      is.na(HEIGHTBL) | HEIGHTBL == 0 | is.na(WEIGHTBL),
      NA_real_,
      WEIGHTBL / ((HEIGHTBL / 100)^2)
    )
  ) |>
  select(STUDYID, USUBJID, HEIGHTBL, WEIGHTBL, BMIBL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
