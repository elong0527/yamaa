# Reference solution for the yamaa benchmark adam-adsl-demographics (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(stringr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)

adsl <- dm |>
  mutate(
    SEX = str_trim(coalesce(SEX, "")),
    SEX = if_else(SEX == "", "U", SEX),
    SEXN = case_when(
      SEX == "M" ~ 1L,
      SEX == "F" ~ 2L,
      SEX == "U" ~ 0L
    ),
    RACEN = case_when(
      is.na(RACE) ~ NA_integer_,
      RACE == "WHITE" ~ 1L,
      RACE == "BLACK OR AFRICAN AMERICAN" ~ 2L,
      RACE == "ASIAN" ~ 3L,
      RACE == "MULTIPLE" ~ 4L,
      .default = 99L
    ),
    .age_num = suppressWarnings(as.numeric(str_trim(coalesce(AGE, "")))),
    AGE = if_else(
      !is.na(.age_num) & floor(.age_num) == .age_num,
      as.integer(.age_num),
      NA_integer_
    ),
    AGEGR1 = case_when(
      is.na(AGE) ~ "UNKNOWN",
      AGE < 18 ~ "<18",
      AGE < 65 ~ "18-64",
      .default = ">=65"
    )
  ) |>
  select(STUDYID, USUBJID, SEX, SEXN, RACE, RACEN, AGE, AGEGR1)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
