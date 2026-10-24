# Reference solution for the yamaa benchmark adam-adsl-bmi-quartile-group (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl_in <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(BMIBL = col_double(), .default = col_character())
)

# R's default quantile definition is type 7.
quartile <- function(x, p) {
  quantile(x, p, type = 7, na.rm = TRUE, names = FALSE)
}

adsl <- adsl_in |>
  group_by(STUDYID) |>
  mutate(
    BMIBLQ1 = quartile(BMIBL, 0.25),
    BMIBLMED = quartile(BMIBL, 0.5),
    BMIBLQ3 = quartile(BMIBL, 0.75)
  ) |>
  ungroup() |>
  mutate(
    BMIBLGR1 = case_when(
      is.na(BMIBL) ~ NA_character_,
      BMIBL <= BMIBLQ1 ~ "Quartile 1",
      BMIBL <= BMIBLMED ~ "Quartile 2",
      BMIBL <= BMIBLQ3 ~ "Quartile 3",
      .default = "Quartile 4"
    )
  ) |>
  select(STUDYID, USUBJID, BMIBL, BMIBLQ1, BMIBLMED, BMIBLQ3, BMIBLGR1)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
