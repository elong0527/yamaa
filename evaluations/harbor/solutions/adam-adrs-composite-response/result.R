# Reference solution for the yamaa benchmark adam-adrs-composite-response (R track).
#
# Written from the benchmark's prompt.md and its inputs alone, as an agent
# would write it. Harbor's oracle agent runs it to check that the prompt can
# be solved and that the grader scores a correct answer 1.

library(dplyr, warn.conflicts = FALSE)
library(readr)

adsl <- read_csv(
  "/app/input/adsl.csv",
  col_types = cols(.default = col_character())
)
raw <- read_csv(
  "/app/input/adrs_raw.csv",
  col_types = cols(PCHG = col_double(), .default = col_character())
)

# The checks apply in a fixed order: a safety or discontinuation rule
# first, then a missing component, then the 75% reduction threshold.
adrs <- raw |>
  left_join(select(adsl, USUBJID, SAEFL, DCSREAS), by = "USUBJID") |>
  mutate(
    PARAMCD = "RESP75",
    PARAM = "EASI-75 Response",
    safety = SAEFL %in% "Y" | (!is.na(DCSREAS) & DCSREAS != ""),
    AVALC = case_when(
      safety ~ "NON-RESPONDER",
      is.na(PCHG) ~ "NOT EVALUABLE",
      PCHG <= -75 ~ "RESPONDER",
      .default = "NON-RESPONDER"
    ),
    ARSN = case_when(
      safety ~ "SAFETY OR DISCONTINUATION RULE",
      is.na(PCHG) ~ "COMPONENT MISSING",
      PCHG <= -75 ~ "THRESHOLD MET",
      .default = "THRESHOLD NOT MET"
    ),
    AVAL = case_when(
      AVALC == "RESPONDER" ~ 1L,
      AVALC == "NON-RESPONDER" ~ 0L
    )
  ) |>
  select(
    STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, PCHG, SAEFL, DCSREAS,
    AVALC, ARSN, AVAL
  ) |>
  arrange(USUBJID, AVISIT)

dir.create("/app/output", showWarnings = FALSE)
write_csv(adrs, "/app/output/adrs.csv", na = "")
