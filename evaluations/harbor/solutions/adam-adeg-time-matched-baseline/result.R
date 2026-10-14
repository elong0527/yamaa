# Reference solution for the yamaa benchmark adam-adeg-time-matched-baseline (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adeg <- read_csv(
  "/app/input/adeg_raw.csv",
  col_types = cols(
    APERIOD = col_integer(), APERDAY = col_integer(),
    ATPTN = col_double(), AVAL = col_double(),
    .default = col_character()
  ),
  na = ""
)

# Day -1 of each period is that period's baseline day.
baseline <- adeg |>
  filter(APERDAY == -1) |>
  select(STUDYID, USUBJID, PARAMCD, APERIOD, ATPTN, BASE = AVAL)

# Time-matched: the same subject, parameter, period and nominal time on the
# period's own baseline day; nothing stands in for a missing one.
adeg_out <- adeg |>
  left_join(
    baseline,
    by = c("STUDYID", "USUBJID", "PARAMCD", "APERIOD", "ATPTN"),
    relationship = "many-to-one"
  ) |>
  mutate(
    ABLFL = if_else(APERDAY == -1, "Y", NA_character_),
    CHG = if_else(APERDAY == -1, NA_real_, AVAL - BASE),
    BASETYPE = paste0("Period ", APERIOD, " Baseline Day ", ATPT)
  ) |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN, AVAL,
    AVALU, ABLFL, BASE, CHG, BASETYPE
  )

dir.create("/app/output", showWarnings = FALSE)
write_csv(adeg_out, "/app/output/adeg.csv", na = "")
