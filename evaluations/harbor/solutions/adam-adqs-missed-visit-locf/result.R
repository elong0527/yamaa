# Reference solution for the yamaa benchmark adam-adqs-missed-visit-locf (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(.default = col_character())
)
qs <- read_csv(
  "/app/input/qs.csv",
  col_types = cols(
    QSSEQ = col_integer(),
    QSSTRESN = col_double(),
    VISITNUM = col_integer(),
    .default = col_character()
  )
)

# Every QS record is kept, an empty score included.
collected <- qs |>
  left_join(
    select(adsl, STUDYID, USUBJID, EFFFL),
    by = c("STUDYID", "USUBJID")
  ) |>
  transmute(
    STUDYID, USUBJID,
    PARAMCD = QSTESTCD, AVISIT = VISIT, AVISITN = VISITNUM,
    AVAL = QSSTRESN, DTYPE = NA_character_, QSSEQ, EFFFL
  )

# Scores by subject and visit; a score of zero counts.
scored <- qs |>
  filter(!is.na(QSSTRESN)) |>
  select(STUDYID, USUBJID, VISITNUM, QSSTRESN, QSSEQ)

scheduled <- tibble(
  AVISIT = c("Week 8", "Week 16", "Week 24"),
  AVISITN = c(8L, 16L, 24L)
)

# Each missed scheduled visit of an efficacy subject, carried from the
# closest earlier post-baseline score; no such score adds no record.
missed <- adsl |>
  filter(EFFFL %in% "Y") |>
  select(STUDYID, USUBJID, EFFFL) |>
  crossing(scheduled) |>
  anti_join(
    scored,
    by = c("STUDYID", "USUBJID", "AVISITN" = "VISITNUM")
  )

carried <- missed |>
  inner_join(
    scored,
    by = c("STUDYID", "USUBJID"),
    relationship = "many-to-many"
  ) |>
  filter(VISITNUM > 0, VISITNUM < AVISITN) |>
  group_by(STUDYID, USUBJID, AVISITN) |>
  slice_max(VISITNUM, n = 1, with_ties = FALSE) |>
  ungroup() |>
  transmute(
    STUDYID, USUBJID, PARAMCD = "ACTOT", AVISIT, AVISITN,
    AVAL = QSSTRESN, DTYPE = "LOCF", QSSEQ, EFFFL
  )

adqs <- bind_rows(collected, carried) |>
  arrange(USUBJID, AVISITN, !is.na(DTYPE))

dir.create("/app/output", showWarnings = FALSE)
write_csv(adqs, "/app/output/adqs.csv", na = "")
