# Reference solution for the yamaa benchmark sdtm-dm-metadata (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
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
    USUBJID = paste(STUDYID, SITEID, SUBJID, sep = "-"),
    AGE = suppressWarnings(as.integer(AGE)),
    AGEU = "YEARS"
  ) |>
  select(DOMAIN, STUDYID, USUBJID, SUBJID, SITEID, AGE, AGEU, SEX, COUNTRY)

dir.create("/app/output", showWarnings = FALSE)
write_csv(dm, "/app/output/dm.csv", na = "")
