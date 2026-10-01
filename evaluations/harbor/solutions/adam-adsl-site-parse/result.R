# Reference solution for the yamaa benchmark adam-adsl-site-parse (R track).
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
    # The middle segment when the identifier holds two dashes with
    # exactly four digits after the last one.
    SITEIDP = str_match(USUBJID, "^YAMAA-([^-]+)-[0-9]{4}$")[, 2],
    SITEID = coalesce(SITEIDP, SITEID),
    SITEID = if_else(is.na(SITEID), "UNKNOWN", SITEID),
    SUBJREF = if_else(is.na(SUBJID), "UNKNOWN", paste0(SITEID, ":", SUBJID))
  ) |>
  select(STUDYID, USUBJID, SUBJID, SITEIDP, SITEID, SUBJREF) |>
  arrange(USUBJID)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
