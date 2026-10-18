# Reference solution for the yamaa benchmark sdtm-dm-xpt (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

dm_raw <- read_csv(
  "/app/input/dm_raw.csv",
  col_types = cols(.default = col_character())
)

dm <- dm_raw |>
  mutate(
    DOMAIN = "DM",
    AGE = as.integer(AGE),
    AGEU = "YEARS"
  ) |>
  select(DOMAIN, STUDYID, USUBJID, SUBJID, RFSTDTC, SITEID, AGE, AGEU, SEX,
    RACE, COUNTRY)

dir.create("/app/output", showWarnings = FALSE)
write_csv(dm, "/app/output/dm.csv", na = "")
