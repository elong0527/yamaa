# Reference solution for the yamaa benchmark adam-adqs-missed-visit-locf (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
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
    AVISITN = col_integer(),
    AVAL = col_double(),
    .default = col_character()
  )
)

# Collected scores; an empty score is left out, so it counts as missed.
collected <- qs |>
  filter(PARAMCD %in% "ACTOT", !is.na(AVAL)) |>
  left_join(select(adsl, USUBJID, EFFFL), by = "USUBJID") |>
  mutate(DTYPE = NA_character_) |>
  select(STUDYID, USUBJID, PARAMCD, AVISIT, AVISITN, AVAL, DTYPE, EFFFL)

# One LOCF record for each missed scheduled visit in the efficacy
# population; its value is the closest earlier visit with a score, where
# a collected zero carries forward and no earlier score leaves no value.
scheduled <- tibble(
  AVISIT = c("Week 8", "Week 16", "Week 24"),
  AVISITN = c(8L, 16L, 24L)
)
eff_subjects <- adsl |>
  filter(EFFFL %in% "Y") |>
  select(STUDYID, USUBJID, EFFFL)

missed_keys <- eff_subjects |>
  crossing(scheduled) |>
  anti_join(
    select(collected, USUBJID, AVISITN),
    by = c("USUBJID", "AVISITN")
  )

carried <- missed_keys |>
  left_join(
    collected |> select(USUBJID, EARLIER_VISITN = AVISITN, EARLIER_AVAL = AVAL),
    by = "USUBJID",
    relationship = "many-to-many"
  ) |>
  filter(!is.na(EARLIER_AVAL), EARLIER_VISITN < AVISITN) |>
  group_by(STUDYID, USUBJID, AVISIT, AVISITN, EFFFL) |>
  slice_max(EARLIER_VISITN, n = 1, with_ties = FALSE) |>
  ungroup() |>
  transmute(USUBJID, AVISITN, CARRIED = EARLIER_AVAL)

missed <- missed_keys |>
  left_join(carried, by = c("USUBJID", "AVISITN")) |>
  mutate(PARAMCD = "ACTOT", AVAL = CARRIED, DTYPE = "LOCF") |>
  select(STUDYID, USUBJID, PARAMCD, AVISIT, AVISITN, AVAL, DTYPE, EFFFL)

adqs <- bind_rows(collected, missed) |>
  arrange(USUBJID, AVISITN)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adqs, "/app/output/adqs.csv", na = "")
