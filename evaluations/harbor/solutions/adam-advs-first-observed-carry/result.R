# Reference solution for the yamaa benchmark adam-advs-first-observed-carry (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

vs <- read_csv(
  "/app/input/vs.csv",
  col_types = cols(AVISITN = col_integer(), AVAL = col_double(), .default = col_character())
)

# The first visit with a result for each subject and parameter.
firsts <- vs |>
  filter(!is.na(AVAL)) |>
  arrange(STUDYID, USUBJID, PARAMCD, AVISITN) |>
  distinct(STUDYID, USUBJID, PARAMCD, .keep_all = TRUE) |>
  select(STUDYID, USUBJID, PARAMCD, FIRST_AVAL = AVAL, FIRST_VISIT = AVISITN)

# The first result, repeated on that and every later visit; visits before
# any result stay missing.
advs <- vs |>
  left_join(firsts, by = c("STUDYID", "USUBJID", "PARAMCD")) |>
  mutate(BASEVAL = if_else(!is.na(FIRST_VISIT) & AVISITN >= FIRST_VISIT, FIRST_AVAL, NA_real_)) |>
  arrange(USUBJID, PARAMCD, AVISITN) |>
  select(STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, BASEVAL)

dir.create("/app/output", showWarnings = FALSE)
write_csv(advs, "/app/output/advs.csv", na = "")
