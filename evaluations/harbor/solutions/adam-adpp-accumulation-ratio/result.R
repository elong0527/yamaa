# Reference solution for the yamaa benchmark adam-adpp-accumulation-ratio (R track).
#
# Written from the benchmark's full prompt and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)
library(tidyr)

pp <- read_csv(
  "/app/input/pp.csv",
  col_types = cols(PPSTRESN = col_double(), .default = col_character())
)
adsl <- read_csv("/app/input/adsl.csv", col_types = cols(.default = col_character()))

# Every PP record, a parameter that was not calculated included.
collected <- pp |>
  transmute(
    STUDYID, USUBJID,
    PARAMCD = PPTESTCD,
    PARAM = PPTEST,
    AVISIT = VISIT,
    AVAL = PPSTRESN,
    AVALU = PPSTRESU
  )

# The steady-state profile is the last planned dosing day of the arm.
steady_state <- tibble(ARMCD = c("A", "B"), SSVISIT = c("DAY 7", "DAY 5"))

subjects <- pp |>
  distinct(STUDYID, USUBJID) |>
  inner_join(select(adsl, STUDYID, USUBJID, ARMCD), by = c("STUDYID", "USUBJID")) |>
  inner_join(steady_state, by = "ARMCD")

params <- tibble(
  PARAMCD = c("ARAUC", "ARCMAX"),
  PARAM = c("Accumulation Ratio AUCTAU", "Accumulation Ratio Cmax"),
  PPTESTCD = c("AUCTAU", "CMAX")
)

steady <- pp |>
  select(STUDYID, USUBJID, PPTESTCD, VISIT, SSVALUE = PPSTRESN)
first <- pp |>
  filter(VISIT == "DAY 1") |>
  select(STUDYID, USUBJID, PPTESTCD, FIRSTVALUE = PPSTRESN)

# A ratio needs both values; it sits at the steady-state visit.
ratios <- params |>
  cross_join(subjects) |>
  inner_join(
    steady,
    by = c("STUDYID", "USUBJID", "PPTESTCD", "SSVISIT" = "VISIT")
  ) |>
  inner_join(first, by = c("STUDYID", "USUBJID", "PPTESTCD")) |>
  filter(!is.na(SSVALUE), !is.na(FIRSTVALUE), FIRSTVALUE != 0) |>
  transmute(
    STUDYID, USUBJID, PARAMCD, PARAM,
    AVISIT = SSVISIT,
    AVAL = SSVALUE / FIRSTVALUE,
    AVALU = "RATIO"
  )

adpp <- bind_rows(collected, ratios) |>
  select(STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adpp, "/app/output/adpp.csv", na = "")
