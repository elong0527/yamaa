# Reference solution for the yamaa benchmark adam-adlb-closest-visit (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

raw <- read_csv(
  "/app/input/adlb_raw.csv",
  col_types = cols(
    LBSEQ = col_integer(),
    ADY = col_integer(),
    AVAL = col_double(),
    ADT = col_date(),
    .default = col_character()
  )
)

# WEEK 2 for study days 8 through 22; target day 15 and distance travel
# together, and records outside the window or with a missing day carry
# none and are never flagged.
based <- raw |>
  mutate(
    AVISIT = if_else(!is.na(ADY) & ADY >= 8 & ADY <= 22, "WEEK 2", NA_character_),
    AWTARGET = if_else(!is.na(AVISIT), 15L, NA_integer_),
    ADIST = abs(ADY - AWTARGET)
  )

# The record standing for its subject and parameter: the closest to the
# target, the later study day when equally close, the lower sequence
# number when they share the same day.
winners <- based |>
  filter(!is.na(AVISIT)) |>
  arrange(ADIST, desc(ADY), LBSEQ) |>
  distinct(STUDYID, USUBJID, PARAMCD, AVISIT, .keep_all = TRUE) |>
  transmute(STUDYID, USUBJID, PARAMCD, AVISIT, LBSEQ, ANL01FL = "Y")

adlb <- based |>
  left_join(
    winners,
    by = c("STUDYID", "USUBJID", "PARAMCD", "AVISIT", "LBSEQ")
  ) |>
  select(
    STUDYID, USUBJID, PARAMCD, LBSEQ, ADT, ADY, AVAL,
    AVISIT, AWTARGET, ADIST, ANL01FL
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adlb, "/app/output/adlb.csv", na = "")
