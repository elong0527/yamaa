# Reference solution for the yamaa benchmark adam-adsl-bmi (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

src <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(
    HEIGHTCM = col_double(),
    WEIGHTKG = col_double(),
    .default = col_character()
  )
)

adsl <- src |>
  mutate(
    BMI = if_else(
      is.na(HEIGHTCM) | HEIGHTCM == 0 | is.na(WEIGHTKG),
      NA_real_,
      WEIGHTKG / ((HEIGHTCM / 100)^2)
    ),
    BMI_FN = BMI
  ) |>
  select(STUDYID, USUBJID, HEIGHTCM, WEIGHTKG, BMI, BMI_FN)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
