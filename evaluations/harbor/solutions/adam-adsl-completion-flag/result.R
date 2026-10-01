# Reference solution for the yamaa benchmark adam-adsl-completion-flag (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl_raw <- read_csv(
  "/app/input/adsl_raw.csv",
  col_types = cols(TRTSDT = col_date(), .default = col_character())
)
ds <- read_csv(
  "/app/input/ds.csv",
  col_types = cols(.default = col_character())
)

completed <- ds |>
  filter(
    DSCAT %in% "DISPOSITION EVENT",
    EPOCH %in% "FOLLOW-UP",
    DSDECOD %in% "COMPLETED"
  ) |>
  distinct(STUDYID, USUBJID) |>
  mutate(.completed = "Y")

adsl <- adsl_raw |>
  left_join(completed, by = c("STUDYID", "USUBJID")) |>
  mutate(COMPLFL = if_else(!is.na(.completed), "Y", "N")) |>
  select(STUDYID, USUBJID, TRTSDT, COMPLFL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adsl, "/app/output/adsl.csv", na = "")
