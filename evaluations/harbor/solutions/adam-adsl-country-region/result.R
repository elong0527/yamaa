# Reference solution for the yamaa benchmark adam-adsl-country-region (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm <- read_csv(
  "/app/input/dm.csv",
  col_types = cols(.default = col_character())
)

adsl <- dm |>
  mutate(
    REGION1 = case_when(
      COUNTRY %in% c("USA", "CAN") ~ "North America",
      COUNTRY %in% c("DEU", "ESP", "FRA", "GBR") ~ "Europe",
      .default = "Rest of World"
    )
  ) |>
  select(STUDYID, USUBJID, COUNTRY, REGION1)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
