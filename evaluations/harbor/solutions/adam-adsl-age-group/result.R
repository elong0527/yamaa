# Reference solution for the yamaa benchmark adam-adsl-age-group (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(AGE = col_double(), .default = col_character())
)

adsl <- dm |>
  mutate(
    AGEGR1 = case_when(
      is.na(AGE) ~ "Missing",
      AGE < 18 ~ "<18",
      AGE <= 64 ~ "18-64",
      .default = ">64"
    ),
    AGEGR1N = case_when(
      AGEGR1 == "<18" ~ 1L,
      AGEGR1 == "18-64" ~ 2L,
      AGEGR1 == ">64" ~ 3L
    )
  ) |>
  select(STUDYID, USUBJID, AGE, AGEU, AGEGR1, AGEGR1N)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
